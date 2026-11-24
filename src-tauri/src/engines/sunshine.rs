use crate::{
    engines::{command, EnginePaths},
    error::{AppError, ErrorCode, Result},
    model::{IncomingControl, PendingPairing, PublishedApp, SunshineSettings, SunshineStatus},
    platform,
    storage::{atomic_write, SecretRecord, Vault},
};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime},
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    process::Child,
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

pub fn config_value(path: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().trim_matches('"').to_owned())
        })
}
fn rebase_managed_config(config: &Path, base: &Path) -> Result<()> {
    if !config.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(config)?;
    let mut updated = String::with_capacity(text.len());
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let ending = &line[body.len()..];
        let Some((name, value)) = body.split_once('=') else {
            updated.push_str(line);
            continue;
        };
        if body.trim_start().starts_with('#')
            || !matches!(
                name.trim(),
                "credentials_file" | "file_state" | "file_apps" | "cert" | "pkey" | "log_path"
            )
        {
            updated.push_str(line);
            continue;
        }
        let value = value.trim();
        let (stored, trailing, quoted) = if let Some(quoted) = value.strip_prefix('"') {
            let Some(end) = quoted.find('"') else {
                updated.push_str(line);
                continue;
            };
            (&quoted[..end], &quoted[end + 1..], true)
        } else if let Some((path, _)) = value.split_once(" #") {
            (path, &value[path.len()..], false)
        } else {
            (value, "", false)
        };
        let normalized = stored.replace('\\', "/");
        let marker = "/engines/sunshine/";
        let Some(index) = normalized.to_ascii_lowercase().rfind(marker) else {
            updated.push_str(line);
            continue;
        };
        if !Path::new(stored).is_absolute() {
            updated.push_str(line);
            continue;
        }
        let suffix = Path::new(&normalized[index + marker.len()..]);
        if suffix.as_os_str().is_empty()
            || suffix
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            updated.push_str(line);
            continue;
        }
        let current = base.join(suffix).to_string_lossy().replace('\\', "/");
        if current == normalized {
            updated.push_str(line);
            continue;
        }
        let quotes = if quoted { "\"" } else { "" };
        updated.push_str(&format!(
            "{name}= {quotes}{current}{quotes}{trailing}{ending}"
        ));
        changed = true;
    }
    if changed {
        // Only relocated LL-managed paths are rebased. Preserve the moved
        // account, certificates, pairing state, app list and custom settings.
        atomic_write(config, updated.as_bytes())?;
    }
    Ok(())
}
pub fn web_port(path: &Path) -> Result<u16> {
    let base = config_value(path, "port")
        .map(|port| port.parse::<u16>())
        .transpose()
        .map_err(|_| AppError::new(ErrorCode::SunshineUnavailable, "Sunshine 配置端口无效。"))?
        .unwrap_or(47989);
    if base < 6 || base > 65514 {
        return Err(AppError::new(
            ErrorCode::SunshineUnavailable,
            "Sunshine 配置端口超出允许范围。",
        ));
    }
    Ok(base + 1)
}

fn pairing_name(name: &str, requested_name: &str) -> Result<String> {
    let name = if name.trim().is_empty() {
        requested_name.trim()
    } else {
        name.trim()
    };
    let name = if name.is_empty() {
        "Moonlight 设备"
    } else {
        name
    };
    if name.len() > 128 || name.chars().count() > 64 || name.contains(['\0', '\r', '\n']) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "配对设备名称过长或包含无效字符，请填写较短的名称。",
        ));
    }
    Ok(name.to_owned())
}

#[derive(Default)]
struct IncomingLog {
    path: PathBuf,
    created: Option<SystemTime>,
    offset: u64,
    pending: usize,
    control: IncomingControl,
}

impl IncomingLog {
    fn read_events(&mut self, bytes: &[u8]) {
        for line in String::from_utf8_lossy(bytes).lines() {
            let Some((time, event)) = line.split_once("]: Info: ") else {
                continue;
            };
            if let Some(count) = event
                .strip_prefix("New streaming session started [active sessions: ")
                .and_then(|count| count.strip_suffix(']'))
                .and_then(|count| count.parse::<usize>().ok())
            {
                self.control.count = self.control.count.min(count.saturating_sub(1));
                self.pending = count.saturating_sub(self.control.count);
            } else if event == "CLIENT CONNECTED" {
                if self.control.count == 0 {
                    self.control.started_at = time.strip_prefix('[')
                        .and_then(|time| chrono::NaiveDateTime::parse_from_str(time, "%Y-%m-%d %H:%M:%S%.f").ok())
                        .and_then(|time| time.and_local_timezone(chrono::Local).earliest())
                        .map(|time| time.to_rfc3339())
                        .or_else(|| Some(crate::model::now()));
                    self.control.device_name = None;
                }
                self.control.count += 1;
                self.pending = self.pending.saturating_sub(1);
            } else if event == "CLIENT DISCONNECTED" {
                self.control.count = self.control.count.saturating_sub(1);
            } else if event.ends_with(": Ping Timeout") {
                if self.pending > 0 {
                    self.pending -= 1;
                } else {
                    self.control.count = self.control.count.saturating_sub(1);
                }
            } else if event == "Process terminated" || event.starts_with("Sunshine version:") {
                self.control = IncomingControl::default();
                self.pending = 0;
            }
        }
        if self.control.count == 0 {
            self.control = IncomingControl::default();
        }
    }
}

