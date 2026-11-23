pub mod moonlight;
mod moonlight_preferences;
pub mod nat;
pub mod sunshine;

use crate::error::{AppError, ErrorCode, Result};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime},
};
use tokio::{
    process::{Child, Command},
    sync::Mutex,
};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineEntry {
    pub version: String,
    pub executable: String,
    #[serde(default)]
    pub extension_protocol: u32,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub script_sha256: String,
}
#[derive(Clone, Deserialize)]
pub struct Manifest {
    pub moonlight: EngineEntry,
    pub sunshine: EngineEntry,
    pub jiaoyuelian: EngineEntry,
}

#[derive(Clone)]
pub struct EnginePaths {
    pub root: PathBuf,
    pub manifest: Manifest,
}
impl EnginePaths {
    pub fn open(root: PathBuf) -> Result<Self> {
        let path = root.join("engines/manifest.json");
        let manifest = serde_json::from_slice(&std::fs::read(path)?)?;
        Ok(Self { root, manifest })
    }
    pub fn executable(&self, entry: &EngineEntry) -> Result<PathBuf> {
        let relative = Path::new(&entry.executable);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "组件清单中的路径无效。",
            ));
        }
        let path = self.root.join("engines").join(relative);
        if !path.is_file() {
            return Err(AppError::new(
                ErrorCode::ComponentMissing,
                "随包内核资源缺失，请在诊断页查看并使用完整 LL远程安装包。",
            ));
        }
        let canonical = path.canonicalize()?;
        if !canonical.starts_with(self.root.join("engines").canonicalize()?) {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "组件路径超出随包目录。",
            ));
        }
        Ok(path)
    }
}

pub fn command(executable: &Path, directory: &Path) -> Command {
    let mut command = Command::new(executable);
    command.current_dir(directory).stdin(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW affects consoles, not the stream GUI.
    command
}

#[cfg(windows)]
fn wait_for_nat_exit(handle: isize) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{TerminateProcess, WaitForSingleObject},
    };
    let process = handle as windows_sys::Win32::Foundation::HANDLE;
    let mut state = unsafe { WaitForSingleObject(process, 4000) };
    if state == WAIT_TIMEOUT {
        let terminated = unsafe { TerminateProcess(process, 0) } != 0;
        state = unsafe { WaitForSingleObject(process, if terminated { 4000 } else { 0 }) };
    }
    if state == WAIT_OBJECT_0 {
        return Ok(());
    }
    Err(AppError::new(
        ErrorCode::NetworkFailed,
        "管理员网络宿主退出未确认，请稍后重试。",
    ))
}

#[cfg(windows)]
fn create_nat_stop_event(port: u16) -> Result<isize> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{CreateEventW, ResetEvent},
    };
    let name: Vec<u16> = format!("Local\\LLRemoteNatStop-{port}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, name.as_ptr()) };
    if event.is_null() {
        return Err(AppError::new(
            ErrorCode::SystemFailed,
            "无法创建网络宿主控制事件。",
        ));
    }
    if unsafe { ResetEvent(event) } == 0 {
        unsafe {
            CloseHandle(event);
        }
        return Err(AppError::new(
            ErrorCode::SystemFailed,
            "无法准备网络宿主控制事件。",
        ));
    }
    Ok(event as isize)
}

