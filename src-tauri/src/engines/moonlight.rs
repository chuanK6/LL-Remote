use crate::{
    engines::{command, EnginePaths},
    error::{AppError, ErrorCode, Result},
    model::{Metrics, PublishedApp, StreamPreferences},
};
use serde_json::{json, Value};
use std::{
    borrow::Cow,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Child,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub fn stream_args(
    address: &str,
    app: &str,
    preferences: &StreamPreferences,
) -> Result<Vec<String>> {
    super::sunshine::validate_address(address)?;
    preferences.validate()?;
    if app.is_empty() || app.chars().count() > 256 || app.contains(['\0', '\r', '\n']) {
        return Err(AppError::new(ErrorCode::InvalidInput, "远程应用名称无效。"));
    }
    let codec = match preferences.codec.as_str() {
        "h264" => "H.264",
        "hevc" => "HEVC",
        "av1" => "AV1",
        _ => "auto",
    };
    Ok(vec![
        "stream".into(),
        address.into(),
        app.into(),
        "--no-quit-after".into(),
        "--resolution".into(),
        format!("{}x{}", preferences.width, preferences.height),
        "--fps".into(),
        preferences.fps.to_string(),
        "--bitrate".into(),
        preferences.bitrate_kbps.to_string(),
        "--display-mode".into(),
        preferences.window_mode.clone(),
        "--video-codec".into(),
        codec.into(),
        "--audio-config".into(),
        preferences.audio_config.clone(),
        "--capture-system-keys".into(),
        preferences.capture_keys.clone(),
        if preferences.mouse_mode == "office" {
            "--absolute-mouse"
        } else {
            "--no-absolute-mouse"
        }
        .into(),
        if preferences.host_audio {
            "--audio-on-host"
        } else {
            "--no-audio-on-host"
        }
        .into(),
        if preferences.gamepad {
            "--background-gamepad"
        } else {
            "--no-background-gamepad"
        }
        .into(),
        "--no-mute-on-focus-loss".into(),
    ])
}

pub enum AppList {
    Paired(Vec<PublishedApp>),
    NeedsPairing,
}

fn decode_cli_output(bytes: &[u8]) -> Cow<'_, str> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Cow::Borrowed(text);
    }
    // Qt's qPrintable uses the Windows ANSI code page for localized CLI text.
    #[cfg(windows)]
    if let Some(text) = crate::platform::decode_system_ansi(bytes) {
        return Cow::Owned(text);
    }
    String::from_utf8_lossy(bytes)
}