pub struct SunshineAdapter {
    paths: EnginePaths,
    data_root: PathBuf,
    pub external: bool,
    pub config_path: PathBuf,
    pub state_path: PathBuf,
    certificate_path: PathBuf,
    executable: PathBuf,
    child: Mutex<Option<Child>>,
    started_external: std::sync::atomic::AtomicBool,
    api_lock: Mutex<()>,
    pending_pairings: Mutex<Vec<PendingPairing>>,
    incoming_log: Mutex<IncomingLog>,
}
impl SunshineAdapter {
    pub fn new(paths: EnginePaths, data_root: PathBuf, reuse_existing: bool) -> Result<Self> {
        let service = reuse_existing.then(platform::sunshine_service).flatten();
        let external = service.is_some();
        let base = service
            .as_ref()
            .map(|s| s.sunshine_root.clone())
            .unwrap_or_else(|| data_root.join("engines/Sunshine"));
        let config_path = base.join("config/sunshine.conf");
        if !external {
            rebase_managed_config(&config_path, &base)?;
        }
        let resolve_config_path = |name: &str, default: &str| {
            config_value(&config_path, name)
                .map(PathBuf::from)
                .map(|p| if p.is_absolute() { p } else { base.join(p) })
                .unwrap_or_else(|| base.join(default))
        };
        let state_path = resolve_config_path("credentials_file", "config/sunshine_state.json");
        let certificate_path = resolve_config_path("cert", "config/credentials/cacert.pem");
        let executable = if external {
            service.unwrap().sunshine_root.join("sunshine.exe")
        } else {
            paths
                .root
                .join("engines")
                .join(&paths.manifest.sunshine.executable)
        };
        Ok(Self {
            paths,
            data_root,
            external,
            config_path,
            state_path,
            certificate_path,
            executable,
            child: Mutex::new(None),
            started_external: std::sync::atomic::AtomicBool::new(false),
            api_lock: Mutex::new(()),
            pending_pairings: Mutex::new(Vec::new()),
            incoming_log: Mutex::default(),
        })
    }
    fn ensure_managed_config(&self) -> Result<()> {
        if self.external || self.config_path.exists() {
            return Ok(());
        }
        let base = self.config_path.parent().unwrap();
        fs::create_dir_all(base.join("credentials"))?;
        let apps = base.join("apps.json");
        atomic_write(
            &apps,
            &serde_json::to_vec_pretty(
                &json!({"env": {}, "apps": [{"name": "Desktop", "image-path": "desktop.png"}]}),
            )?,
        )?;
        let absolute = |path: PathBuf| path.to_string_lossy().replace('\\', "/");
        let config = format!("locale = zh\nport = 47989\ncredentials_file = {}\nfile_state = {}\nfile_apps = {}\ncert = {}\npkey = {}\nlog_path = {}\n", absolute(self.state_path.clone()), absolute(self.state_path.clone()), absolute(apps), absolute(self.certificate_path.clone()), absolute(base.join("credentials/cakey.pem")), absolute(base.join("sunshine.log")));
        atomic_write(&self.config_path, config.as_bytes())
    }
    pub fn initialized(&self) -> bool {
        fs::read(&self.state_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|state| {
                state
                    .get("username")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty())
                    && state
                        .get("password")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.is_empty())
            })
    }
    pub fn account_username(&self) -> Option<String> {
        fs::read(&self.state_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|state| {
                state
                    .get("username")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .filter(|username| !username.is_empty())
    }
    pub fn port(&self) -> Result<u16> {
        web_port(&self.config_path)
    }
    async fn client(&self, refresh_pin: bool) -> Result<Client> {
        let port = self.port()?;
        let certificate = if let Ok(pem) = fs::read(&self.certificate_path) {
            reqwest::Certificate::from_pem(&pem).map_err(|_| {
                AppError::new(ErrorCode::SunshineUnavailable, "Sunshine 本机证书无效。")
            })?
        } else {
            // Public certificates in imported services may be ACL-protected. Pin the
            // TLS certificate only after verifying the listener's executable identity.
            let pin_path = self.data_root.join(format!("trusted/sunshine-{port}.der"));
            let der = if !refresh_pin && pin_path.is_file() {
                fs::read(&pin_path)?
            } else {
                if !platform::verify_listener(port, &self.executable) {
                    return Err(AppError::new(
                        ErrorCode::SunshineUnavailable,
                        "无法确认本地 HTTPS 接口属于 Sunshine，未发送管理凭据。",
                    ));
                }
                let bootstrap = Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(4))
                    .redirect(reqwest::redirect::Policy::none())
                    .tls_info(true)
                    .danger_accept_invalid_certs(true)
                    .build()
                    .map_err(|_| {
                        AppError::new(
                            ErrorCode::SunshineUnavailable,
                            "无法读取 Sunshine 的本机证书。",
                        )
                    })?;
                // This loopback-only bootstrap never sends credentials or a mutation.
                let response = bootstrap
                    .get(format!("https://127.0.0.1:{port}/api/configLocale"))
                    .send()
                    .await
                    .map_err(|_| {
                        AppError::new(
                            ErrorCode::SunshineUnavailable,
                            "Sunshine 本机 HTTPS 接口不可达。",
                        )
                    })?;
                let der = response
                    .extensions()
                    .get::<reqwest::tls::TlsInfo>()
                    .and_then(|info| info.peer_certificate())
                    .ok_or_else(|| {
                        AppError::new(
                            ErrorCode::SunshineUnavailable,
                            "无法读取 Sunshine 公共证书。",
                        )
                    })?
                    .to_vec();
                atomic_write(&pin_path, &der)?;
                der
            };
            reqwest::Certificate::from_der(&der).map_err(|_| {
                AppError::new(
                    ErrorCode::SunshineUnavailable,
                    "保存的 Sunshine 公共证书无效。",
                )
            })?
        };
        // Hostname relaxation is private to this pinned loopback client. Certificate
        // validation remains on; credentials are never used with a remote URL.
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(7))
            .redirect(reqwest::redirect::Policy::none())
            .tls_built_in_root_certs(false)
            .add_root_certificate(certificate)
            .danger_accept_invalid_hostnames(true)
            .build()
            .map_err(|_| {
                AppError::new(
                    ErrorCode::SunshineUnavailable,
                    "无法创建 Sunshine 的本机安全连接。",
                )
            })
    }
    async fn request(
        &self,
        client: &Client,
        method: Method,
        endpoint: &str,
        credential: Option<&SecretRecord>,
        body: Option<Value>,
    ) -> Result<Value> {
        if !matches!(
            endpoint,
            "/api/config"
                | "/api/password"
                | "/api/pin"
                | "/api/apps"
                | "/api/apps/close"
                | "/api/clients/list"
                | "/api/configLocale"
                | "/api/restart"
        ) {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "Sunshine 操作不在允许范围内。",
            ));
        }
        let pairing_confirmation = endpoint == "/api/pin" && method == Method::POST;
        let port = self.port()?;
        let mut request = client.request(method, format!("https://127.0.0.1:{port}{endpoint}"));
        if pairing_confirmation {
            // Sunshine waits for Moonlight's complete handshake before returning.
            request = request.timeout(Duration::from_secs(125));
        }
        if let Some(secret) = credential {
            request = request.basic_auth(&secret.username, Some(&secret.password));
        }
        // Native clients do not send Origin/Referer. This is Sunshine's documented
        // non-browser CSRF path, confirmed in the pinned source version.
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|error| {
            if pairing_confirmation && error.is_timeout() {
                AppError::new(
                    ErrorCode::PairingFailed,
                    "等待配对完成超时，请让控制端重新连接并确认新的 PIN。",
                )
            } else {
                AppError::new(
                    ErrorCode::SunshineUnavailable,
                    "Sunshine 本机接口不可达或证书已变化，请检查服务并重新验证账号。",
                )
            }
        })?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::new(
                ErrorCode::SunshineAccountRequired,
                "Sunshine 管理账号验证失败，请使用正确的本机账号和密码。",
            ));
        }
        if !response.status().is_success() {
            return Err(AppError::new(
                ErrorCode::SunshineUnavailable,
                "Sunshine 拒绝此操作，请在高级设置中检查当前配置。",
            ));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| AppError::new(ErrorCode::SunshineUnavailable, "Sunshine 响应未完成。"))?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(AppError::new(
                ErrorCode::SunshineUnavailable,
                "Sunshine 返回的数据过大。",
            ));
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            AppError::new(
                ErrorCode::ProtocolUnsupported,
                "Sunshine 返回了当前版本不能识别的数据。",
            )
        })
    }
    pub async fn running(&self) -> bool {
        if self.external {
            return platform::sunshine_service().is_some_and(|service| service.running);
        }
        let mut child = self.child.lock().await;
        if let Some(process) = child.as_mut() {
            if process.try_wait().ok().flatten().is_none() {
                return true;
            }
        }
        false
    }
    pub async fn start(&self) -> Result<bool> {
        self.start_for(true).await
    }
    async fn start_for(&self, streaming: bool) -> Result<bool> {
        let started = !self.running().await;
        if started && self.external {
            tokio::task::spawn_blocking(|| platform::control_sunshine_service(true))
                .await
                .map_err(|_| {
                    AppError::new(ErrorCode::SystemFailed, "Sunshine 服务权限组件未完成。")
                })??;
            self.started_external
                .store(true, std::sync::atomic::Ordering::SeqCst);
        } else if started {
            self.ensure_managed_config()?;
            let executable = self.paths.executable(&self.paths.manifest.sunshine)?;
            let mut child = self.child.lock().await;
            *child = Some(
                command(&executable, executable.parent().unwrap())
                    .arg(&self.config_path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()?,
            );
        }
        let web_port = self.port()?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
        let reachable = |port| async move {
            tokio::time::timeout(
                Duration::from_millis(400),
                tokio::net::TcpStream::connect(("127.0.0.1", port)),
            )
            .await
            .is_ok_and(|value| value.is_ok())
        };
        while tokio::time::Instant::now() < deadline {
            if !self.running().await {
                return Err(AppError::new(
                    ErrorCode::SunshineUnavailable,
                    self.startup_error(),
                ));
            }
            let ready = if streaming {
                let (web, stream, discovery, pairing) = tokio::join!(
                    reachable(web_port),
                    reachable(web_port + 20),
                    reachable(web_port - 1),
                    reachable(web_port - 6)
                );
                web && stream && discovery && pairing
            } else {
                reachable(web_port).await
            };
            if ready {
                return Ok(started);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Err(AppError::new(
            ErrorCode::SunshineUnavailable,
            "Sunshine 启动后管理或配对接口尚未就绪，请检查端口占用和采集服务状态。",
        ))
    }
    fn log_path(&self) -> PathBuf {
        let app_data = self.executable.parent().unwrap().join("config");
        let path = config_value(&self.config_path, "log_path").unwrap_or_else(|| "sunshine.log".into());
        app_data.join(path)
    }

    pub async fn incoming_control(&self) -> IncomingControl {
        self.incoming_log.lock().await.control.clone()
    }

    pub async fn poll_incoming_control(&self, vault: &Vault) -> IncomingControl {
        let mut log = self.incoming_log.lock().await;
        if !self.running().await {
            *log = IncomingLog::default();
            return log.control.clone();
        }
        let path = self.log_path();
        let Ok(mut file) = tokio::fs::File::open(&path).await else {
            *log = IncomingLog::default();
            return log.control.clone();
        };
        let Ok(metadata) = file.metadata().await else {
            return log.control.clone();
        };
        let created = metadata.created().ok();
        if log.path != path || log.created != created || metadata.len() < log.offset {
            *log = IncomingLog { path, created, ..IncomingLog::default() };
        }
        if metadata.len() == log.offset || file.seek(std::io::SeekFrom::Start(log.offset)).await.is_err() {
            return log.control.clone();
        }
        let mut bytes = Vec::new();
        const READ_LIMIT: u64 = 1024 * 1024;
        if file.take(READ_LIMIT).read_to_end(&mut bytes).await.is_ok() {
            if let Some(end) = bytes.iter().rposition(|byte| *byte == b'\n') {
                let started_at = log.control.started_at.clone();
                log.read_events(&bytes[..=end]);
                log.offset += (end + 1) as u64;
                if log.control.count > 0 && log.control.started_at != started_at {
                    log.control.device_name = self.incoming_device_name(vault).await;
                }
            } else if bytes.len() as u64 == READ_LIMIT {
                log.offset += READ_LIMIT;
            }
        }
        log.control.clone()
    }

    async fn incoming_device_name(&self, vault: &Vault) -> Option<String> {
        let secret = vault.get(&Vault::reference("local", "sunshine-admin")).ok()?;
        let client = self.client(false).await.ok()?;
        let response = self.request(&client, Method::GET, "/api/clients/list", Some(&secret), None).await.ok()?;
        let mut enabled = response.get("named_certs")?.as_array()?.iter()
            .filter(|device| device.get("enabled").and_then(Value::as_bool) != Some(false));
        let device = enabled.next()?;
        if enabled.next().is_some() {
            return None;
        }
        let name = device.get("name")?.as_str()?.trim();
        (!name.is_empty()).then(|| name.to_owned())
    }

    pub async fn disconnect_incoming_control(&self, vault: &Vault) -> Result<()> {
        let secret = vault.get(&Vault::reference("local", "sunshine-admin")).map_err(|_| {
            AppError::new(ErrorCode::SunshineAccountRequired, "请先验证本机 Sunshine 管理账号，再断开远控。")
        })?;
        let client = self.client(false).await?;
        let response = self.request(&client, Method::POST, "/api/apps/close", Some(&secret), None).await?;
        if response.get("status").and_then(Value::as_bool) != Some(true) {
            return Err(AppError::new(ErrorCode::SunshineUnavailable, "Sunshine 未能断开远控。"));
        }
        Ok(())
    }

    fn startup_error(&self) -> String {
        let log = fs::read(self.log_path())
            .map(|bytes| String::from_utf8_lossy(&bytes).to_lowercase())
            .unwrap_or_default();
        if log.contains("couldn't bind") || log.contains("address already in use") {
            format!("Sunshine 端口被占用或被 Windows 保留。请检查管理端口 {} 和串流端口 {}，并关闭重复实例后重试。", self.port().unwrap_or(47990), self.port().unwrap_or(47990) + 20)
        } else if log.contains("no display")
            || log.contains("no working encoder")
            || log.contains("could not find an encoder")
        {
            "Sunshine 未找到可用显示器或编码器，请确认显示器已开启并更新显卡驱动后重试。".into()
        } else {
            "Sunshine 启动失败，请在连接诊断中查看组件状态；完整日志位于本机数据目录的 engines/Sunshine/config/sunshine.log。".into()
        }
    }
    pub async fn stop(&self) -> Result<()> {
        if self.external {
            if self.running().await {
                tokio::task::spawn_blocking(|| platform::control_sunshine_service(false))
                    .await
                    .map_err(|_| {
                        AppError::new(ErrorCode::SystemFailed, "Sunshine 服务权限组件未完成。")
                    })??;
            }
            self.started_external
                .store(false, std::sync::atomic::Ordering::SeqCst);
        } else {
            let mut child = self.child.lock().await;
            if let Some(process) = child.as_mut() {
                if process.try_wait()?.is_none() {
                    process.kill().await?;
                }
                *child = None;
            }
        }
        self.pending_pairings.lock().await.clear();
        Ok(())
    }
    pub async fn stop_on_exit(&self, stop_reused: bool) -> Result<()> {
        if stop_reused
            || !self.external
            || self
                .started_external
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            self.stop().await?;
        }
        Ok(())
    }
    async fn account_exists(&self, client: &Client) -> Result<bool> {
        let response = client
            .get(format!("https://127.0.0.1:{}/api/config", self.port()?))
            .send()
            .await
            .map_err(|_| {
                AppError::new(
                    ErrorCode::SunshineUnavailable,
                    "Sunshine 管理接口尚未就绪。",
                )
            })?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(true);
        }
        if response.status().is_redirection()
            && response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|location| location == "/welcome" || location.ends_with("/welcome"))
        {
            return Ok(false);
        }
        Err(AppError::new(
            ErrorCode::SunshineUnavailable,
            "Sunshine 未返回有效的账号状态，请稍后重试。",
        ))
    }
    pub async fn setup(
        &self,
        username: &str,
        password: &str,
        confirm: &str,
        mode: &str,
        vault: &Vault,
    ) -> Result<()> {
        if username.trim().is_empty()
            || username.contains(':')
            || username.len() > 128
            || password.is_empty()
            || password.len() > 1024
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "请填写有效的 Sunshine 用户名和密码，用户名不能包含冒号。",
            ));
        }
        if mode != "existing" && password != confirm {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "两次输入的 Sunshine 密码不一致。",
            ));
        }
        if !["create", "existing", "change"].contains(&mode) {
            return Err(AppError::new(ErrorCode::InvalidInput, "账号操作无效。"));
        }
        let reference = Vault::reference("local", "sunshine-admin");
        let new_credential = SecretRecord {
            username: username.trim().into(),
            password: password.into(),
        };
        // The bundled Sunshine version stores SHA-256(password + salt).
        // For an idle managed instance this verifies the saved account without GPU initialization.
        if mode == "existing" && !self.external && !self.running().await {
            if let Some(valid) = self.verify_saved_account(&new_credential) {
                if !valid {
                    return Err(AppError::new(
                        ErrorCode::SunshineAccountRequired,
                        "Sunshine 用户名或密码错误。",
                    ));
                }
                return vault.put(&reference, &new_credential);
            }
        }
        let started = self.start_for(false).await?;
        let temporary = started && !self.external;
        let result = async {
            let _guard = self.api_lock.lock().await;
            let client = self.client(true).await?;
            let initialized = self.initialized() || self.account_exists(&client).await?;
            if mode == "existing" && !initialized { return Err(AppError::new(ErrorCode::SunshineAccountRequired, "Sunshine 尚未设置管理账号，请选择创建管理账号。")); }
            if mode == "create" || mode == "change" {
                if mode == "create" && initialized { return Err(AppError::new(ErrorCode::SunshineAccountRequired, "已检测到 Sunshine 管理账号，请选择使用已有账号进行验证。")); }
                let old = if mode == "change" { Some(vault.get(&reference)?) } else { None };
                let body = json!({ "currentUsername": old.as_ref().map(|s| s.username.as_str()).unwrap_or(""), "currentPassword": old.as_ref().map(|s| s.password.as_str()).unwrap_or(""), "newUsername": username.trim(), "newPassword": password, "confirmNewPassword": confirm });
                let response = self.request(&client, Method::POST, "/api/password", old.as_ref(), Some(body)).await?;
                if response.get("status") != Some(&Value::Bool(true)) { return Err(AppError::new(ErrorCode::SunshineAccountRequired, "Sunshine 未保存管理账号，请检查输入后重试。")); }
            }
            let config = self.request(&client, Method::GET, "/api/config", Some(&new_credential), None).await?;
            if !config.is_object() { return Err(AppError::new(ErrorCode::SunshineAccountRequired, "Sunshine 账号未通过实际接口认证。")); }
            vault.put(&reference, &new_credential)?;
            // Importing an existing account only authenticates and saves the
            // caller's encrypted credential; it does not rewrite service config.
            let mut config = config;
            if mode != "existing" && config.get("locale").and_then(Value::as_str) != Some("zh") {
                if let Some(object) = config.as_object_mut() { object.insert("locale".into(), json!("zh")); for key in ["platform", "version", "status"] { object.remove(key); } }
                if self.request(&client, Method::POST, "/api/config", Some(&new_credential), Some(config)).await.is_err() {
                    tracing::warn!("Sunshine account verified; locale update was not accepted");
                }
            }
            Ok(())
        }.await;
        if temporary {
            let _ = self.stop().await;
        }
        result
    }
    fn verify_saved_account(&self, credential: &SecretRecord) -> Option<bool> {
        let state: Value = serde_json::from_slice(&fs::read(&self.state_path).ok()?).ok()?;
        let username = state.get("username")?.as_str()?;
        let salt = state.get("salt")?.as_str()?;
        let password_hash = state.get("password")?.as_str()?;
        if !username.is_ascii()
            || !credential.username.is_ascii()
            || salt.len() != 16
            || password_hash.len() != 64
            || !password_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        let mut hash = Sha256::new();
        hash.update(credential.password.as_bytes());
        hash.update(salt.as_bytes());
        // Sunshine's util::hex emits the digest bytes in reverse order.
        let actual: String = hash
            .finalize()
            .iter()
            .rev()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let difference = actual
            .bytes()
            .zip(password_hash.bytes())
            .fold(0, |difference, (left, right)| {
                difference | (left ^ right.to_ascii_lowercase())
            });
        Some(username.eq_ignore_ascii_case(&credential.username) && difference == 0)
    }
    pub async fn status(&self, vault: &Vault) -> SunshineStatus {
        let running = self.running().await;
        let mut status = SunshineStatus {
            running,
            external: self.external,
            account_initialized: self.initialized(),
            account_username: self.account_username(),
            port: self.port().unwrap_or(47990),
            locale: config_value(&self.config_path, "locale").unwrap_or_else(|| "zh".into()),
            ..SunshineStatus::default()
        };
        if !running {
            self.pending_pairings.lock().await.clear();
            return status;
        }
        status.pending_pairings = self.pending_pairings.lock().await.clone();
        let reachable = tokio::time::timeout(
            Duration::from_millis(500),
            tokio::net::TcpStream::connect(("127.0.0.1", status.port)),
        )
        .await
        .is_ok_and(|result| result.is_ok());
        if !reachable {
            return status;
        }
        let Ok(client) = self.client(false).await else {
            return status;
        };
        // Always read the live account state. A browser login/configuration and
        // LL's encrypted PIN credential are distinct, but share this service.
        if let Ok(initialized) = self.account_exists(&client).await {
            status.ready = true;
            status.account_initialized = initialized;
            if !initialized {
                status.account_username = None;
            }
        }
        let Ok(secret) = vault.get(&Vault::reference("local", "sunshine-admin")) else {
            return status;
        };
        let Ok(config) = self
            .request(&client, Method::GET, "/api/config", Some(&secret), None)
            .await
        else {
            return status;
        };
        status.ready = true;
        status.account_initialized = true;
        status.account_username = Some(secret.username.clone());
        status.credential_valid = true;
        status.locale = config
            .get("locale")
            .and_then(Value::as_str)
            .unwrap_or(&status.locale)
            .into();
        status.config = SunshineSettings {
            output_name: config
                .get("output_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            audio_sink: config
                .get("audio_sink")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            adapter_name: config
                .get("adapter_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
        };
        if let Ok(response) = self
            .request(&client, Method::GET, "/api/apps", Some(&secret), None)
            .await
        {
            if let Some(apps) = response.get("apps").and_then(Value::as_array) {
                status.apps = apps
                    .iter()
                    .enumerate()
                    .filter_map(|(index, app)| {
                        app.get("name")
                            .and_then(Value::as_str)
                            .map(|name| PublishedApp {
                                id: index.to_string(),
                                name: name.into(),
                            })
                    })
                    .collect();
            }
        }
        if let Ok(response) = self
            .request(&client, Method::GET, "/api/pin", Some(&secret), None)
            .await
        {
            if let Some(pairs) = response.get("pairings").and_then(Value::as_array) {
                status.pending_pairings = pairs
                    .iter()
                    .filter_map(|pair| {
                        Some(PendingPairing {
                            id: pair.get("id")?.as_str()?.into(),
                            name: pair
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .into(),
                        })
                    })
                    .collect();
                *self.pending_pairings.lock().await = status.pending_pairings.clone();
            }
        }
        status
    }
    pub async fn confirm_pin(
        &self,
        pin: &str,
        name: &str,
        pairing_id: &str,
        vault: &Vault,
    ) -> Result<()> {
        if pin.len() != 4 || !pin.bytes().all(|b| b.is_ascii_digit()) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "请输入四位数字 PIN。",
            ));
        }
        if pairing_id.len() != 32 || !pairing_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AppError::new(
                ErrorCode::PairingFailed,
                "请选择当前待确认的设备，请让控制端重新连接以取得配对请求。",
            ));
        }
        let _guard = self.api_lock.lock().await;
        let secret = vault.get(&Vault::reference("local", "sunshine-admin"))?;
        let client = self.client(false).await?;
        let pending = self
            .request(&client, Method::GET, "/api/pin", Some(&secret), None)
            .await?;
        let selected = pending
            .get("pairings")
            .and_then(Value::as_array)
            .and_then(|pairings| {
                pairings
                    .iter()
                    .find(|pairing| pairing.get("id").and_then(Value::as_str) == Some(pairing_id))
            })
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::PairingFailed,
                    "此配对请求已结束，请让控制端重新连接并输入新的 PIN。",
                )
            })?;
        let name = pairing_name(
            name,
            selected.get("name").and_then(Value::as_str).unwrap_or(""),
        )?;
        let body = json!({ "pin": pin, "name": name, "pairing_id": pairing_id });
        let response = self
            .request(&client, Method::POST, "/api/pin", Some(&secret), Some(body))
            .await?;
        self.pending_pairings
            .lock()
            .await
            .retain(|pending| pending.id != pairing_id);
        if response.get("status") != Some(&Value::Bool(true)) {
            return Err(AppError::new(
                ErrorCode::PairingFailed,
                "Sunshine 未确认此 PIN，请检查当前配对请求。",
            ));
        }
        Ok(())
    }
    pub async fn save_settings(&self, settings: &SunshineSettings, vault: &Vault) -> Result<()> {
        for value in [
            &settings.output_name,
            &settings.audio_sink,
            &settings.adapter_name,
        ] {
            if value.len() > 1024 || value.contains(['\0', '\r', '\n']) {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "采集设置包含无效字符或长度。",
                ));
            }
        }
        let _guard = self.api_lock.lock().await;
        let secret = vault.get(&Vault::reference("local", "sunshine-admin"))?;
        let client = self.client(false).await?;
        let mut config = self
            .request(&client, Method::GET, "/api/config", Some(&secret), None)
            .await?;
        let object = config.as_object_mut().ok_or_else(|| {
            AppError::new(
                ErrorCode::ProtocolUnsupported,
                "Sunshine 配置结构不受支持。",
            )
        })?;
        object.insert("output_name".into(), json!(settings.output_name));
        object.insert("audio_sink".into(), json!(settings.audio_sink));
        object.insert("adapter_name".into(), json!(settings.adapter_name));
        object.insert("locale".into(), json!("zh"));
        for key in ["platform", "version", "status"] {
            object.remove(key);
        }
        let response = self
            .request(
                &client,
                Method::POST,
                "/api/config",
                Some(&secret),
                Some(config),
            )
            .await?;
        if response.get("status") == Some(&Value::Bool(false)) {
            return Err(AppError::new(
                ErrorCode::SunshineUnavailable,
                "Sunshine 未接受设置。",
            ));
        }
        Ok(())
    }
    pub fn config_url(&self) -> Result<String> {
        Ok(format!("https://127.0.0.1:{}/config", self.port()?))
    }
    pub async fn prepare_vpn_firewall(&self, local_address: &str) -> Result<()> {
        if !matches!(local_address.parse::<std::net::IpAddr>(), Ok(std::net::IpAddr::V4(ip)) if !ip.is_loopback() && !ip.is_unspecified())
        {
            return Err(AppError::new(
                ErrorCode::SunshineUnavailable,
                "尚未取得本机皎月连组网地址，无法准备 Sunshine 的组网访问。",
            ));
        }
        self.vpn_firewall_command(Some(local_address)).await
    }
    pub async fn restore_vpn_firewall(&self) -> Result<()> {
        if !self
            .data_root
            .join("network-firewall/sunshine.json")
            .exists()
            && !self
                .data_root
                .join("network-firewall/sunshine.json.pending")
                .exists()
        {
            return Ok(());
        }
        self.vpn_firewall_command(None).await
    }
    async fn vpn_firewall_command(&self, local_address: Option<&str>) -> Result<()> {
        let helper = self
            .paths
            .root
            .join("component-tools/ConfigureSunshineVpn.ps1");
        if !helper.is_file() {
            return Err(AppError::new(
                ErrorCode::ComponentMissing,
                "随包 Sunshine 组网权限组件缺失，请使用完整运行目录。",
            ));
        }
        let state = self.data_root.join("network-firewall/sunshine.json");
        let mut process = command(Path::new("powershell.exe"), &self.paths.root);
        process
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(helper)
            .arg("-Program")
            .arg(&self.executable)
            .arg("-StateFile")
            .arg(state)
            .args(["-BasePort", &(self.port()? - 1).to_string(), "-Apply"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(address) = local_address {
            process.args(["-LocalAddress", address]);
        } else {
            process.arg("-Restore");
        }
        // CIM starts slowly on some Windows installations. The helper performs
        // one bounded transaction and rolls every touched filter back on error.
        let output = match tokio::time::timeout(Duration::from_secs(45), process.output()).await {
            Ok(result) => result?,
            Err(_) => {
                if local_address.is_some() {
                    // The helper journals the original filters before mutation.
                    // A fresh restore invocation recovers a terminated transaction.
                    let recovered = Box::pin(self.vpn_firewall_command(None)).await;
                    if recovered.is_err() {
                        return Err(AppError::new(ErrorCode::SunshineUnavailable, "Sunshine 组网权限调整超时，原防火墙规则恢复尚未确认，请检查本机规则后重试。"));
                    }
                }
                return Err(AppError::new(
                    ErrorCode::SunshineUnavailable,
                    "等待 Windows 准备 Sunshine 组网权限超时，已请求恢复原规则，请重试。",
                ));
            }
        };
        let report: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
            AppError::new(
                ErrorCode::SunshineUnavailable,
                "Windows 未返回有效的 Sunshine 组网权限结果。",
            )
        })?;
        if output.status.success() && report.get("ok") == Some(&Value::Bool(true)) {
            return Ok(());
        }
        tracing::warn!(
            code = firewall_diagnostic_identifier(&report, "code"),
            phase = firewall_diagnostic_identifier(&report, "phase"),
            exception_type = firewall_diagnostic_identifier(&report, "exceptionType"),
            blocker_kind = firewall_diagnostic_identifier(&report, "blockerKind"),
            policy_source = firewall_diagnostic_identifier(&report, "policySource"),
            hresult = report.get("hresult").and_then(|value| value.as_i64()),
            native_error_code = report
                .get("nativeErrorCode")
                .and_then(|value| value.as_i64()),
            win32_code = report.get("win32Code").and_then(|value| value.as_i64()),
            mutation_started = report
                .get("mutationStarted")
                .and_then(|value| value.as_bool()),
            rollback_complete = report
                .get("rollbackComplete")
                .and_then(|value| value.as_bool()),
            "Sunshine VPN firewall operation did not complete"
        );
        Err(vpn_firewall_report_error(&report))
    }
    pub async fn peer_gateways(
        &self,
        local_address: &str,
        cancel: &CancellationToken,
    ) -> Vec<String> {
        if local_address.parse::<std::net::IpAddr>().is_err() {
            return vec![];
        }
        let helper = self.paths.root.join("component-tools/GetPeerAddress.ps1");
        let mut process = command(Path::new("powershell.exe"), &self.paths.root);
        process
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(helper)
            .args(["-LocalAddress", local_address])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let output = tokio::select! { _ = cancel.cancelled() => return vec![], value = tokio::time::timeout(Duration::from_secs(5), process.output()) => value };
        output
            .ok()
            .and_then(|result| result.ok())
            .and_then(|output| serde_json::from_slice::<Vec<String>>(&output.stdout).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
            .take(4)
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct HostInfo {
    pub uuid: String,
    pub name: String,
    pub address: String,
    pub https_port: u16,
}

impl HostInfo {
    pub async fn check_https_port(&self, cancel: &CancellationToken) -> Result<()> {
        let (host, _) = validate_address(&self.address)?;
        let host = host.trim_matches(['[', ']']);
        let port = self.https_port;
        let connect = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect((host, port)),
        );
        tokio::select! {
            _ = cancel.cancelled() => Err(AppError::new(ErrorCode::Canceled, "连接已取消。")),
            result = connect => {
                if matches!(result, Ok(Ok(_))) {
                    Ok(())
                } else {
                    tracing::debug!(port, timed_out = result.is_err(), "Sunshine GameStream HTTPS port is unreachable");
                    Err(AppError::new(
                        ErrorCode::SunshineUnavailable,
                        format!("已找到对方 Sunshine，但配对和应用列表所需的 TCP {port} 端口无法连接。请在被控端检查 Sunshine 运行状态和组网防火墙规则。"),
                    ))
                }
            },
        }
    }
}
fn firewall_diagnostic_identifier<'a>(report: &'a Value, key: &str) -> Option<&'a str> {
    let value = report.get(key)?.as_str()?;
    let accepted = match key {
        "code" => matches!(
            value,
            "program-invalid"
                | "firewall-busy"
                | "recovery-required"
                | "recovery-failed"
                | "state-conflict"
                | "owned-rule-conflict"
                | "vpn-address-invalid"
                | "vpn-address-not-bound"
                | "vpn-address-not-unique"
                | "vpn-interface-invalid"
                | "vpn-interface-not-up"
                | "port-invalid"
                | "policy-blocked"
                | "profile-inbound-blocked"
                | "profile-local-rules-disabled"
                | "rules-not-effective"
                | "block-rule-missing"
                | "access-denied"
                | "firewall-operation-failed"
        ),
        "phase" => matches!(
            value,
            "validation"
                | "program-validation"
                | "lock-acquisition"
                | "journal-recovery"
                | "state-validation"
                | "restore-plan"
                | "restore-journal"
                | "restore-blocks"
                | "restore-rules"
                | "restore-state"
                | "address-validation"
                | "interface-address-query"
                | "interface-adapter-query"
                | "interface-profile-query"
                | "profile-policy-query"
                | "verify-active-rules"
                | "firewall-rule-query"
                | "blocker-inspection"
                | "owned-rule-snapshot"
                | "apply-journal"
                | "apply-rules"
                | "apply-tcp-allow"
                | "apply-udp-allow"
                | "apply-tcp-block"
                | "apply-udp-block"
                | "apply-blocks"
                | "persist-state"
        ),
        "blockerKind" => matches!(
            value,
            "group-policy" | "custom-rule" | "nonlocal-rule" | "modified-automatic-rule"
        ),
        "policySource" => matches!(
            value,
            "Local" | "GroupPolicy" | "Dynamic" | "Hardcoded" | "Generated" | "Unknown"
        ),
        "exceptionType" => matches!(
            value,
            "Microsoft.Management.Infrastructure.CimException"
                | "Microsoft.PowerShell.Cmdletization.Cim.CimJobException"
                | "System.UnauthorizedAccessException"
                | "System.ComponentModel.Win32Exception"
                | "System.IO.IOException"
                | "System.ArgumentException"
                | "System.InvalidOperationException"
                | "System.NullReferenceException"
                | "System.Management.Automation.RuntimeException"
                | "System.Management.Automation.CommandNotFoundException"
                | "System.Management.Automation.ItemNotFoundException"
                | "System.Management.Automation.ParameterBindingException"
        ),
        _ => false,
    };
    accepted.then_some(value)
}