pub struct EngineManager {
    pub paths: EnginePaths,
    data_root: PathBuf,
    // Serialize host lifecycle operations and retain pending shutdowns for retry.
    nat_stopping: Mutex<bool>,
    nat_child: Mutex<Option<Child>>,
    nat_stop: Mutex<Option<isize>>,
    nat_elevated: Mutex<Option<isize>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NatDriverError {
    pub process_id: u32,
    #[serde(default)]
    pub timestamp: Option<u64>,
    pub elevated: bool,
    pub win32_code: Option<u32>,
    pub problem_code: Option<u32>,
    pub nt_status: Option<u32>,
    pub stage: String,
}
impl NatDriverError {
    pub fn application_error(&self) -> Option<AppError> {
        let stage = match self.stage.as_str() {
            "adapterSetup" | "interfaceWait" => "Windows 无法完成 Wintun 网卡接口注册",
            "adapterCreate" => "Windows 无法创建 Wintun 网卡设备",
            "driverInstall" => "Wintun 驱动安装失败",
            "sessionStart" => "Wintun 收发会话启动失败",
            "wintun" => "Wintun 网卡初始化失败",
            _ => return None,
        };
        let mut details = vec![];
        if let Some(code) = self.win32_code {
            details.push(format!("Windows 错误 {code}"));
        }
        if let Some(code) = self.problem_code {
            details.push(format!("设备问题 {code}"));
        }
        if let Some(status) = self.nt_status {
            details.push(format!("NTSTATUS 0x{status:08X}"));
        }
        if details.is_empty() {
            return None;
        }
        Some(AppError::new(
            if self.win32_code == Some(5) {
                ErrorCode::PermissionDenied
            } else {
                ErrorCode::VirtualAdapterFailed
            },
            format!(
                "{stage}（{}）。具体阶段已记录在网络宿主日志中。",
                details.join("，")
            ),
        ))
    }
    fn recorded_at(&self) -> Option<SystemTime> {
        let timestamp = self.timestamp?;
        let seconds = (timestamp / 10_000_000).checked_sub(11_644_473_600)?;
        let nanoseconds = (timestamp % 10_000_000) as u32 * 100;
        SystemTime::UNIX_EPOCH.checked_add(Duration::new(seconds, nanoseconds))
    }
}

impl EngineManager {
    pub fn new(paths: EnginePaths, data_root: PathBuf) -> Self {
        Self {
            paths,
            data_root,
            nat_stopping: Mutex::new(false),
            nat_child: Mutex::new(None),
            nat_stop: Mutex::new(None),
            nat_elevated: Mutex::new(None),
        }
    }
    pub async fn ensure_nat(&self) -> Result<()> {
        let mut stopping = self.nat_stopping.lock().await;
        if *stopping {
            self.release_owned_nat_inner().await?;
            *stopping = false;
        }
        let port = self.paths.manifest.jiaoyuelian.port;
        if tokio::time::timeout(
            std::time::Duration::from_millis(400),
            tokio::net::TcpStream::connect(("127.0.0.1", port)),
        )
        .await
        .is_ok_and(|value| value.is_ok())
        {
            return Ok(());
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::{
                Foundation::{CloseHandle, WAIT_OBJECT_0},
                System::Threading::WaitForSingleObject,
            };
            let mut elevated = self.nat_elevated.lock().await;
            if let Some(handle) = *elevated {
                if unsafe { WaitForSingleObject(handle as _, 0) } != WAIT_OBJECT_0 {
                    return Err(AppError::new(
                        ErrorCode::NetworkFailed,
                        "管理员网络宿主的接口尚未就绪，请稍后重试。",
                    ));
                }
                unsafe {
                    CloseHandle(handle as _);
                }
                *elevated = None;
            }
        }
        let mut child = self.nat_child.lock().await;
        if let Some(process) = child.as_mut() {
            if process.try_wait()?.is_none() {
                return Ok(());
            }
        }
        let executable = self.paths.executable(&self.paths.manifest.jiaoyuelian)?;
        let host = self.paths.root.join("LLRemote.EngineHost.exe");
        let data = self.data_root.join("engines/Jiaoyuelian");
        std::fs::create_dir_all(&data)?;
        std::fs::copy(
            executable.parent().unwrap().join("network.dll"),
            data.join("network.dll"),
        )?;
        let stop_name = format!("Local\\LLRemoteNatStop-{port}");
        #[cfg(windows)]
        {
            let event = create_nat_stop_event(port)?;
            if let Some(previous) = self.nat_stop.lock().await.replace(event) {
                unsafe {
                    windows_sys::Win32::Foundation::CloseHandle(previous as _);
                }
            }
        }
        *child = Some(if host.is_file() {
            command(&host, &data)
                .env("LLREMOTE_ENGINE_DIRECTORY", executable.parent().unwrap())
                .env("LLREMOTE_DATA_DIRECTORY", &data)
                .env("LLREMOTE_STOP_EVENT", stop_name)
                .args(["-p", &port.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?
        } else {
            command(&executable, executable.parent().unwrap())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?
        });
        for _ in 0..30 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        Err(AppError::new(
            ErrorCode::NetworkFailed,
            "皎月连尚未启动本地接口，请检查组件初始化与系统权限。",
        ))
    }
    pub async fn release_owned_nat(&self) -> Result<()> {
        let mut stopping = self.nat_stopping.lock().await;
        *stopping = true;
        self.release_owned_nat_inner().await?;
        *stopping = false;
        Ok(())
    }
    async fn release_owned_nat_inner(&self) -> Result<()> {
        #[cfg(windows)]
        if let Some(event) = *self.nat_stop.lock().await {
            if unsafe { windows_sys::Win32::System::Threading::SetEvent(event as _) } == 0 {
                return Err(AppError::new(
                    ErrorCode::NetworkFailed,
                    "无法请求网络宿主退出，请稍后重试。",
                ));
            }
        }
        {
            let mut owned_child = self.nat_child.lock().await;
            if let Some(child) = owned_child.as_mut() {
                match tokio::time::timeout(std::time::Duration::from_secs(4), child.wait()).await {
                    Ok(result) => {
                        result?;
                    }
                    Err(_) => {
                        tokio::time::timeout(std::time::Duration::from_secs(4), child.kill())
                            .await
                            .map_err(|_| {
                                AppError::new(ErrorCode::Timeout, "网络宿主尚未退出，请稍后重试。")
                            })?
                            .map_err(|_| {
                                AppError::new(
                                    ErrorCode::NetworkFailed,
                                    "网络宿主退出未完成，请稍后重试。",
                                )
                            })?;
                    }
                }
                *owned_child = None;
            }
        }
        #[cfg(windows)]
        {
            let mut elevated = self.nat_elevated.lock().await;
            if let Some(handle) = *elevated {
                tokio::task::spawn_blocking(move || wait_for_nat_exit(handle))
                    .await
                    .map_err(|_| {
                        AppError::new(ErrorCode::SystemFailed, "网络宿主退出确认未完成。")
                    })??;
                unsafe {
                    windows_sys::Win32::Foundation::CloseHandle(handle as _);
                }
                *elevated = None;
            }
            if let Some(event) = self.nat_stop.lock().await.take() {
                unsafe {
                    windows_sys::Win32::Foundation::CloseHandle(event as _);
                }
            }
        }
        Ok(())
    }
    pub async fn owns_nat(&self) -> bool {
        let child = self.nat_child.lock().await.is_some();
        let elevated = self.nat_elevated.lock().await.is_some();
        child || elevated
    }
    pub async fn nat_is_elevated(&self) -> bool {
        let elevated = self.nat_elevated.lock().await.is_some();
        let child = self.nat_child.lock().await.is_some();
        elevated || (child && crate::platform::is_elevated())
    }
    pub async fn nat_driver_error(&self, started: Option<SystemTime>) -> Option<NatDriverError> {
        let _lifecycle = self.nat_stopping.lock().await;
        let path = self.data_root.join("engines/Jiaoyuelian/wintun-error.json");
        let data = std::fs::read(&path).ok()?;
        let report: NatDriverError = serde_json::from_slice(&data).ok()?;
        if report.process_id == 0 || report.application_error().is_none() {
            return None;
        }
        if let Some(started) = started {
            let recorded = match report.timestamp {
                Some(_) => report.recorded_at()?,
                None => std::fs::metadata(&path).ok()?.modified().ok()?,
            };
            if recorded < started {
                return None;
            }
        }
        let child_id = self.nat_child.lock().await.as_ref().and_then(Child::id);
        #[cfg(windows)]
        let elevated_id = self.nat_elevated.lock().await.map(|handle| unsafe {
            windows_sys::Win32::System::Threading::GetProcessId(handle as _)
        });
        #[cfg(not(windows))]
        let elevated_id: Option<u32> = None;
        (child_id == Some(report.process_id)
            || elevated_id.filter(|id| *id != 0) == Some(report.process_id))
        .then_some(report)
    }
    pub async fn elevate_owned_nat(&self) -> Result<()> {
        let mut stopping = self.nat_stopping.lock().await;
        if !self.owns_nat().await {
            return Err(AppError::new(ErrorCode::VirtualAdapterFailed, "当前复用的皎月连内核由其他程序启动，LL 无法重启它。请退出原内核后在 LL 中重新登录，以便自动请求网络权限。"));
        }
        *stopping = true;
        self.release_owned_nat_inner().await?;
        *stopping = false;
        let port = self.paths.manifest.jiaoyuelian.port;
        if tokio::time::timeout(
            std::time::Duration::from_millis(400),
            tokio::net::TcpStream::connect(("127.0.0.1", port)),
        )
        .await
        .is_ok_and(|result| result.is_ok())
        {
            return Err(AppError::new(
                ErrorCode::NetworkFailed,
                "本地网络端口仍被占用，无法重启网络宿主。",
            ));
        }
        let host = self.paths.root.join("LLRemote.EngineHost.exe");
        let data = self.data_root.join("engines/Jiaoyuelian");
        #[cfg(windows)]
        {
            *self.nat_stop.lock().await = Some(create_nat_stop_event(port)?);
            let handle = tokio::task::spawn_blocking(move || {
                crate::platform::launch_elevated_nat_host(&host, &data, port)
            })
            .await
            .map_err(|_| AppError::new(ErrorCode::SystemFailed, "网络权限请求未完成。"))??;
            *self.nat_elevated.lock().await = Some(handle);
            for _ in 0..40 {
                if tokio::time::timeout(
                    std::time::Duration::from_millis(400),
                    tokio::net::TcpStream::connect(("127.0.0.1", port)),
                )
                .await
                .is_ok_and(|result| result.is_ok())
                {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
            return Err(AppError::new(
                ErrorCode::NetworkFailed,
                "管理员网络宿主启动后接口未就绪。",
            ));
        }
        #[cfg(not(windows))]
        {
            Err(AppError::new(
                ErrorCode::PermissionDenied,
                "虚拟网卡初始化只支持 Windows。",
            ))
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::{EngineManager, EnginePaths, NatDriverError};
    use std::{
        os::windows::{io::AsRawHandle, process::CommandExt},
        path::PathBuf,
        process::{Command, Stdio},
    };
    use windows_sys::Win32::{Foundation::DuplicateHandle, System::Threading::GetCurrentProcess};

    #[test]
    fn adapter_setup_diagnostics_preserve_native_codes_and_reject_cleanup_messages() {
        let report: NatDriverError = serde_json::from_value(serde_json::json!({
            "stage":"adapterSetup", "processId":42, "elevated":true,
            "win32Code":2, "problemCode":31, "ntStatus":0xc00002f0u32,
        }))
        .unwrap();
        let error = report.application_error().unwrap();
        assert_eq!(error.code, crate::error::ErrorCode::VirtualAdapterFailed);
        assert!(error.message.contains("接口注册"));
        assert!(error.message.contains("Windows 错误 2"));
        assert!(error.message.contains("设备问题 31"));
        assert!(error.message.contains("0xC00002F0"));
        assert!(!error.message.contains("管理员权限"));
        let mut invalid = report;
        invalid.stage = "cleanup".into();
        assert!(invalid.application_error().is_none());
        invalid.stage = "untrusted-stage-with-secret".into();
        assert!(invalid.application_error().is_none());
    }

    #[tokio::test]
    async fn driver_error_files_must_match_the_tracked_pid_and_current_operation() {
        use std::time::{Duration, SystemTime};
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let data = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(EnginePaths::open(root).unwrap(), data.path().into());
        let child = tokio::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 60",
            ])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let process_id = child.id().unwrap();
        *manager.nat_child.lock().await = Some(child);
        let directory = data.path().join("engines/Jiaoyuelian");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("wintun-error.json");
        let started = SystemTime::now();
        let unix = started.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let timestamp =
            (unix.as_secs() + 11_644_473_600) * 10_000_000 + u64::from(unix.subsec_nanos() / 100);
        let mut report = serde_json::json!({
            "stage":"adapterSetup", "processId":process_id, "timestamp":timestamp,
            "elevated":true, "win32Code":2, "problemCode":31, "ntStatus":0xc00002f0u32,
        });
        let write = |report: &serde_json::Value| {
            std::fs::write(&path, serde_json::to_vec(report).unwrap()).unwrap()
        };
        write(&report);
        let current = manager
            .nat_driver_error(Some(started - Duration::from_millis(1)))
            .await;
        assert!(current.is_some());
        assert!(manager
            .nat_driver_error(Some(started + Duration::from_secs(1)))
            .await
            .is_none());
        report["processId"] = serde_json::json!(0);
        write(&report);
        assert!(manager.nat_driver_error(None).await.is_none());
        report["processId"] = serde_json::json!(process_id + 1);
        write(&report);
        assert!(manager.nat_driver_error(None).await.is_none());
        report["processId"] = serde_json::json!(process_id);
        report["stage"] = serde_json::json!("cleanup");
        write(&report);
        assert!(manager.nat_driver_error(None).await.is_none());
        report["stage"] = serde_json::json!("adapterSetup");
        write(&report);
        let mut child = manager.nat_child.lock().await.take().unwrap();
        let mut query_handle = std::ptr::null_mut();
        assert_ne!(
            unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    child.raw_handle().unwrap() as _,
                    GetCurrentProcess(),
                    &mut query_handle,
                    0x00101000,
                    0,
                    0,
                )
            },
            0
        );
        *manager.nat_elevated.lock().await = Some(query_handle as isize);
        child.kill().await.unwrap();
        child.wait().await.unwrap();
        *manager.nat_child.lock().await = Some(child);
        // The tracked process handle retains its PID after native exit.
        assert!(manager.nat_driver_error(None).await.is_some());
        manager.release_owned_nat().await.unwrap();
        assert!(manager.nat_driver_error(None).await.is_none());
    }

    #[tokio::test]
    async fn failed_host_shutdown_retains_ownership_until_exit_is_confirmed() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let data = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(EnginePaths::open(root).unwrap(), data.path().into());
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 60",
            ])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut wait_only = std::ptr::null_mut();
        // A wait-only handle models a shutdown failure without launching an
        // elevated process or changing any network state.
        let duplicated = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                child.as_raw_handle() as _,
                GetCurrentProcess(),
                &mut wait_only,
                0x00100000, // SYNCHRONIZE
                0,
                0,
            )
        };
        if duplicated == 0 {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Could not duplicate an isolated child-process handle");
        }
        *manager.nat_elevated.lock().await = Some(wait_only as isize);
        let first = manager.release_owned_nat().await;
        let retained = manager.owns_nat().await;
        let stopping = *manager.nat_stopping.lock().await;
        let _ = child.kill();
        let _ = child.wait();
        let second = manager.release_owned_nat().await;

        assert!(first.is_err());
        assert!(retained);
        assert!(stopping);
        assert!(second.is_ok());
        assert!(!manager.owns_nat().await);
        assert!(!*manager.nat_stopping.lock().await);
    }
}