pub struct StreamProcess {
    pub child: Child,
    pub verified: Arc<AtomicBool>,
}
#[derive(Clone)]
pub struct MoonlightAdapter {
    pub paths: EnginePaths,
    data_directory: Option<std::path::PathBuf>,
}
impl MoonlightAdapter {
    fn settings_path(&self) -> Option<std::path::PathBuf> {
        let base = self.data_directory.clone().or_else(|| {
            let base = self.paths.root.join("engines/Moonlight");
            base.join("portable.dat").exists().then_some(base)
        });
        base.map(|base| base.join("Moonlight Game Streaming Project/Moonlight.ini"))
    }
    pub fn read_preferences(&self, base: &StreamPreferences) -> Result<Option<StreamPreferences>> {
        super::moonlight_preferences::read(self.settings_path().as_deref(), base)
    }
    pub fn write_preferences(&self, preferences: &StreamPreferences) -> Result<()> {
        super::moonlight_preferences::write(self.settings_path().as_deref(), preferences)
    }
    pub fn new(paths: EnginePaths, data_directory: Option<std::path::PathBuf>) -> Self {
        Self {
            paths,
            data_directory,
        }
    }
    async fn cli(
        &self,
        args: &[String],
        host_uuid: Option<&str>,
        cancel: &CancellationToken,
        limit: Duration,
    ) -> Result<std::process::Output> {
        let executable = self.paths.executable(&self.paths.manifest.moonlight)?;
        let mut process = command(&executable, executable.parent().unwrap());
        process.env("LLREMOTE_EXACT_ADDRESS", "1");
        process.env("LLREMOTE_TARGET_UUID", host_uuid.unwrap_or_default());
        if let Some(directory) = &self.data_directory {
            std::fs::create_dir_all(directory)?;
            process.env("LLREMOTE_MOONLIGHT_DATA_DIR", directory);
        }
        if args.first().is_some_and(|arg| arg == "pair") {
            process.env("LLREMOTE_HEADLESS_PAIR", "1");
        }
        process
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        tokio::select! { _ = cancel.cancelled() => Err(AppError::new(ErrorCode::Canceled, "连接已取消。")), value = tokio::time::timeout(limit, process.output()) => value.map_err(|_| AppError::new(ErrorCode::Timeout, "Moonlight 响应超时。"))?.map_err(|_| AppError::new(ErrorCode::StreamFailed, "Moonlight 无法启动，请检查随包依赖。")) }
    }
    pub async fn list_apps(&self, address: &str, host_uuid: Option<&str>, cancel: &CancellationToken) -> Result<AppList> {
        super::sunshine::validate_address(address)?;
        let output = self
            .cli(
                &["list".into(), address.into()],
                host_uuid,
                cancel,
                Duration::from_secs(35),
            )
            .await?;
        if output.stdout.len() > 1024 * 1024 || output.stderr.len() > 1024 * 1024 {
            return Err(AppError::new(
                ErrorCode::StreamFailed,
                "Moonlight 返回的应用列表过大。",
            ));
        }
        if output.status.success() {
            let apps = decode_cli_output(&output.stdout)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .enumerate()
                .map(|(index, name)| PublishedApp {
                    id: index.to_string(),
                    name: name.trim().into(),
                })
                .collect();
            Ok(AppList::Paired(apps))
        } else {
            let error = Zeroizing::new(decode_cli_output(&output.stderr).to_lowercase());
            if error.contains("not been paired")
                || error.contains("未配对")
                || error.contains("尚未配对")
            {
                Ok(AppList::NeedsPairing)
            } else {
                let (reason, code, message) = if error.contains("server certificate mismatch")
                    || error.contains("tls initialization failed")
                    || error.contains("ssl handshake failed")
                {
                    ("tls", ErrorCode::StreamFailed, "Moonlight 无法建立受信任的 Sunshine 加密连接。请核对被控端是否更换了 Sunshine 证书或安装目录。")
                } else if error.contains("failed to connect to ")
                    || error.contains("request timed out")
                {
                    ("timeout", ErrorCode::Timeout, "Moonlight 等待对方 Sunshine 响应超时，请检查被控端 Sunshine 和组网连接。")
                } else if error.contains("connection refused") {
                    ("refused", ErrorCode::NetworkFailed, "对方拒绝了 Moonlight 的连接，请检查被控端 Sunshine 是否仍在运行。")
                } else if error.contains("invalid applist xml")
                    || error.contains("malformed xml")
                {
                    ("invalid_response", ErrorCode::StreamFailed, "Sunshine 返回的应用列表格式无效，请检查被控端的 Sunshine 日志。")
                } else {
                    ("unknown", ErrorCode::StreamFailed, "Moonlight 无法读取远程应用，请检查主机与配对状态。")
                };
                tracing::warn!(reason, exit_code = ?output.status.code(), "Moonlight could not list remote applications");
                Err(AppError::new(code, message))
            }
        }
    }
    pub async fn apps_after_pairing(
        &self,
        address: &str,
        host_uuid: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<PublishedApp>> {
        let first = self.list_apps(address, Some(host_uuid), cancel).await;
        match first {
            Ok(AppList::Paired(apps)) => return Ok(apps),
            Err(error) if error.code != ErrorCode::NetworkFailed => return Err(error),
            _ => {}
        }
        tokio::select! {
            _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Canceled, "连接已取消。")),
            _ = tokio::time::sleep(Duration::from_millis(750)) => {}
        }
        match self.list_apps(address, Some(host_uuid), cancel).await? {
            AppList::Paired(apps) => Ok(apps),
            AppList::NeedsPairing => Err(AppError::new(
                ErrorCode::PairingFailed,
                "已提交 PIN，但 Sunshine 仍未授权此电脑。请检查被控端的 Sunshine 已配对设备。",
            )),
        }
    }
    pub async fn pair(&self, address: &str, host_uuid: Option<&str>, pin: &str, cancel: &CancellationToken) -> Result<()> {
        super::sunshine::validate_address(address)?;
        if pin.len() != 4 || !pin.bytes().all(|b| b.is_ascii_digit()) {
            return Err(AppError::new(ErrorCode::InvalidInput, "配对 PIN 无效。"));
        }
        let output = self
            .cli(
                &["pair".into(), address.into(), "--pin".into(), pin.into()],
                host_uuid,
                cancel,
                Duration::from_secs(120),
            )
            .await?;
        if !output.status.success() {
            return Err(AppError::new(
                ErrorCode::PairingFailed,
                "首次配对未完成，请确认对方已填写正确 PIN。",
            ));
        }
        Ok(())
    }
    pub fn spawn_stream(
        &self,
        address: &str,
        host_uuid: Option<&str>,
        app: &str,
        preferences: &StreamPreferences,
        pipe_name: &str,
        token: &str,
    ) -> Result<StreamProcess> {
        let executable = self.paths.executable(&self.paths.manifest.moonlight)?;
        let mut process = command(&executable, executable.parent().unwrap());
        if let Some(directory) = &self.data_directory {
            std::fs::create_dir_all(directory)?;
            process.env("LLREMOTE_MOONLIGHT_DATA_DIR", directory);
        }
        process
            .args(stream_args(address, app, preferences)?)
            .env("LLREMOTE_EXACT_ADDRESS", "stream")
            .env("LLREMOTE_TARGET_UUID", host_uuid.unwrap_or_default())
            .env("LLREMOTE_PIPE_NAME", pipe_name)
            .env("LLREMOTE_SESSION_TOKEN", token)
            .env("LLREMOTE_AUDIO_DEVICE", &preferences.audio_device)
            .env("LLREMOTE_AUDIO_VOLUME", preferences.volume.to_string())
            .env(
                "LLREMOTE_AUDIO_MUTED",
                if preferences.muted { "1" } else { "0" },
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = process.spawn().map_err(|_| {
            AppError::new(
                ErrorCode::StreamFailed,
                "Moonlight 启动失败，请检查完整运行资源。",
            )
        })?;
        let verified = Arc::new(AtomicBool::new(false));
        if let Some(stdout) = child.stdout.take() {
            let ready = verified.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if line == "LLREMOTE_STREAM_READY" {
                        ready.store(true, Ordering::SeqCst);
                    }
                    // Never forward native stdout or request data to the app log.
                }
            });
        }
        Ok(StreamProcess { child, verified })
    }
    pub fn open_settings(&self) -> Result<Option<String>> {
        let executable = self.paths.executable(&self.paths.manifest.moonlight)?;
        let mut process = command(&executable, executable.parent().unwrap());
        if let Some(directory) = &self.data_directory {
            std::fs::create_dir_all(directory)?;
            process.env("LLREMOTE_MOONLIGHT_DATA_DIR", directory);
        }
        if self.paths.manifest.moonlight.extension_protocol == 1 {
            process.arg("--ll-settings");
        }
        process
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| AppError::new(ErrorCode::StreamFailed, "无法打开 Moonlight 原设置。"))?;
        Ok((self.paths.manifest.moonlight.extension_protocol != 1)
            .then(|| "当前 Moonlight 尚未包含设置直达扩展，已打开原界面。".into()))
    }
    pub async fn ipc(
        &self,
        pipe_name: &str,
        token: &str,
        action: &str,
        payload: Value,
        expected_pid: u32,
    ) -> Result<Value> {
        if self.paths.manifest.moonlight.extension_protocol != 1 {
            return Err(AppError::new(
                ErrorCode::ProtocolUnsupported,
                "此 Moonlight 内核未提供 LL 会话扩展。",
            ));
        }
        if !["status", "apply", "disconnect", "input"].contains(&action) {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "会话操作不在允许范围内。",
            ));
        }
        if !pipe_name.starts_with("LLRemote-")
            || !pipe_name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "会话管道名称无效。",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use tokio::net::windows::named_pipe::ClientOptions;
            let path = format!("\\\\.\\pipe\\{pipe_name}");
            let mut pipe = ClientOptions::new().open(&path).map_err(|_| {
                AppError::new(ErrorCode::StreamFailed, "Moonlight 会话接口尚未就绪。")
            })?;
            let mut owner = 0;
            let verified = unsafe {
                windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(
                    pipe.as_raw_handle() as _,
                    &mut owner,
                )
            };
            if verified == 0 || owner != expected_pid {
                return Err(AppError::new(
                    ErrorCode::PermissionDenied,
                    "会话管道不属于本次启动的 Moonlight。",
                ));
            }
            let bytes = Zeroizing::new(serde_json::to_vec(
                &json!({ "version": 1, "token": token, "action": action, "payload": payload }),
            )?);
            let response = tokio::time::timeout(Duration::from_secs(3), async {
                pipe.write_all(&bytes).await?;
                pipe.write_all(b"\n").await?;
                let mut reader = BufReader::new(pipe);
                let mut response = vec![];
                reader.read_until(b'\n', &mut response).await?;
                if response.len() > 65536 {
                    return Err(AppError::new(ErrorCode::StreamFailed, "会话接口数据过大。"));
                }
                serde_json::from_slice::<Value>(&response).map_err(AppError::from)
            })
            .await
            .map_err(|_| AppError::new(ErrorCode::Timeout, "会话设置响应超时。"))??;
            if response.get("version") != Some(&json!(1))
                || response.get("ok") != Some(&Value::Bool(true))
            {
                return Err(AppError::new(
                    ErrorCode::ProtocolUnsupported,
                    "Moonlight 会话接口拒绝此请求。",
                ));
            }
            Ok(response)
        }
        #[cfg(not(windows))]
        {
            let _ = (token, payload, expected_pid);
            Err(AppError::new(
                ErrorCode::SystemFailed,
                "会话管道仅支持 Windows。",
            ))
        }
    }
    pub fn metrics(response: &Value) -> Metrics {
        response
            .get("metrics")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }
    pub fn executable(&self) -> Result<std::path::PathBuf> {
        self.paths.executable(&self.paths.manifest.moonlight)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionPatch;
    #[test]
    fn independent_changes_always_pass_full_quality_snapshot() {
        let original = StreamPreferences {
            bitrate_kbps: 42000,
            fps: 120,
            ..StreamPreferences::default()
        };
        let updated = original
            .patched(&SessionPatch::Resolution {
                width: 2560,
                height: 1440,
            })
            .unwrap();
        let args = stream_args("10.3.0.1", "Desktop", &updated).unwrap();
        assert!(args.windows(2).any(|v| v == ["--bitrate", "42000"]));
        assert!(args.windows(2).any(|v| v == ["--fps", "120"]));
        assert!(args.contains(&"--no-quit-after".to_string()));
        let mouse = updated
            .patched(&SessionPatch::Mouse {
                mode: "game".into(),
            })
            .unwrap();
        assert_eq!(
            (mouse.width, mouse.height, mouse.fps, mouse.bitrate_kbps),
            (2560, 1440, 120, 42000)
        );
        assert_eq!(mouse.audio_config, original.audio_config);
    }
}