fn vpn_firewall_report_error(report: &Value) -> AppError {
    if report.get("rollbackComplete") == Some(&Value::Bool(false)) {
        return AppError::new(
            ErrorCode::SunshineUnavailable,
            "Sunshine 防火墙调整未完成，原规则恢复尚未确认，诊断记录已保留。",
        );
    }
    let reason = firewall_diagnostic_identifier(report, "code").unwrap_or("");
    let blocker_confirmed = report.get("blockingRuleDetected") == Some(&Value::Bool(true));
    let message = match reason {
        "vpn-address-invalid"
        | "vpn-address-not-bound"
        | "vpn-address-not-unique"
        | "vpn-interface-invalid"
        | "vpn-interface-not-up" => {
            "本机组网地址尚未绑定到已启用的皎月连网卡，Sunshine 组网访问暂未确认。"
        }
        "policy-blocked" if blocker_confirmed => {
            match firewall_diagnostic_identifier(report, "blockerKind") {
                Some("group-policy") => "组织防火墙策略阻止了 Sunshine 组网访问。请把规则名称发给管理这台电脑的管理员。",
                Some("custom-rule" | "modified-automatic-rule") => "本机防火墙规则限制了 Sunshine 组网访问。请在被控电脑的「高级安全 Windows Defender 防火墙 → 入站规则」核对该规则，由设置这条规则的人调整其适用范围，再重新开启接受控制。",
                _ => "已发现匹配的防火墙阻止规则，Sunshine 自动调整尚未完成。",
            }
        }
        "profile-inbound-blocked" => "Windows 当前网络配置禁止入站连接。请在被控电脑的防火墙设置中核对当前网络配置，受管理电脑请联系管理员。",
        "profile-local-rules-disabled" => "Windows 当前网络策略禁止应用本地防火墙规则。请由被控电脑的管理员为 Sunshine 配置组网入站权限。",
        "rules-not-effective" => "Windows 未启用写入的 Sunshine 防火墙规则。请检查被控电脑的防火墙管理策略。",
        "access-denied" => "Windows 拒绝写入 Sunshine 防火墙规则，组网访问暂未确认。",
        "state-conflict" | "owned-rule-conflict" => {
            "Sunshine 防火墙规则与保存的记录不一致，已保留现有设置。"
        }
        "firewall-busy" => "另一项 Sunshine 防火墙操作尚未结束，组网访问暂未确认。",
        "recovery-required" | "recovery-failed" => {
            "上次 Sunshine 防火墙操作恢复尚未完成，诊断记录已保留。"
        }
        _ => "Sunshine 防火墙设置未完成，具体原因尚未确认，可查看连接诊断记录。",
    };
    let diagnostic = matches!(reason, "access-denied" | "firewall-operation-failed")
        .then(|| report.get("hresult").and_then(Value::as_i64))
        .flatten();
    let message = if reason == "policy-blocked" && blocker_confirmed {
        let rule_name: String = report
            .get("blockerDisplayName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_control())
            .take(140)
            .collect();
        let rule_name = rule_name.trim();
        if rule_name.is_empty() {
            message.to_owned()
        } else {
            format!("{message} 阻止规则「{rule_name}」。")
        }
    } else if let Some(code) = diagnostic {
        format!("{message}（错误 0x{:08X}）", code as u32)
    } else {
        message.to_owned()
    };
    AppError::new(ErrorCode::SunshineUnavailable, message)
}

pub fn validate_address(address: &str) -> Result<(String, u16)> {
    if address.is_empty()
        || address.len() > 253
        || address.starts_with('-')
        || address.contains(['/', '\\', '?', '#', '@', '\0'])
        || address.chars().any(char::is_whitespace)
    {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "主机地址无效，请填写 IP 或主机名。",
        ));
    }
    if let Ok(ip) = address.parse::<std::net::IpAddr>() {
        return Ok((ip.to_string(), 47989));
    }
    let target = reqwest::Url::parse(&format!("http://{address}"))
        .map_err(|_| AppError::new(ErrorCode::InvalidInput, "主机地址无效。"))?;
    let host = target
        .host_str()
        .ok_or_else(|| AppError::new(ErrorCode::InvalidInput, "主机地址无效。"))?
        .to_owned();
    let port = target.port().unwrap_or(47989);
    if port == 0 {
        return Err(AppError::new(ErrorCode::InvalidInput, "主机端口无效。"));
    }
    Ok((host, port))
}
pub async fn probe_host(
    address: &str,
    expected_uuid: Option<&str>,
    cancel: &CancellationToken,
) -> Result<HostInfo> {
    let (host, port) = validate_address(address)?;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AppError::new(ErrorCode::SunshineUnavailable, "无法检查远程主机。"))?;
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host
    };
    let endpoint = format!("{host}:{port}");
    let response = tokio::select! {
        _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Canceled, "连接已取消。")),
        value = client.get(format!("http://{endpoint}/serverinfo")).send() => value.map_err(|error| {
            let detail = if error.is_timeout() { "请求超时" } else { "无法建立连接" };
            AppError::new(
                ErrorCode::SunshineUnavailable,
                format!("对方 Sunshine {detail}（{endpoint}）。请确认对方已开启接受控制，并允许 Sunshine 通过 Windows 防火墙。"),
            )
        })?
    };
    if !response.status().is_success() {
        return Err(AppError::new(
            ErrorCode::SunshineUnavailable,
            format!(
                "对方 Sunshine 拒绝主机探测（{endpoint}，HTTP {}），请核对组网地址和 Sunshine 端口。",
                response.status().as_u16()
            ),
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| AppError::new(ErrorCode::SunshineUnavailable, "远程主机响应未完成。"))?;
    if bytes.len() > 256 * 1024 {
        return Err(AppError::new(
            ErrorCode::SunshineUnavailable,
            "远程主机响应格式无效。",
        ));
    }
    let text = String::from_utf8_lossy(&bytes);
    let document = roxmltree::Document::parse(&text).map_err(|_| {
        AppError::new(
            ErrorCode::SunshineUnavailable,
            "远程地址未返回有效的 Sunshine 主机信息。",
        )
    })?;
    if document
        .root_element()
        .attribute("status_code")
        .is_some_and(|status| status != "200")
    {
        return Err(AppError::new(
            ErrorCode::SunshineUnavailable,
            "对方 Sunshine 返回主机探测失败状态，请检查对方服务。",
        ));
    }
    let value = |name: &str| {
        document
            .descendants()
            .find(|node| node.has_tag_name(name))
            .and_then(|node| node.text())
            .map(str::to_owned)
    };
    let uuid = value("uniqueid")
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::SunshineUnavailable,
                "无法确认远程 Sunshine 主机身份。",
            )
        })?;
    if expected_uuid.is_some_and(|expected| !expected.eq_ignore_ascii_case(&uuid)) {
        return Err(AppError::new(
            ErrorCode::PermissionDenied,
            "远程主机身份与保存的设备不同，已停止连接。请重新核对设备并配对。",
        ));
    }
    let https_port = value("HttpsPort")
        .and_then(|value| value.trim().parse::<u16>().ok())
        .filter(|port| *port != 0)
        .unwrap_or(47984);
    Ok(HostInfo {
        uuid,
        name: value("hostname").unwrap_or_else(|| "远程电脑".into()),
        address: address.into(),
        https_port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firewall_errors_require_evidence_before_claiming_policy_restrictions() {
        for report in [
            json!({"code":"policy-blocked","rollbackComplete":true}),
            json!({"code":"firewall-operation-failed","hresult":-2147024809,"rollbackComplete":true}),
            json!({"code":"untrusted fixture-password response","rollbackComplete":true}),
        ] {
            let error = vpn_firewall_report_error(&report);
            assert!(!error.message.contains("组策略"));
            assert!(!error.message.contains("管理员"));
            assert!(!error.message.contains("fixture-password"));
        }
        let custom = vpn_firewall_report_error(
            &json!({"code":"policy-blocked","blockingRuleDetected":true,"blockerKind":"custom-rule","rollbackComplete":true}),
        );
        assert!(custom.message.contains("入站规则"));
        assert!(!custom.message.contains("组策略"));
        let group = vpn_firewall_report_error(
            &json!({"code":"policy-blocked","blockingRuleDetected":true,"blockerKind":"group-policy","rollbackComplete":true}),
        );
        assert!(group.message.contains("组织防火墙策略"));
        assert!(group.message.contains("管理员"));
    }

    #[test]
    fn a_missing_vpn_binding_is_distinct_from_permission_and_policy_failures() {
        let binding = vpn_firewall_report_error(
            &json!({"code":"vpn-address-not-bound","phase":"interface-address-query","rollbackComplete":true}),
        );
        assert!(binding.message.contains("尚未绑定"));
        assert!(!binding.message.contains("组策略"));
        let denied = vpn_firewall_report_error(
            &json!({"code":"access-denied","hresult":-2147024891,"rollbackComplete":true}),
        );
        assert!(denied.message.contains("拒绝写入"));
        assert!(denied.message.contains("80070005"));
    }

    #[test]
    fn firewall_diagnostics_never_log_arbitrary_native_text() {
        let report = json!({"code":"fixture-secret","phase":"fixture-private-path","exceptionType":"fixture-secret"});
        for key in ["code", "phase", "exceptionType"] {
            assert!(firewall_diagnostic_identifier(&report, key).is_none());
        }
        let error =
            vpn_firewall_report_error(&json!({"code":"access-denied","rollbackComplete":false}));
        assert!(error.message.contains("恢复尚未确认"));
        assert!(!error.message.contains("已恢复"));
    }

    #[test]
    fn blank_pairing_name_uses_the_requested_device_name() {
        assert_eq!(
            pairing_name("", "Living room PC").unwrap(),
            "Living room PC"
        );
        assert_eq!(pairing_name("  ", "").unwrap(), "Moonlight 设备");
        assert_eq!(pairing_name(" 客厅电脑 ", "Other PC").unwrap(), "客厅电脑");
    }

    #[test]
    fn pairing_names_respect_sunshine_utf8_byte_limit() {
        assert!(pairing_name(&"电".repeat(42), "").is_ok());
        assert_eq!(
            pairing_name(&"电".repeat(43), "").unwrap_err().code,
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn host_addresses_reject_whitespace_and_keep_stream_http_port() {
        assert_eq!(
            validate_address("10.20.0.2").unwrap(),
            ("10.20.0.2".into(), 47989)
        );
        assert_eq!(
            validate_address("10.20.0.2:50000").unwrap(),
            ("10.20.0.2".into(), 50000)
        );
        assert_eq!(
            validate_address("2001:db8::2").unwrap(),
            ("2001:db8::2".into(), 47989)
        );
        for address in [
            "\t10.20.0.2",
            "10.20.0.2\n",
            "http://10.20.0.2",
            "user@10.20.0.2",
            "10.20.0.2:0",
        ] {
            assert_eq!(
                validate_address(address).unwrap_err().code,
                ErrorCode::InvalidInput
            );
        }
    }

    async fn host_response(status: &str, body: &str) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let reply = format!("HTTP/1.1 {status}\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        let worker = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut byte = [0];
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                socket.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
            }
            socket.write_all(reply.as_bytes()).await.unwrap();
        });
        (address, worker)
    }

    #[tokio::test]
    async fn valid_host_identity_requires_successful_http_and_protocol_status() {
        let uuid = "0f5e69e7-6878-4473-a2ef-8e10f17bb8b0";
        let body = format!("<root status_code=\"200\"><uniqueid>{uuid}</uniqueid><hostname>Fixture host</hostname></root>");
        let (address, worker) = host_response("503 Service Unavailable", &body).await;
        let error = probe_host(&address, None, &CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::SunshineUnavailable);
        assert!(error.message.contains("HTTP 503"));
        worker.await.unwrap();

        let failed = body.replace("status_code=\"200\"", "status_code=\"503\"");
        let (address, worker) = host_response("200 OK", &failed).await;
        assert_eq!(
            probe_host(&address, None, &CancellationToken::new())
                .await
                .unwrap_err()
                .code,
            ErrorCode::SunshineUnavailable
        );
        worker.await.unwrap();

        let (address, worker) = host_response("200 OK", &body).await;
        let host = probe_host(&address, Some(uuid), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(host.uuid, uuid);
        assert_eq!(host.address, address);
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn reachable_different_host_never_reuses_saved_pairing_identity() {
        let (address, worker) = host_response("200 OK", "<root status_code=\"200\"><uniqueid>0f5e69e7-6878-4473-a2ef-8e10f17bb8b0</uniqueid></root>").await;
        let error = probe_host(
            &address,
            Some("e7aa11d9-2a8d-4ec2-bfe4-e3d9bb71f752"),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        worker.await.unwrap();
    }
}
