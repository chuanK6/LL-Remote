use crate::{
    controls,
    engines::{
        moonlight::{AppList, MoonlightAdapter},
        nat::{NatAdapter, NetConnection},
        sunshine::{probe_host, validate_address, HostInfo, SunshineAdapter},
        EngineManager, EnginePaths,
    },
    error::{AppError, ErrorCode, Result},
    host_audio::HostAudio,
    model::*,
    platform::{self, PowerManager},
    storage::{SecretRecord, Store, Vault},
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::{Duration, SystemTime},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    process::Child,
    sync::{Mutex, Semaphore},
};
use tokio_util::sync::CancellationToken;

fn host_candidates(
    explicit: Option<&str>,
    reported: &[String],
    saved: Option<&str>,
    discovered: &[String],
    local_addresses: &[String],
) -> Result<Vec<String>> {
    let local_ips: Vec<std::net::IpAddr> = local_addresses
        .iter()
        .filter_map(|address| address.parse().ok())
        .collect();
    let mut result = vec![];
    let mut add = |address: &str, explicit: bool| -> Result<()> {
        let (host, port) = match validate_address(address.trim()) {
            Ok(target) => target,
            Err(error) if explicit => return Err(error),
            Err(_) => return Ok(()),
        };
        let host = host.trim_matches(['[', ']']);
        if host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| local_ips.contains(&ip))
        {
            if explicit {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "填写的是本机组网地址，请填写对方电脑的组网地址。",
                ));
            }
            return Ok(());
        }
        let target = if port == 47989 {
            host.to_ascii_lowercase()
        } else if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{}:{port}", host.to_ascii_lowercase())
        };
        if !result.contains(&target) {
            result.push(target);
        }
        Ok(())
    };
    if let Some(address) = explicit {
        add(address, true)?;
    }
    for address in reported {
        add(address, false)?;
    }
    if let Some(address) = saved {
        add(address, false)?;
    }
    for address in discovered {
        add(address, false)?;
    }
    Ok(result)
}
use zeroize::Zeroizing;

mod direct;
mod role_switch;
mod shortcuts;

#[derive(Clone)]
struct RoleResume {
    session_id: String,
    username: Option<String>,
    connection_password: Zeroizing<String>,
    web_password: Zeroizing<String>,
    previous_auto_pause: bool,
    intent_epoch: u64,
}

pub struct SessionData {
    pub snapshot: SessionSnapshot,
    pub cancel: CancellationToken,
    pub child: Option<Child>,
    pub verified: Arc<AtomicBool>,
    pub address: Option<String>,
    pub pipe_name: String,
    pub token: Zeroizing<String>,
    pub changing: bool,
    direct: bool,
    role_resume: Option<RoleResume>,
    role_conflict_address: Option<String>,
    role_switch_prompt: Option<tokio::sync::oneshot::Sender<bool>>,
}
pub struct Runtime {
    pub app: AppHandle,
    pub store: StdMutex<Store>,
    pub vault: Vault,
    pub engines: EngineManager,
    pub nat: NatAdapter,
    direct_acceptance: Mutex<Option<direct::Acceptance>>,
    pub sunshine: SunshineAdapter,
    pub moonlight: MoonlightAdapter,
    pub power: StdMutex<PowerManager>,
    host_audio: StdMutex<HostAudio>,
    pub session: Mutex<SessionData>,
    pub overlay: Mutex<controls::Overlay>,
    stream_stop: Mutex<()>,
    session_update: Mutex<()>,
    pub gate: Arc<Semaphore>,
    pub portable: bool,
    pub executable: PathBuf,
    pub exiting: AtomicBool,
    pub administrative_lock: Mutex<()>,
    acceptance_firewall_pending: AtomicBool,
    acceptance_firewall_warning: StdMutex<Option<String>>,
    pub autostart_warning: StdMutex<Option<String>>,
    startup_network_warning: StdMutex<Option<String>>,
    pause_automatic_acceptance: AtomicBool,
    role_switch_warning: StdMutex<Option<String>>,
    acceptance_intent_epoch: AtomicU64,
    pub moonlight_settings_target: StdMutex<Option<(String, Option<String>)>>,
    pending_pairing_ids: StdMutex<Vec<String>>,
}
impl Runtime {
    pub fn create(app: AppHandle) -> Result<Arc<Self>> {
        let executable = std::env::current_exe()?;
        let exe_root = executable.parent().unwrap();
        let resource_root = if cfg!(debug_assertions) {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .to_path_buf()
        } else {
            app.path()
                .resource_dir()
                .map_err(|_| AppError::new(ErrorCode::ComponentMissing, "无法定位随包资源。"))?
        };
        let portable = exe_root.join("portable.flag").exists()
            || std::env::args().any(|arg| arg == "--portable");
        let data_root =
            if cfg!(debug_assertions) && std::env::var_os("LLREMOTE_TEST_DATA").is_some() {
                PathBuf::from(std::env::var_os("LLREMOTE_TEST_DATA").unwrap())
            } else if portable {
                exe_root.join("Data")
            } else {
                PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
                    AppError::new(ErrorCode::StorageFailed, "无法定位当前用户的数据目录。")
                })?)
                .join("LLRemote")
            };
        let mut store = Store::open(data_root.clone())?;
        let vault = Vault::new(&data_root);
        if store.preferences.setup_complete
            && vault.contains(&Vault::reference("local", "sunshine-admin"))
            && vault
                .get(&Vault::reference("local", "sunshine-admin"))
                .is_err()
        {
            store.warning = Some(
                "本机管理凭据不可用。在 LL 内确认 PIN 或修改采集设置前，请验证 Sunshine 账号。"
                    .into(),
            );
        }
        let paths = EnginePaths::open(resource_root)?;
        let sunshine = SunshineAdapter::new(
            paths.clone(),
            data_root.clone(),
            store.preferences.reuse_existing,
        )?;
        let nat = NatAdapter::new(
            paths.manifest.jiaoyuelian.port,
            paths.manifest.jiaoyuelian.script_sha256.clone(),
        ).with_local_name(
            std::env::var("COMPUTERNAME").ok()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty()),
        );
        let legacy_moonlight = store.preferences.reuse_existing
            && !platform::imported_moonlight_hosts(&mut store.preferences.stream.clone())
                .is_empty();
        let moonlight = MoonlightAdapter::new(
            paths.clone(),
            (!legacy_moonlight).then(|| data_root.join("engines/Moonlight")),
        );
        let mut power = PowerManager::new();
        if let Err(error) = power.apply(
            store.preferences.prevent_sleep,
            store.preferences.keep_display_awake,
        ) {
            store.warning = Some(error.message);
        }
        let mut autostart_warning = platform::migrate_autostart(&executable).err().map(|error| error.message);
        let registered = platform::autostart_enabled(&executable);
        if store.preferences.autostart && !registered {
            match platform::set_autostart(true, &executable) {
                Ok(()) => {}
                Err(error) => {
                    autostart_warning = Some(error.message);
                    store.preferences.autostart = false;
                }
            }
        } else {
            store.preferences.autostart = registered;
        }
        let host_audio = HostAudio::new(data_root.clone());
        Ok(Arc::new(Self {
            app,
            store: StdMutex::new(store),
            vault,
            engines: EngineManager::new(paths, data_root),
            nat,
            direct_acceptance: Mutex::new(None),
            sunshine,
            moonlight,
            power: StdMutex::new(power),
            host_audio: StdMutex::new(host_audio),
            session: Mutex::new(SessionData {
                snapshot: SessionSnapshot {
                    stage: "idle".into(),
                    ..SessionSnapshot::default()
                },
                cancel: CancellationToken::new(),
                child: None,
                verified: Arc::new(AtomicBool::new(false)),
                address: None,
                pipe_name: String::new(),
                token: Zeroizing::new(String::new()),
                changing: false,
                direct: false,
                role_resume: None,
                role_conflict_address: None,
                role_switch_prompt: None,
            }),
            gate: Arc::new(Semaphore::new(1)),
            portable,
            executable,
            exiting: AtomicBool::new(false),
            administrative_lock: Mutex::new(()),
            overlay: Mutex::default(),
            stream_stop: Mutex::new(()),
            session_update: Mutex::new(()),
            acceptance_firewall_pending: AtomicBool::new(false),
            acceptance_firewall_warning: StdMutex::new(None),
            autostart_warning: StdMutex::new(autostart_warning),
            startup_network_warning: StdMutex::new(None),
            pause_automatic_acceptance: AtomicBool::new(false),
            role_switch_warning: StdMutex::new(None),
            acceptance_intent_epoch: AtomicU64::new(0),
            moonlight_settings_target: StdMutex::new(None),
            pending_pairing_ids: StdMutex::new(vec![]),
        }))
    }
    pub fn store(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        self.store.lock().map_err(|_| {
            AppError::new(
                ErrorCode::SystemFailed,
                "应用数据锁不可用，请重新启动 LL远程。",
            )
        })
    }
    pub fn reconcile_sunshine(&self, sunshine: &SunshineStatus, account: &Account) -> Result<()> {
        let mut store = self.store()?;
        let previous = (
            store.preferences.setup_complete,
            store.preferences.sunshine_username.clone(),
            store.preferences.onboarding_complete,
        );
        if sunshine.account_initialized || sunshine.credential_valid {
            store.preferences.setup_complete = true;
        } else if sunshine.ready {
            // Only a confirmed live response can invalidate a completed step.
            store.preferences.setup_complete = false;
            store.preferences.sunshine_username = None;
        }
        if let Some(username) = &sunshine.account_username {
            store.preferences.sunshine_username = Some(username.clone());
        }
        if store.preferences.setup_complete && account.authenticated {
            store.preferences.onboarding_complete = true;
        }
        if !store.preferences.first_connection_complete
            && store
                .account_data
                .history
                .iter()
                .any(|entry| entry.result == "success")
        {
            store.preferences.first_connection_complete = true;
            store.save_preferences()?;
        }
        if previous
            != (
                store.preferences.setup_complete,
                store.preferences.sunshine_username.clone(),
                store.preferences.onboarding_complete,
            )
        {
            store.save_preferences()?;
        }
        Ok(())
    }
    pub async fn require_sunshine_account(&self) -> Result<()> {
        let sunshine = self.sunshine.status(&self.vault).await;
        self.reconcile_sunshine(&sunshine, &self.nat.cached_account().await)?;
        if self.store()?.preferences.setup_complete {
            return Ok(());
        }
        Err(AppError::new(
            ErrorCode::SunshineAccountRequired,
            "Sunshine 管理账号尚未设置，请先完成账号设置。高级设置中的已有账号会自动同步。",
        ))
    }
    pub async fn local_server_passwords(&self) -> LocalServerPasswords {
        let mut passwords = self.nat.local_server_passwords().await;
        if passwords.connection_password.is_empty() {
            if let Ok(secret) = self
                .vault
                .get(&Vault::reference("local", "jiaoyue-server-password"))
            {
                passwords.connection_password = secret.password.clone();
            }
        }
        if passwords.web_password.is_empty() {
            if let Ok(secret) = self
                .vault
                .get(&Vault::reference("local", "jiaoyue-web-password"))
            {
                passwords.web_password = secret.password.clone();
            }
        }
        passwords
    }
    pub async fn elevate_network_host(&self, cancel: &CancellationToken) -> Result<()> {
        if !self.engines.owns_nat().await {
            return Err(AppError::new(
                ErrorCode::VirtualAdapterFailed,
                "正在复用的皎月连进程权限不足。请关闭原皎月连进程，再以管理员权限启动 LL远程。",
            ));
        }
        let login = self
            .nat
            .restart_login_secret()
            .await
            .or_else(|| {
                self.vault
                    .get(&Vault::reference("local", "jiaoyue-login"))
                    .ok()
            })
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::LoginRequired,
                    "请在 LL远程中登录皎月连，以便在管理员网络宿主中恢复登录。",
                )
            })?;
        let account = self.nat.cached_account().await;
        let passwords = self.local_server_passwords().await;
        self.nat.disconnect_for_restart().await;
        self.engines.elevate_owned_nat().await?;
        if cancel.is_cancelled() {
            return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
        }
        match self.nat.snapshot().await {
            Err(error) if error.code == ErrorCode::LocalAuthorizationRequired => {
                self.nat.authorize(&passwords.web_password).await?;
            }
            Err(error) => return Err(error),
            Ok(_) => {}
        }
        self.nat
            .login(&login.username, &login.password, cancel)
            .await?;
        self.nat
            .remember_server_passwords(&passwords.connection_password, &passwords.web_password)
            .await;
        if account.server_running {
            self.nat
                .set_server(
                    true,
                    &passwords.connection_password,
                    &passwords.web_password,
                )
                .await?;
        }
        Ok(())
    }
    pub async fn start_accepting(
        &self,
        connection_password: &str,
        web_password: &str,
        started: SystemTime,
    ) -> Result<()> {
        self.require_no_direct_acceptance().await?;
        let (account, _) = self.nat.snapshot().await?;
        if !account.authenticated {
            return Err(AppError::new(ErrorCode::LoginRequired, "请先登录皎月连。"));
        }
        let server_was_running = account.server_running;
        let sunshine_was_running = self.sunshine.running().await;
        if self.exiting.load(Ordering::SeqCst) {
            return Err(AppError::new(ErrorCode::Canceled, "接受控制启动已取消。"));
        }
        let start_network = async {
            let mut result = self
                .nat
                .set_server(true, connection_password, web_password)
                .await;
            if result.as_ref().is_err_and(|error| {
                matches!(
                    error.code,
                    ErrorCode::VirtualAdapterFailed | ErrorCode::PermissionDenied
                )
            }) && !self.engines.nat_is_elevated().await
            {
                result = if self.gate.available_permits() == 0 {
                    Err(AppError::new(
                        ErrorCode::Busy,
                        "请先断开当前远控，再恢复组网权限。",
                    ))
                } else {
                    match self.elevate_network_host(&CancellationToken::new()).await {
                        Ok(()) => {
                            self.nat
                                .set_server(true, connection_password, web_password)
                                .await
                        }
                        Err(error) => Err(error),
                    }
                };
            }
            match result {
                Ok(()) => Ok(()),
                Err(error) => Err(self.network_error(error, started).await),
            }
        };
        let start_sunshine = async {
            self.sunshine.start().await?;
            Ok(())
        };
        let (network_result, sunshine_result) = tokio::join!(start_network, start_sunshine);
        let result = network_result.and(sunshine_result).and_then(|()| {
            if self.exiting.load(Ordering::SeqCst) {
                return Err(AppError::new(ErrorCode::Canceled, "接受控制启动已取消。"));
            }
            Ok(())
        });
        if let Err(error) = result {
            if !sunshine_was_running {
                if let Err(stop_error) = self.sunshine.stop().await {
                    tracing::warn!(code = ?stop_error.code, "Could not roll back Sunshine startup");
                }
            }
            if !server_was_running {
                if let Err(stop_error) = self.nat.set_server(false, "", "").await {
                    tracing::warn!(code = ?stop_error.code, "Could not roll back network server startup");
                }
            }
            return Err(error);
        }
        Ok(())
    }

    pub async fn stop_accepting(&self) -> Result<AppSnapshot> {
        // Revoke the pending restore immediately, even if another admin
        // operation must finish before the native stop can acquire its lock.
        self.pause_automatic_acceptance(true);
        self.cancel_role_restore().await;
        let _guard = self.administrative_lock.lock().await;
        self.cancel_role_restore().await;
        self.pause_automatic_acceptance(true);
        self.stop_direct_acceptance().await?;
        let account = self.nat.cached_account().await;
        let accepting = self.store()?.preferences.accept_control;
        if account.server_running {
            self.nat.set_server(false, "", "").await?;
        }
        if account.server_running || accepting {
            self.sunshine.stop().await?;
        }
        self.store()?.preferences.accept_control = false;
        let warning = self.sunshine.restore_vpn_firewall().await.err().map(|error| {
            tracing::warn!(code = ?error.code, "Local acceptance stopped but VPN firewall restoration did not complete");
            format!("本机接受控制已停止，Sunshine 防火墙规则恢复尚未完成。{}", error.message)
        });
        self.set_acceptance_firewall_warning(warning);
        let snapshot = self.snapshot().await?;
        let _ = self.app.emit_to("main", "ll:snapshot", &snapshot);
        Ok(snapshot)
    }

    pub fn set_acceptance_firewall_warning(&self, warning: Option<String>) {
        if let Ok(mut value) = self.acceptance_firewall_warning.lock() {
            *value = warning;
        }
    }

    pub fn schedule_acceptance_firewall(self: &Arc<Self>) {
        if self
            .acceptance_firewall_pending
            .swap(true, Ordering::SeqCst)
        {
            return;
        }
        self.set_acceptance_firewall_warning(Some(
            "本机接受控制已开启；正在确认 Sunshine 的组网访问。".into(),
        ));
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            {
                // Serialize with stop/quit so a late preparation cannot recreate
                // rules after those operations restore the previous state.
                let _guard = runtime.administrative_lock.lock().await;
                let accepting = runtime
                    .store()
                    .is_ok_and(|store| store.preferences.accept_control);
                if !runtime.exiting.load(Ordering::SeqCst) && accepting {
                    let account = runtime.nat.cached_account().await;
                    if account.server_running {
                        let result = match account.local_vpn_address {
                            Some(address) => runtime.sunshine.prepare_vpn_firewall(&address).await,
                            None => Err(AppError::new(
                                ErrorCode::AddressRequired,
                                "本机组网地址尚未取得，Sunshine 的组网访问暂未确认。",
                            )),
                        };
                        runtime.set_acceptance_firewall_warning(result.err().map(|error| {
                            tracing::warn!(code = ?error.code, "Local acceptance is running but Sunshine VPN firewall preparation did not complete");
                            format!("本机接受控制已开启；Sunshine 组网访问尚未确认。{}", error.message)
                        }));
                    }
                }
                runtime
                    .acceptance_firewall_pending
                    .store(false, Ordering::SeqCst);
            }
            if !runtime.exiting.load(Ordering::SeqCst) {
                if let Ok(snapshot) = runtime.snapshot().await {
                    let _ = runtime.app.emit_to("main", "ll:snapshot", snapshot);
                }
            }
        });
    }
    pub async fn network_error(&self, error: AppError, started: SystemTime) -> AppError {
        if !matches!(
            error.code,
            ErrorCode::VirtualAdapterFailed
                | ErrorCode::Timeout
                | ErrorCode::NetworkFailed
                | ErrorCode::PermissionDenied
        ) {
            return error;
        }
        let Some(driver) = self.engines.nat_driver_error(Some(started)).await else {
            tracing::warn!(code = ?error.code, "Jiaoyuelian network initialization failed");
            return error;
        };
        tracing::error!(
            stage = driver.stage,
            elevated = driver.elevated,
            win32_code = driver.win32_code,
            problem_code = driver.problem_code,
            nt_status = driver.nt_status,
            "Wintun native initialization failed"
        );
        driver.application_error().unwrap_or(error)
    }
    pub async fn snapshot(&self) -> Result<AppSnapshot> {
        self.reconcile_moonlight_settings()?;
        let sunshine = self.sunshine.status(&self.vault).await;
        let owns_nat = self.engines.owns_nat().await;
        let (account, network) = self.nat.cached_state().await;
        self.reconcile_sunshine(&sunshine, &account)?;
        let session = self.session.lock().await.snapshot.clone();
        let direct_acceptance = self.direct_acceptance.lock().await.as_ref().map(|acceptance| acceptance.status.clone());
        let show_pairing = {
            let mut previous = self
                .pending_pairing_ids
                .lock()
                .map_err(|_| AppError::new(ErrorCode::SystemFailed, "配对通知状态不可用。"))?;
            let ids: Vec<String> = sunshine
                .pending_pairings
                .iter()
                .map(|pairing| pairing.id.clone())
                .collect();
            let changed = ids.iter().any(|id| !previous.contains(id));
            *previous = ids;
            changed
        };
        if show_pairing && !self.exiting.load(Ordering::SeqCst) {
            crate::show_main_window(&self.app);
        }
        let components = vec![
            ComponentStatus {
                name: "皎月连".into(),
                available: self
                    .engines
                    .paths
                    .executable(&self.engines.paths.manifest.jiaoyuelian)
                    .is_ok(),
                running: account.protocol_supported,
                ready: account.authenticated,
                owned: owns_nat,
                version: self.engines.paths.manifest.jiaoyuelian.version.clone(),
                detail: if account.protocol_supported {
                    "已验证本机接口版本"
                } else {
                    "使用随包组件，可在高级设置中检查原界面"
                }
                .into(),
            },
            ComponentStatus {
                name: "Moonlight".into(),
                available: self.moonlight.executable().is_ok(),
                running: session.stage == "streaming",
                ready: self.moonlight.executable().is_ok(),
                owned: session.id.is_some(),
                version: self.engines.paths.manifest.moonlight.version.clone(),
                detail: if self.engines.paths.manifest.moonlight.extension_protocol == 1 {
                    "含设置直达与会话 IPC 扩展"
                } else {
                    "原 CLI 可用，会话扩展尚未准备"
                }
                .into(),
            },
            ComponentStatus {
                name: "Sunshine".into(),
                available: self
                    .engines
                    .paths
                    .executable(&self.engines.paths.manifest.sunshine)
                    .is_ok()
                    || sunshine.external,
                running: sunshine.running,
                ready: sunshine.ready,
                owned: !sunshine.external && sunshine.running,
                version: self.engines.paths.manifest.sunshine.version.clone(),
                detail: if sunshine.external {
                    "复用已有 Windows 服务与配对"
                } else {
                    "使用受管理的随包进程"
                }
                .into(),
            },
        ];
        let store = self.store()?;
        let mut devices = store.account_data.devices.clone();
        for device in &mut devices {
            device.account_online &= account.authenticated;
        }
        if let Some(network) = network {
            let connected_id = devices
                .iter()
                .find(|device| {
                    session.device_id.as_deref() == Some(&device.id)
                        && !session
                            .error
                            .as_ref()
                            .is_some_and(|error| error.code == ErrorCode::PermissionDenied)
                        && network.matches_device(device)
                })
                .or_else(|| {
                    devices
                        .iter()
                        .filter(|device| network.matches_device(device))
                        .max_by_key(|device| {
                            (
                                &device.observed_at,
                                &device.last_connected_at,
                                device.uuid.is_some(),
                            )
                        })
                })
                .map(|device| device.id.clone());
            for device in &mut devices {
                device.network_connected = connected_id.as_deref() == Some(&device.id);
            }
        }
        let power_status = self
            .power
            .lock()
            .map(|power| power.status.clone())
            .unwrap_or_else(|_| "无法读取".into());
        Ok(AppSnapshot {
            version: env!("CARGO_PKG_VERSION").into(),
            portable: self.portable,
            data_directory: store.root.to_string_lossy().into(),
            preferences: store.preferences.clone(),
            account,
            devices,
            history: store.account_data.history.clone(),
            sunshine,
            direct_acceptance,
            session,
            components,
            power_status,
            storage_warning: self
                .role_switch_warning
                .lock()
                .ok()
                .and_then(|warning| warning.clone())
                .or_else(|| store.warning.clone())
                .or_else(|| {
                    self.autostart_warning
                        .lock()
                        .ok()
                        .and_then(|value| value.clone())
                })
                .or_else(|| {
                    self.startup_network_warning
                        .lock()
                        .ok()
                        .and_then(|value| value.clone())
                })
                .or_else(|| {
                    self.acceptance_firewall_warning
                        .lock()
                        .ok()
                        .and_then(|value| value.clone())
                }),
        })
    }
    pub async fn refresh(&self) -> Result<AppSnapshot> {
        match self.nat.snapshot().await {
            Ok((account, hosts)) => {
                let connection = self.nat.cached_connection().await;
                let active_host = connection.as_ref().and_then(|connection| {
                    hosts
                        .iter()
                        .find(|host| connection.peer_id.as_deref() == Some(&host.id))
                });
                let verified = if let Some(host) = active_host {
                    if let Some(address) = &host.address {
                        probe_host(address, None, &CancellationToken::new())
                            .await
                            .ok()
                            .map(|info| (host.id.clone(), info))
                    } else {
                        None
                    }
                } else {
                    None
                };
                let mut store = self.store()?;
                if account.authenticated {
                    store.switch_account(account.username.as_deref())?;
                }
                if account.authenticated {
                    if store.preferences.reuse_existing && !store.preferences.moonlight_imported {
                        let mut defaults = store.preferences.stream.clone();
                        let imported = platform::imported_moonlight_hosts(&mut defaults);
                        if defaults.validate().is_ok() {
                            store.preferences.stream = defaults;
                        }
                        for device in imported {
                            store.merge_device(device);
                        }
                        store.preferences.moonlight_imported = true;
                        store.save_preferences()?;
                    }
                    let mut directory = Vec::new();
                    for host in hosts.into_iter().filter(|host| host.online) {
                        let info = verified.as_ref().filter(|(id, _)| id == &host.id).map(|(_, info)| info);
                        let name = info.map_or_else(|| host.name.clone(), |info| info.name.clone());
                        let device = Device {
                            host_name: name.clone(),
                            display_name: name,
                            source: "account".into(),
                            uuid: info.map(|info| info.uuid.clone()),
                            nat_id: Some(host.id),
                            account_host_name: (!host.name.trim().is_empty()).then_some(host.name),
                            address: host.address,
                            status: "online".into(),
                            service_ready: host.service_ready,
                            observed_at: Some(now()),
                            preferences: store.preferences.stream.clone(),
                            ..Device::default()
                        };
                        directory.push(device);
                    }
                    store.merge_account_directory(directory);
                    store.save_devices()?;
                } else {
                    for device in &mut store.account_data.devices {
                        device.account_online = false;
                    }
                }
            }
            Err(error) => {
                tracing::debug!(code = ?error.code, "Native refresh unavailable");
                for device in &mut self.store()?.account_data.devices {
                    device.account_online = false;
                }
            }
        }
        let snapshot = self.snapshot().await?;
        let _ = self.app.emit_to("main", "ll:snapshot", &snapshot);
        Ok(snapshot)
    }
    pub fn write_moonlight_preferences(
        &self,
        device_id: Option<&str>,
        preferences: &StreamPreferences,
    ) -> Result<()> {
        let mut target = self
            .moonlight_settings_target
            .lock()
            .map_err(|_| AppError::new(ErrorCode::SystemFailed, "Moonlight 设置同步不可用。"))?;
        let scope = {
            let store = self.store()?;
            if let Some(id) = device_id {
                store.device(id)?;
            }
            store.account_scope.clone()
        };
        self.moonlight.write_preferences(preferences)?;
        *target = Some((scope, device_id.map(str::to_owned)));
        Ok(())
    }
    fn reconcile_moonlight_settings(&self) -> Result<()> {
        let mut target = self
            .moonlight_settings_target
            .lock()
            .map_err(|_| AppError::new(ErrorCode::SystemFailed, "Moonlight 设置同步不可用。"))?;
        let Some((scope, device_id)) = target.as_ref() else {
            return Ok(());
        };
        let mut store = self.store()?;
        if &store.account_scope != scope {
            return Ok(());
        }
        let previous = if let Some(id) = device_id {
            let Some(device) = store.account_data.devices.iter().find(|device| &device.id == id) else {
                *target = None;
                return Ok(());
            };
            device.preferences.clone()
        } else {
            store.preferences.stream.clone()
        };
        let Some(preferences) = self.moonlight.read_preferences(&previous)? else {
            return Ok(());
        };
        if preferences == previous {
            return Ok(());
        }
        if let Some(id) = device_id {
            if let Some(device) = store
                .account_data
                .devices
                .iter_mut()
                .find(|device| &device.id == id)
            {
                device.preferences = preferences;
            }
            store.save_devices()?;
        } else {
            store.preferences.stream = preferences;
            store.save_preferences()?;
        }
        Ok(())
    }
    pub async fn stage(&self, stage: &str, error: Option<AppError>) {
        let snapshot = {
            let mut session = self.session.lock().await;
            if session.cancel.is_cancelled()
                && !["canceled", "disconnected", "disconnectFailed"].contains(&stage)
            {
                return;
            }
            session.snapshot.stage = stage.into();
            session.snapshot.error = error;
            if stage != "pairing" {
                session.snapshot.pin = None;
            }
            session.snapshot.clone()
        };
        let error_code = snapshot.error.as_ref().map(|error| &error.code);
        tracing::info!(stage, ?error_code, "Connection state changed");
        self.emit_session(&snapshot);
    }
    pub fn emit_session(&self, snapshot: &SessionSnapshot) {
        let _ = self.app.emit_to("main", "ll:session", snapshot);
        let _ = self.app.emit_to("session-controls", "ll:session", snapshot);
        let _ = self.app.emit_to("session-performance", "ll:performance", &snapshot.metrics);
    }
    pub async fn connect(self: &Arc<Self>, request: ConnectRequest) -> Result<SessionSnapshot> {
        self.require_sunshine_account().await?;
        if request.device_id.is_none() && request.peer_code.is_none() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "请选择设备或填写识别码。",
            ));
        }
        if let Some(code) = &request.peer_code {
            if !(4..=32).contains(&code.len()) || !code.bytes().all(|b| b.is_ascii_digit()) {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "识别码须为 4–32 位数字。",
                ));
            }
            let address = request
                .address
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AppError::new(
                        ErrorCode::AddressRequired,
                        "识别码连接需要填写对方的组网地址。",
                    )
                })?;
            validate_address(address)?;
        }
        let permit =
            self.gate.clone().try_acquire_owned().map_err(|_| {
                AppError::new(ErrorCode::Busy, "已有连接正在处理，请先取消或断开。")
            })?;
        let device = request
            .device_id
            .as_deref()
            .map(|id| self.store()?.device(id))
            .transpose()?;
        if request.peer_code.is_none()
            && device.as_ref().is_some_and(|device| {
                device.peer_code.is_some()
                    && device
                        .address
                        .as_deref()
                        .is_none_or(|address| address.trim().is_empty())
            })
        {
            return Err(AppError::new(
                ErrorCode::AddressRequired,
                "请在协助页面填写此设备的组网地址。",
            ));
        }
        self.begin_connection(request, device, permit, false).await
    }

    async fn begin_connection(
        self: &Arc<Self>,
        request: ConnectRequest,
        device: Option<Device>,
        permit: tokio::sync::OwnedSemaphorePermit,
        direct: bool,
    ) -> Result<SessionSnapshot> {
        let snapshot = {
            let mut session = self.session.lock().await;
            if self.exiting.load(Ordering::SeqCst) {
                return Err(AppError::new(ErrorCode::Busy, "LL远程正在退出。"));
            }
            if ["disconnecting", "disconnectFailed"].contains(&session.snapshot.stage.as_str()) {
                return Err(AppError::new(ErrorCode::Busy, "请先结束上一次连接。"));
            }
            if session.role_resume.is_some() {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "正在延迟检查并恢复本机接受控制，请稍候；也可从托盘停止本机接受控制。",
                ));
            }
            let id = uuid::Uuid::new_v4().to_string();
            session.cancel = CancellationToken::new();
            session.address = None;
            session.child = None;
            session.changing = false;
            session.direct = direct;
            session.role_conflict_address = None;
            session.role_switch_prompt = None;
            session.pipe_name = format!("LLRemote-{id}");
            session.token = Zeroizing::new(format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ));
            session.snapshot = SessionSnapshot {
                id: Some(id),
                device_id: request.device_id.clone(),
                device_name: device.as_ref().map(|d| {
                    if d.display_name.is_empty() {
                        d.host_name.clone()
                    } else {
                        d.display_name.clone()
                    }
                }),
                stage: if direct { "checkingHost" } else { "preparingNetwork" }.into(),
                attempt: 1,
                app: request.app.clone().or_else(|| Some("Desktop".into())),
                extension_available: self.engines.paths.manifest.moonlight.extension_protocol == 1,
                ..SessionSnapshot::default()
            };
            session.snapshot.clone()
        };
        self.emit_session(&snapshot);
        let runtime = self.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let started = SystemTime::now();
            let result = if direct {
                runtime.connect_direct_attempt(&request, device.as_ref()).await
            } else {
                runtime.connect_inner(&request).await
            };
            if let Err(error) = result {
                let (canceled, device_id) = {
                    let session = runtime.session.lock().await;
                    (
                        session.cancel.is_cancelled(),
                        session.snapshot.device_id.clone(),
                    )
                };
                if canceled {
                    runtime.record_failure(
                        &request,
                        &AppError::new(ErrorCode::Canceled, "连接已取消。"),
                        device_id.as_deref(),
                        direct,
                    );
                    return;
                }
                let network_stage =
                    runtime.session.lock().await.snapshot.stage == "preparingNetwork";
                let error = if network_stage {
                    runtime.network_error(error, started).await
                } else {
                    error
                };
                let canceled = error.code == ErrorCode::Canceled;
                runtime.record_failure(&request, &error, device_id.as_deref(), direct);
                if !direct && (canceled || error.code == ErrorCode::PermissionDenied) {
                    runtime.nat.cancel_owned_connection().await;
                }
                runtime
                    .stage(
                        if canceled { "canceled" } else { "failed" },
                        (!canceled).then_some(error),
                    )
                    .await;
                runtime.kill_stream().await;
                controls::close(&runtime.app);
                if let Ok(snapshot) = runtime.snapshot().await {
                    let _ = runtime.app.emit_to("main", "ll:snapshot", snapshot);
                }
            }
            if !direct && !runtime.session.lock().await.cancel.is_cancelled() {
                match runtime.finish_role_session().await {
                    Ok(()) => runtime.schedule_role_restore().await,
                    Err(error) => runtime.stage("disconnectFailed", Some(error)).await,
                }
            }
            if let Ok(snapshot) = runtime.snapshot().await {
                let _ = runtime.app.emit_to("main", "ll:snapshot", snapshot);
            }
        });
        Ok(snapshot)
    }
    async fn connect_inner(self: &Arc<Self>, request: &ConnectRequest) -> Result<()> {
        let cancel = self.session.lock().await.cancel.clone();
        // Address discovery can reveal a collision only after the first tunnel
        // is established. Retry that tunnel once within the same logical session.
        for attempt in 0..2 {
            self.prepare_role_for_connection(request, &cancel).await?;
            let result = self.connect_attempt(request).await;
            if attempt == 0
                && result
                    .as_ref()
                    .is_err_and(|error| error.code == ErrorCode::RoleSwitchRequired)
            {
                let _guard = self.administrative_lock.lock().await;
                self.nat.end_session_connection(None).await?;
                continue;
            }
            return result;
        }
        unreachable!("the second connection attempt always returns")
    }
    async fn connect_attempt(self: &Arc<Self>, request: &ConnectRequest) -> Result<()> {
        let cancel = self.session.lock().await.cancel.clone();
        if cancel.is_cancelled() {
            return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
        }
        let network_guard = self.administrative_lock.lock().await;
        let network_started = SystemTime::now();
        let account = self.restore_jiaoyue(&cancel).await?;
        if !account.authenticated {
            return Err(AppError::new(ErrorCode::LoginRequired, "请先登录皎月连。"));
        }
        self.store()?.switch_account(account.username.as_deref())?;
        let device = request
            .device_id
            .as_deref()
            .map(|id| self.store()?.device(id))
            .transpose()?;
        let mut preferences = device
            .as_ref()
            .map(|d| d.preferences.clone())
            .unwrap_or_else(|| {
                self.store()
                    .map(|s| s.preferences.stream.clone())
                    .unwrap_or_default()
            });
        // Fresh connections open in a window even when migrated/native settings
        // remember fullscreen. Internal reconnects use the active session's
        // preferences, so switching fullscreen during a session still persists
        // through quality changes.
        preferences.window_mode = "windowed".into();
        let code = request
            .peer_code
            .as_deref()
            .or_else(|| device.as_ref().and_then(|d| d.peer_code.as_deref()));
        let nat_id = if code.is_none() {
            device.as_ref().and_then(|d| d.nat_id.as_deref())
        } else {
            None
        };
        let credential = if request.password.is_none() && code.is_some() {
            let device = device.as_ref().ok_or_else(|| {
                AppError::new(
                    ErrorCode::CredentialUnavailable,
                    "首次连接需要填写连接密码。",
                )
            })?;
            if device.credential_state == "invalid" {
                return Err(AppError::new(
                    ErrorCode::PasswordRejected,
                    "保存的连接授权已失效，请先更新连接密码。",
                ));
            }
            let reference = device.credential_ref.as_deref().ok_or_else(|| {
                AppError::new(
                    ErrorCode::CredentialUnavailable,
                    "本机没有可复用凭据，请更新连接密码。",
                )
            })?;
            Some(self.vault.get(reference)?)
        } else {
            None
        };
        let password = request
            .password
            .as_deref()
            .or_else(|| credential.as_ref().map(|s| s.password.as_str()));
        let limit = if self.store()?.preferences.auto_retry {
            3
        } else {
            1
        };
        let network = if nat_id.is_none() && code.is_none() {
            None
        } else {
            if let Some(id) = nat_id {
                let _ =
                    tokio::time::timeout(Duration::from_secs(2), self.nat.host_info(id, &cancel))
                        .await;
            }
            let mut attempt = 0;
            let mut permission_retried = false;
            let mut refreshed_after_offline = false;
            loop {
                attempt += 1;
                self.session.lock().await.snapshot.attempt = attempt;
                self.stage("preparingNetwork", None).await;
                let result = match self.nat.connect(nat_id, code, password, &cancel).await {
                    Ok(connection) => Ok(connection),
                    Err(error) => Err(self.network_error(error, network_started).await),
                };
                match result {
                    Ok(connection) => break Some(connection),
                    Err(error)
                        if error.code == ErrorCode::Offline
                            && !refreshed_after_offline
                            && nat_id.is_some() =>
                    {
                        refreshed_after_offline = true;
                        let id = nat_id.unwrap();
                        if !tokio::time::timeout(
                            Duration::from_secs(3),
                            self.nat.host_info(id, &cancel),
                        )
                        .await
                        .is_ok_and(|host| host.is_ok_and(|host| host.service_ready == Some(true)))
                        {
                            return Err(error);
                        }
                    }
                    Err(error)
                        if matches!(
                            error.code,
                            ErrorCode::VirtualAdapterFailed | ErrorCode::PermissionDenied
                        ) && !permission_retried
                            && !self.engines.nat_is_elevated().await =>
                    {
                        self.nat.cancel_owned_connection().await;
                        self.elevate_network_host(&cancel).await?;
                        permission_retried = true;
                    }
                    Err(error) if error.retryable && attempt < limit => {
                        self.nat.cancel_owned_connection().await;
                        tokio::select! { _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Canceled, "连接已取消。")), _ = tokio::time::sleep(Duration::from_millis(500 * 3u64.pow(attempt - 1))) => {} }
                    }
                    Err(error) => return Err(error),
                }
            }
        };
        drop(network_guard);
        if cancel.is_cancelled() {
            return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
        }
        if let Some(network) = &network {
            let id = {
                let mut store = self.store()?;
                let mut connected = device.clone().unwrap_or_default();
                connected.nat_id = network.peer_id.clone().or(connected.nat_id);
                connected.peer_code = code.map(str::to_owned).or(connected.peer_code);
                connected.observed_at = Some(now());
                if connected.host_name.is_empty() {
                    connected.host_name = code
                        .map(|code| format!("识别码 {code}"))
                        .unwrap_or_else(|| "已连接设备".into());
                    connected.display_name = connected.host_name.clone();
                    connected.source = if code.is_some() { "code" } else { "account" }.into();
                }
                let id = store.merge_device(connected);
                store.save_devices()?;
                id
            };
            self.session.lock().await.snapshot.device_id = Some(id);
        }
        self.stage("checkingHost", None).await;
        if let Ok(snapshot) = self.snapshot().await {
            let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
        }
        let info = self
            .resolve_address(request, device.as_ref(), network.as_ref(), &cancel)
            .await?;
        if cancel.is_cancelled() {
            return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
        }
        let selected_id = self.session.lock().await.snapshot.device_id.clone();
        let id = {
            let mut store = self.store()?;
            let mut connected = Device {
                host_name: info.name.clone(),
                display_name: info.name.clone(),
                source: if code.is_some() { "code" } else { "account" }.into(),
                nat_id: network
                    .as_ref()
                    .and_then(|network| network.peer_id.clone())
                    .or_else(|| device.as_ref().and_then(|device| device.nat_id.clone())),
                uuid: Some(info.uuid.clone()),
                address: Some(info.address.clone()),
                peer_code: code.map(str::to_owned),
                status: if nat_id.is_some() {
                    "online"
                } else {
                    "unknown"
                }
                .into(),
                service_ready: Some(true),
                observed_at: Some(now()),
                preferences: preferences.clone(),
                ..Device::default()
            };
            if let Some(selected) = selected_id.as_deref().and_then(|id| store.device(id).ok()) {
                if selected
                    .uuid
                    .as_deref()
                    .is_none_or(|uuid| uuid.eq_ignore_ascii_case(&info.uuid))
                {
                    connected.id = selected.id;
                }
            }
            if code.is_some() {
                if let Some(password) = password {
                    let reference =
                        Vault::reference(&store.account_scope, &connected.credential_identity()?);
                    self.vault.put(
                        &reference,
                        &SecretRecord {
                            username: String::new(),
                            password: password.into(),
                        },
                    )?;
                    connected.credential_ref = Some(reference);
                    connected.credential_state = "valid".into();
                }
            }
            let id = store.merge_device(connected);
            store.save_devices()?;
            id
        };
        {
            let mut session = self.session.lock().await;
            session.snapshot.device_id = Some(id.clone());
            session.snapshot.device_name = Some(info.name.clone());
            session.address = Some(info.address.clone());
        }
        if let Ok(snapshot) = self.snapshot().await {
            let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
        }
        self.stream_host(request, info, preferences, code, false, &cancel).await
    }

    async fn stream_host(
        self: &Arc<Self>,
        request: &ConnectRequest,
        info: HostInfo,
        preferences: StreamPreferences,
        code: Option<&str>,
        direct: bool,
        cancel: &CancellationToken,
    ) -> Result<()> {
        self.stage("checkingPairing", None).await;
        let apps = match self.moonlight.list_apps(&info.address, Some(&info.uuid), cancel).await? {
            AppList::Paired(apps) => apps,
            AppList::NeedsPairing => {
                let random = uuid::Uuid::new_v4();
                let number =
                    u16::from_le_bytes([random.as_bytes()[0], random.as_bytes()[1]]) % 9000 + 1000;
                let pin = Zeroizing::new(format!("{number:04}"));
                {
                    let mut session = self.session.lock().await;
                    session.snapshot.stage = "pairing".into();
                    session.snapshot.pin = Some(pin.to_string());
                    let snapshot = session.snapshot.clone();
                    self.emit_session(&snapshot);
                }
                let result = self.moonlight.pair(&info.address, Some(&info.uuid), &pin, cancel).await;
                self.session.lock().await.snapshot.pin = None;
                result?;
                self.stage("checkingPairing", None).await;
                self.moonlight.apps_after_pairing(&info.address, &info.uuid, cancel).await?
            }
        };
        let application = request.app.as_deref().unwrap_or("Desktop");
        if !apps.iter().any(|app| app.name == application) {
            return Err(AppError::new(
                ErrorCode::StreamFailed,
                "对方 Sunshine 尚未发布此应用，请在快速启动页面刷新。",
            ));
        }
        if cancel.is_cancelled() {
            return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
        }
        self.stage("startingStream", None).await;
        {
            let mut session = self.session.lock().await;
            if cancel.is_cancelled() {
                return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
            }
            let process = self.moonlight.spawn_stream(
                &info.address,
                Some(&info.uuid),
                application,
                &preferences,
                &session.pipe_name,
                &session.token,
                &self.store()?.preferences.shortcuts,
            )?;
            self.watch_stream_shortcuts(&session, &process.child, process.shortcut_events);
            session.child = Some(process.child);
            session.verified = process.verified;
            session.address = Some(info.address.clone());
            session.snapshot.preferences = Some(preferences.clone());
        }
        self.await_stream(cancel).await?;
        let id = {
            let mut store = self.store()?;
            let mut connected = store
                .account_data
                .devices
                .iter()
                .find(|device| {
                    (device.source == "direct") == direct && device
                        .uuid
                        .as_deref()
                        .is_some_and(|uuid| uuid.eq_ignore_ascii_case(&info.uuid))
                })
                .cloned()
                .ok_or_else(|| {
                    AppError::new(
                        ErrorCode::Offline,
                        "已连接设备的身份记录不可用，请刷新后重试。",
                    )
                })?;
            connected.last_connected_at = Some(now());
            connected.preferences = preferences;
            connected.apps = apps;
            let id = store.merge_device(connected);
            store.account_data.history.push(HistoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                device_id: Some(id.clone()),
                peer_code: code.map(str::to_owned),
                connected_at: now(),
                method: if direct {
                    "IP 直连"
                } else if code.is_some() {
                    "识别码"
                } else {
                    "同账号 / 已有连接"
                }
                .into(),
                result: "success".into(),
                error_code: None,
            });
            let excess = store.account_data.history.len().saturating_sub(2000);
            if excess > 0 {
                store.account_data.history.drain(..excess);
            }
            store.save_devices()?;
            Self::record_first_connection(&mut store);
            id
        };
        {
            let mut session = self.session.lock().await;
            session.snapshot.device_id = Some(id);
            session.snapshot.device_name = Some(info.name.clone());
            session.snapshot.started_at = Some(now());
        }
        self.stage("streaming", None).await;
        if let Some(owner) = self.app.get_webview_window("main") {
            if let Err(error) = controls::show_remote_window(self, &owner).await {
                tracing::warn!(code = ?error.code, "Could not activate the stream window");
            }
        }
        let show_ball = self.store()?.preferences.show_float_ball;
        if show_ball {
            if let Err(error) = controls::show(self).await {
                tracing::warn!(code = ?error.code, "Session controls unavailable");
            }
        }
        if let Err(error) = controls::show_performance(self).await {
            tracing::warn!(code = ?error.code, "Session performance window unavailable");
        }
        if let Ok(snapshot) = self.snapshot().await {
            let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
        }
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
            let (exit, changing, ipc) = {
                let mut session = self.session.lock().await;
                let exit = session
                    .child
                    .as_mut()
                    .and_then(|child| child.try_wait().ok())
                    .flatten();
                let ipc = session
                    .child
                    .as_ref()
                    .and_then(Child::id)
                    .map(|pid| (session.pipe_name.clone(), session.token.to_string(), pid));
                (exit, session.changing, ipc)
            };
            if let Some(exit) = exit {
                if !changing {
                    controls::close(&self.app);
                    if exit.success() {
                        self.stage("disconnected", None).await;
                    } else {
                        self.stage(
                            "failed",
                            Some(AppError::new(
                                ErrorCode::NetworkFailed,
                                "远程串流已中断，请检查网络后重连。",
                            )),
                        )
                        .await;
                    }
                    return Ok(());
                }
            }
            {
                let session = self.session.lock().await;
                if !changing && session.child.is_none() && session.snapshot.stage == "failed" {
                    return Ok(());
                }
            }
            if !changing {
                if let Some((pipe, token, pid)) = ipc {
                    let status = self.moonlight.ipc(&pipe, &token, "status", json!({}), pid);
                    let response = tokio::select! {
                        _ = cancel.cancelled() => return Ok(()),
                        response = status => response,
                    };
                    if let Ok(response) = response {
                        let mut session = self.session.lock().await;
                        session.snapshot.metrics = MoonlightAdapter::metrics(&response);
                        self.emit_session(&session.snapshot);
                    }
                }
            }
        }
    }
    async fn resolve_address(
        &self,
        request: &ConnectRequest,
        device: Option<&Device>,
        network: Option<&NetConnection>,
        cancel: &CancellationToken,
    ) -> Result<HostInfo> {
        let expected = device.and_then(|d| d.uuid.as_deref());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let mut reported = network
            .and_then(|connection| connection.remote_address.clone())
            .into_iter()
            .collect::<Vec<_>>();
        let mut local_addresses = network
            .and_then(|connection| connection.local_address.clone())
            .into_iter()
            .collect::<Vec<_>>();
        if let Some(address) = self.nat.cached_account().await.local_vpn_address {
            local_addresses.push(address);
        }
        let code_connection = request.peer_code.is_some() || device.is_some_and(|device| device.peer_code.is_some());
        let saved_address = device.and_then(|device| device.address.as_deref());
        let explicit = if code_connection {
            Some(
                request
                    .address
                    .as_deref()
                    .or(saved_address)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        AppError::new(
                            ErrorCode::AddressRequired,
                            "识别码连接需要填写对方的组网地址。",
                        )
                    })?,
            )
        } else {
            request.address.as_deref()
        };
        self.check_role_conflict(
            explicit
                .or_else(|| reported.first().map(String::as_str))
                .or(saved_address),
        )
        .await?;
        let explicit_addresses = host_candidates(explicit, &[], None, &[], &local_addresses)?;
        let mut discovered = vec![];
        let mut last = None;
        let mut identity_error = None;
        let mut https_error = None;
        let mut found_address = false;
        let mut route_attempts = 0;
        loop {
            if cancel.is_cancelled() {
                return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
            }
            let account_connection = network.filter(|_| !code_connection);
            if let Some(id) = account_connection.and_then(|connection| connection.peer_id.as_deref()) {
                let lookup_deadline =
                    (tokio::time::Instant::now() + Duration::from_secs(3)).min(deadline);
                if let Ok(Ok(host)) =
                    tokio::time::timeout_at(lookup_deadline, self.nat.host_info(id, cancel)).await
                {
                    if let Some(address) = host.address {
                        reported.retain(|previous| previous != &address);
                        reported.insert(0, address);
                    }
                }
            }
            if let (Some(expected_connection), Some(current_connection)) =
                (account_connection, self.nat.cached_connection().await)
            {
                let same_peer = expected_connection
                    .peer_id
                    .as_deref()
                    .zip(current_connection.peer_id.as_deref())
                    .is_some_and(|(expected, current)| expected.eq_ignore_ascii_case(current));
                let same_code = expected_connection
                    .code
                    .as_deref()
                    .zip(current_connection.code.as_deref())
                    .is_some_and(|(expected, current)| expected == current);
                if same_peer || (expected_connection.peer_id.is_none() && same_code) {
                    if let Some(address) = current_connection.remote_address {
                        reported.retain(|previous| previous != &address);
                        reported.insert(0, address);
                    }
                    if let Some(address) = current_connection.local_address {
                        if !local_addresses.contains(&address) {
                            local_addresses.push(address);
                        }
                    }
                }
            }
            self.check_role_conflict(
                explicit
                    .or_else(|| reported.first().map(String::as_str))
                    .or(saved_address),
            )
            .await?;
            let mut addresses = if code_connection {
                explicit_addresses.clone()
            } else {
                host_candidates(
                    explicit,
                    &reported,
                    saved_address,
                    &discovered,
                    &local_addresses,
                )?
            };
            if (addresses.is_empty() || last.is_some() || https_error.is_some()) && route_attempts < 2 {
                if let Some(local_address) = account_connection.and_then(|n| n.local_address.as_deref()) {
                    route_attempts += 1;
                    if let Ok(peers) = tokio::time::timeout_at(
                        deadline,
                        self.sunshine.peer_gateways(local_address, cancel),
                    )
                    .await
                    {
                        discovered = peers;
                        addresses = host_candidates(
                            request.address.as_deref(),
                            &reported,
                            device.and_then(|d| d.address.as_deref()),
                            &discovered,
                            &local_addresses,
                        )?;
                    }
                }
            }
            found_address |= !addresses.is_empty();
            for address in addresses {
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
                match tokio::time::timeout_at(deadline, probe_host(&address, expected, cancel))
                    .await
                {
                    Ok(Ok(info)) => {
                        match tokio::time::timeout_at(deadline, info.check_https_port(cancel)).await {
                            Ok(Ok(())) => return Ok(info),
                            Ok(Err(error)) if error.code == ErrorCode::Canceled => {
                                return Err(error)
                            }
                            Ok(Err(error)) => https_error = Some(error),
                            Err(_) => break,
                        }
                    }
                    Ok(Err(error)) if error.code == ErrorCode::Canceled => return Err(error),
                    Ok(Err(error)) if error.code == ErrorCode::PermissionDenied => {
                        identity_error = Some(error);
                    }
                    Ok(Err(error)) => last = Some(error),
                    Err(_) => break,
                }
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::select! {
                _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Canceled, "连接已取消。")),
                _ = tokio::time::sleep_until((tokio::time::Instant::now() + Duration::from_millis(400)).min(deadline)) => {}
            }
        }
        if let Some(error) = https_error.or(identity_error) {
            return Err(error);
        }
        if !found_address {
            Err(AppError::new(
                ErrorCode::AddressRequired,
                "组网已建立，但当前内核未返回对方地址。请填写对方 Sunshine 的组网地址后重试。",
            ))
        } else {
            Err(last.unwrap_or_else(|| {
                AppError::new(ErrorCode::SunshineUnavailable, "对方 Sunshine 不可达。")
            }))
        }
    }
    async fn await_stream(&self, cancel: &CancellationToken) -> Result<()> {
        for _ in 0..100 {
            let (ready, failed, ipc) = {
                let mut session = self.session.lock().await;
                let failed = session
                    .child
                    .as_mut()
                    .and_then(|child| child.try_wait().ok())
                    .flatten()
                    .is_some();
                let ipc = session
                    .child
                    .as_ref()
                    .and_then(Child::id)
                    .map(|pid| (session.pipe_name.clone(), session.token.to_string(), pid));
                (session.verified.load(Ordering::SeqCst), failed, ipc)
            };
            if ready {
                return Ok(());
            }
            if failed {
                return Err(AppError::new(
                    ErrorCode::StreamFailed,
                    "Moonlight 在建立视频会话前退出，请检查主机采集与配对。",
                ));
            }
            if let Some((pipe, token, pid)) = ipc {
                let status = self.moonlight.ipc(&pipe, &token, "status", json!({}), pid);
                let response = tokio::select! {
                    _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Canceled, "连接已取消。")),
                    response = status => response,
                };
                if let Ok(response) = response {
                    if response.get("connected") == Some(&serde_json::Value::Bool(true)) {
                        return Ok(());
                    }
                }
            }
            tokio::select! { _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Canceled, "连接已取消。")), _ = tokio::time::sleep(Duration::from_millis(200)) => {} }
        }
        Err(AppError::new(
            ErrorCode::StreamFailed,
            "未取得 Moonlight 的会话就绪信号。请使用包含 LL 会话扩展的完整内核。",
        ))
    }
    fn record_failure(
        &self,
        request: &ConnectRequest,
        error: &AppError,
        connected_device_id: Option<&str>,
        direct: bool,
    ) {
        let device_id = connected_device_id.or(request.device_id.as_deref());
        if let Ok(mut store) = self.store() {
            if error.reauthenticate && !direct {
                if let Some(device) = store
                    .account_data
                    .devices
                    .iter_mut()
                    .find(|d| device_id == Some(&d.id))
                {
                    device.credential_state = "invalid".into();
                }
            }
            store.account_data.history.push(HistoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                device_id: device_id.map(str::to_owned),
                peer_code: request.peer_code.clone(),
                connected_at: now(),
                method: if direct {
                    "IP 直连"
                } else if request.peer_code.is_some() {
                    "识别码"
                } else {
                    "同账号 / 已有连接"
                }
                .into(),
                result: if error.code == ErrorCode::Canceled {
                    "canceled"
                } else {
                    "failed"
                }
                .into(),
                error_code: Some(error.code.clone()),
            });
            let _ = store.save_devices();
        }
    }
    pub async fn kill_stream(&self) {
        let _stopping = self.stream_stop.lock().await;
        let info = {
            let mut session = self.session.lock().await;
            session.child.take().map(|child| {
                (
                    child,
                    session.pipe_name.clone(),
                    Zeroizing::new(session.token.to_string()),
                )
            })
        };
        if let Some((mut child, pipe, token)) = info {
            if let Some(pid) = child.id() {
                if self
                    .moonlight
                    .ipc(&pipe, &token, "disconnect", json!({}), pid)
                    .await
                    .is_ok()
                    && matches!(
                        tokio::time::timeout(Duration::from_secs(2), child.wait()).await,
                        Ok(Ok(_))
                    )
                {
                    return;
                }
            }
            if matches!(
                tokio::time::timeout(Duration::from_secs(5), child.kill()).await,
                Ok(Ok(()))
            ) || matches!(child.try_wait(), Ok(Some(_)))
            {
                return;
            }
            // A failed stop must keep the owned process handle observable.
            // Otherwise the role-restoration check could mistake it for an exit.
            self.session.lock().await.child = Some(child);
        }
    }
    pub async fn cancel(self: &Arc<Self>) -> Result<SessionSnapshot> {
        let id = self.session.lock().await.snapshot.id.clone();
        if let Some(id) = id {
            return self.end_session(&id, "canceled").await;
        }
        Ok(self.session.lock().await.snapshot.clone())
    }
    pub async fn disconnect(self: &Arc<Self>, id: &str) -> Result<SessionSnapshot> {
        self.end_session(id, "disconnected").await
    }
    fn record_first_connection(store: &mut Store) {
        if !store.preferences.first_connection_complete {
            store.preferences.first_connection_complete = true;
            if let Err(error) = store.save_preferences() {
                store.warning = Some(error.message);
            }
        }
    }
    pub async fn monitor_incoming_control(self: Arc<Self>) {
        let mut previous = IncomingControl::default();
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if self.exiting.load(Ordering::SeqCst) {
                continue;
            }
            let control = self.sunshine.poll_incoming_control(&self.vault).await;
            if let Ok(mut audio) = self.host_audio.lock() {
                if !self.exiting.load(Ordering::SeqCst) {
                    if let Err(error) = audio.update(control.count > 0) {
                        tracing::debug!(code = ?error.code, "Could not update host audio mute");
                    }
                }
            }
            if control == previous {
                continue;
            }
            let started = control.count > 0
                && (control.count > previous.count || control.started_at != previous.started_at);
            if started {
                if let Ok(mut store) = self.store() {
                    Self::record_first_connection(&mut store);
                }
            }
            let _ = self.app.emit_to("main", "ll:incoming-control", &control);
            if started {
                crate::show_main_window(&self.app);
            }
            previous = control;
        }
    }
    pub async fn disconnect_incoming_control(&self) -> Result<()> {
        let _guard = self.administrative_lock.lock().await;
        if self.sunshine.incoming_control().await.count == 0 {
            return Ok(());
        }
        self.sunshine.disconnect_incoming_control(&self.vault).await
    }
    pub async fn disconnect_from_tray(self: &Arc<Self>) -> Result<()> {
        let active = {
            let session = self.session.lock().await;
            (!["idle", "disconnected", "canceled", "failed"].contains(&session.snapshot.stage.as_str()))
                .then(|| session.snapshot.id.clone())
                .flatten()
        };
        if let Some(id) = active {
            return self.disconnect(&id).await.map(|_| ());
        }
        if self.sunshine.incoming_control().await.count > 0 {
            return self.disconnect_incoming_control().await;
        }
        let _permit = self
            .gate
            .try_acquire()
            .map_err(|_| AppError::new(ErrorCode::Busy, "连接正在准备，请稍后重试断开。"))?;
        let _guard = self.administrative_lock.lock().await;
        self.nat.disconnect_current().await?;
        let snapshot = self.snapshot().await?;
        let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
        Ok(())
    }
    async fn end_session(self: &Arc<Self>, id: &str, final_stage: &str) -> Result<SessionSnapshot> {
        {
            let mut session = self.session.lock().await;
            if session.snapshot.id.as_deref() != Some(id) {
                return Err(AppError::new(ErrorCode::SessionEnded, "此会话已经结束。"));
            }
            if session.snapshot.stage == "disconnecting" {
                return Ok(session.snapshot.clone());
            }
            session.cancel.cancel();
            session.snapshot.stage = "disconnecting".into();
            session.snapshot.pin = None;
            session.snapshot.error = None;
            self.emit_session(&session.snapshot);
        }
        self.kill_stream().await;
        controls::close(&self.app);
        let _permit = match tokio::time::timeout(Duration::from_secs(20), self.gate.acquire()).await
        {
            Ok(Ok(permit)) => permit,
            _ => {
                let error = AppError::new(ErrorCode::Timeout, "连接清理仍在进行，请稍后重试。");
                self.stage("disconnectFailed", Some(error.clone())).await;
                return Err(error);
            }
        };
        let _session_update = self.session_update.lock().await;
        let _guard = self.administrative_lock.lock().await;
        if self.session.lock().await.child.is_some() {
            let error = AppError::new(
                ErrorCode::Timeout,
                "Moonlight 尚未确认退出，本机接受控制将继续保持暂停。请重试断开。",
            );
            self.stage("disconnectFailed", Some(error.clone())).await;
            return Err(error);
        }
        let (device_id, direct) = {
            let session = self.session.lock().await;
            if session.snapshot.id.as_deref() != Some(id) {
                return Err(AppError::new(ErrorCode::SessionEnded, "此会话已经结束。"));
            }
            (session.snapshot.device_id.clone(), session.direct)
        };
        if !direct {
            let device = device_id.and_then(|id| self.store().ok()?.device(&id).ok());
            if let Err(error) = self.nat.end_session_connection(device.as_ref()).await {
                self.stage("disconnectFailed", Some(error.clone())).await;
                return Err(error);
            }
        }
        self.stage(final_stage, None).await;
        self.schedule_role_restore().await;
        if let Ok(snapshot) = self.snapshot().await {
            let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
        }
        Ok(self.session.lock().await.snapshot.clone())
    }
    pub async fn disconnect_device(self: &Arc<Self>, device_id: &str) -> Result<AppSnapshot> {
        let device = self.store()?.device(device_id)?;
        let session_id = {
            let session = self.session.lock().await;
            (session.snapshot.device_id.as_deref() == Some(device_id)
                && !["disconnected", "canceled"].contains(&session.snapshot.stage.as_str()))
            .then(|| session.snapshot.id.clone())
            .flatten()
        };
        if let Some(session_id) = session_id {
            self.disconnect(&session_id).await?;
        } else if device.source != "direct" {
            let _permit = self.gate.try_acquire().map_err(|_| {
                AppError::new(ErrorCode::Busy, "另一台设备正在连接，请先结束该连接。")
            })?;
            let _guard = self.administrative_lock.lock().await;
            self.nat.disconnect_device(&device).await?;
        }
        self.refresh().await
    }
    pub async fn update_session(&self, id: &str, patch: SessionPatch) -> Result<SessionSnapshot> {
        let _session_update = self.session_update.lock().await;
        let (preferences, address, app, pipe, token, pid, device_id, cancel) =
            {
                let mut session = self.session.lock().await;
                if session.snapshot.id.as_deref() != Some(id)
                    || session.snapshot.stage != "streaming"
                    || session.cancel.is_cancelled()
                {
                    return Err(AppError::new(
                        ErrorCode::SessionEnded,
                        "当前没有可调整的远控会话。",
                    ));
                }
                if session.changing {
                    return Err(AppError::new(ErrorCode::Busy, "上一次会话设置正在应用。"));
                }
                let current =
                    session.snapshot.preferences.as_ref().ok_or_else(|| {
                        AppError::new(ErrorCode::SessionEnded, "会话偏好不可用。")
                    })?;
                let next = current.patched(&patch)?;
                let pid =
                    session.child.as_ref().and_then(Child::id).ok_or_else(|| {
                        AppError::new(ErrorCode::SessionEnded, "Moonlight 已退出。")
                    })?;
                session.changing = true;
                (
                    next,
                    session.address.clone().unwrap(),
                    session.snapshot.app.clone().unwrap(),
                    session.pipe_name.clone(),
                    Zeroizing::new(session.token.to_string()),
                    pid,
                    session.snapshot.device_id.clone(),
                    session.cancel.clone(),
                )
            };
        let mut restart = match &patch {
            SessionPatch::Resolution { .. }
            | SessionPatch::Bitrate { .. }
            | SessionPatch::Fps { .. } => true,
            SessionPatch::Audio {
                device, host_audio, ..
            } => {
                let session = self.session.lock().await;
                session
                    .snapshot
                    .preferences
                    .as_ref()
                    .is_some_and(|current| {
                        current.audio_device != *device || current.host_audio != *host_audio
                    })
            }
            _ => false,
        };
        let result: Result<()> = async {
            if !restart {
                let applied = self
                    .moonlight
                    .ipc(
                        &pipe,
                        &token,
                        "apply",
                        json!({ "patch": patch, "preferences": preferences }),
                        pid,
                    )
                    .await;
                if matches!(&patch, SessionPatch::Window { .. })
                    && applied
                        .as_ref()
                        .is_err_and(|error| error.code != ErrorCode::PermissionDenied)
                {
                    restart = true;
                } else {
                    applied?;
                }
            }
            if restart {
                self.stage("reconnecting", None).await;
                self.kill_stream().await;
                if self.session.lock().await.child.is_some() {
                    return Err(AppError::new(
                        ErrorCode::Timeout,
                        "上一次 Moonlight 串流尚未退出，已停止重连。",
                    ));
                }
                let expected = device_id
                    .as_deref()
                    .and_then(|id| self.store().ok().and_then(|store| store.device(id).ok()))
                    .and_then(|d| d.uuid);
                let host = probe_host(&address, expected.as_deref(), &cancel).await?;
                {
                    let mut session = self.session.lock().await;
                    if cancel.is_cancelled() {
                        return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
                    }
                    let process = self.moonlight.spawn_stream(
                        &address,
                        Some(&host.uuid),
                        &app,
                        &preferences,
                        &pipe,
                        &token,
                        &self.store()?.preferences.shortcuts,
                    )?;
                    self.watch_stream_shortcuts(&session, &process.child, process.shortcut_events);
                    session.child = Some(process.child);
                    session.verified = process.verified;
                }
                self.await_stream(&cancel).await?;
            }
            if let Some(device_id) = device_id {
                let mut store = self.store()?;
                if let Some(device) = store
                    .account_data
                    .devices
                    .iter_mut()
                    .find(|device| device.id == device_id)
                {
                    device.preferences = preferences.clone();
                    store.save_devices()?;
                }
            }
            self.session.lock().await.snapshot.preferences = Some(preferences);
            Ok(())
        }
        .await;
        {
            let mut session = self.session.lock().await;
            session.changing = false;
            if session.cancel.is_cancelled() {
                return Err(AppError::new(ErrorCode::SessionEnded, "此会话已经结束。"));
            }
        }
        if let Err(error) = result {
            if restart {
                self.stage("failed", Some(error.clone())).await;
                self.kill_stream().await;
                controls::close(&self.app);
            }
            return Err(error);
        }
        self.stage("streaming", None).await;
        if restart {
            if let Some(owner) = self.app.get_webview_window("main") {
                if let Err(error) = controls::show_remote_window(self, &owner).await {
                    tracing::warn!(code = ?error.code, "Could not activate the reconnected stream window");
                }
            }
        }
        Ok(self.session.lock().await.snapshot.clone())
    }
    pub async fn diagnostics(&self, device_id: Option<&str>) -> Result<Vec<Diagnostic>> {
        let snapshot = self.refresh().await?;
        let driver_error = self
            .engines
            .nat_driver_error(None)
            .await
            .and_then(|report| report.application_error());
        let mut rows = vec![
            Diagnostic {
                name: "皎月连账号".into(),
                status: if snapshot.account.authenticated {
                    "ok"
                } else {
                    "error"
                }
                .into(),
                detail: if snapshot.account.authenticated {
                    "已从内核读取登录状态"
                } else {
                    "请登录皎月连或检查本地接口访问密码"
                }
                .into(),
            },
            Diagnostic {
                name: "可用连接时长".into(),
                status: if snapshot.account.quota_minutes.is_some_and(|m| m > 0) {
                    "ok"
                } else {
                    "unknown"
                }
                .into(),
                detail: if snapshot.account.quota_minutes.is_none() {
                    "接口未返回余额，未知不等于零"
                } else {
                    "显示内核返回的余额"
                }
                .into(),
            },
            Diagnostic {
                name: "本机 Sunshine 管理账号".into(),
                status: if snapshot.preferences.setup_complete {
                    "ok"
                } else {
                    "error"
                }
                .into(),
                detail: if snapshot.preferences.setup_complete {
                    if snapshot.sunshine.credential_valid {
                        "账号已设置，LL 管理凭据已验证"
                    } else {
                        "账号已设置；在 LL 内确认 PIN 或保存采集设置前可验证管理账号"
                    }
                } else {
                    "首次使用需要完成账号创建或验证"
                }
                .into(),
            },
        ];
        rows.push(Diagnostic {
            name: "Wintun 虚拟网卡".into(),
            status: if driver_error.is_some() {
                "error"
            } else if snapshot.account.server_running {
                "ok"
            } else {
                "unknown"
            }
            .into(),
            detail: driver_error.map(|error| error.message).unwrap_or_else(|| {
                if snapshot.account.server_running {
                    "皎月连组网服务已启动"
                } else {
                    "开启接受控制时记录 Wintun 接口初始化结果"
                }
                .into()
            }),
        });
        if let Some(id) = device_id {
            let device = self.store()?.device(id)?;
            let cancel = CancellationToken::new();
            let host = if let Some(address) = &device.address {
                probe_host(address, device.uuid.as_deref(), &cancel).await.ok()
            } else { None };
            let reachable = host.is_some();
            rows.push(Diagnostic {
                name: "对方 Sunshine".into(),
                status: if reachable { "ok" } else { "unknown" }.into(),
                detail: if reachable {
                    "地址可达，已核对主机身份"
                } else {
                    "尚未取得或验证对方地址；连接时会准备网络"
                }
                .into(),
            });
            if let Some(host) = host {
                let (status, detail) = match host.check_https_port(&cancel).await {
                    Ok(()) => ("ok", format!("TCP {} 可连接", host.https_port)),
                    Err(error) => ("error", error.message),
                };
                rows.push(Diagnostic {
                    name: "对方 Sunshine 配对端口".into(),
                    status: status.into(),
                    detail,
                });
            }
            rows.push(Diagnostic {
                name: "连接凭据".into(),
                status: if device
                    .credential_ref
                    .as_ref()
                    .is_some_and(|reference| self.vault.get(reference).is_ok())
                {
                    "ok"
                } else {
                    "unknown"
                }
                .into(),
                detail: "普通设备记录只保存凭据引用，真实密码由 DPAPI 保护".into(),
            });
        }
        rows.push(Diagnostic {
            name: "Moonlight 会话扩展".into(),
            status: if self.engines.paths.manifest.moonlight.extension_protocol == 1 {
                "ok"
            } else {
                "error"
            }
            .into(),
            detail: "设置直达、动态输入与真实指标由随包扩展提供".into(),
        });
        Ok(rows)
    }
    pub fn pause_automatic_acceptance(&self, pause: bool) {
        self.pause_automatic_acceptance.store(pause, Ordering::SeqCst);
    }

    async fn restore_jiaoyue(&self, cancel: &CancellationToken) -> Result<Account> {
        self.engines.ensure_nat().await?;
        let account = match self.nat.snapshot().await {
            Ok((account, _)) => account,
            Err(error) if error.code == ErrorCode::LocalAuthorizationRequired => {
                let secret = self
                    .vault
                    .get(&Vault::reference("local", "jiaoyue-local-access"))
                    .or_else(|_| {
                        self.vault
                            .get(&Vault::reference("local", "jiaoyue-web-password"))
                    })
                    .map_err(|_| error)?;
                self.nat.authorize(&secret.password).await?;
                self.nat.snapshot().await?.0
            }
            Err(error) => return Err(error),
        };
        if account.authenticated {
            return Ok(account);
        }
        let secret = match self.nat.restart_login_secret().await {
            Some(secret) => Some(secret),
            None if self.store()?.preferences.remember_login => {
                let reference = Vault::reference("local", "jiaoyue-login");
                self.vault.contains(&reference).then(|| self.vault.get(&reference)).transpose()?
            }
            None => None,
        };
        match secret {
            Some(secret) => {
                self.nat
                    .login(&secret.username, &secret.password, cancel)
                    .await
            }
            None => Ok(account),
        }
    }

    async fn restore_background_services(self: &Arc<Self>, retry_login: bool) -> Result<()> {
        let direct = self.direct_acceptance.lock().await.is_some();
        if direct && !self.sunshine.running().await {
            self.stop_direct_acceptance().await?;
        }
        let cancel = CancellationToken::new();
        let account = if retry_login {
            self.restore_jiaoyue(&cancel).await?
        } else {
            self.nat.cached_account().await
        };
        let (desired, active) = {
            let store = self.store()?;
            (
                store.preferences.always_accept_control,
                store.preferences.accept_control,
            )
        };
        if !desired || direct
            || self.pause_automatic_acceptance.load(Ordering::SeqCst)
            || self.session.lock().await.role_resume.is_some()
        {
            return Ok(());
        }
        if !account.authenticated {
            return Err(AppError::new(
                ErrorCode::LoginRequired,
                "请登录皎月连并勾选在本机记住登录，以便下次启动时接受控制。",
            ));
        }
        if active && account.server_running && self.sunshine.running().await {
            return Ok(());
        }
        self.require_sunshine_account().await?;
        let passwords = self.local_server_passwords().await;
        self.start_accepting(
            &passwords.connection_password,
            &passwords.web_password,
            SystemTime::now(),
        ).await?;
        self.store()?.preferences.accept_control = true;
        self.schedule_acceptance_firewall();
        Ok(())
    }

    pub async fn prepare_background(self: &Arc<Self>) {
        {
            let _guard = self.administrative_lock.lock().await;
            if self.direct_acceptance.lock().await.is_none() {
                if let Err(error) = self.sunshine.restore_direct_firewall().await {
                    tracing::warn!(code = ?error.code, "Could not remove leftover direct access firewall rules");
                    if let Ok(mut store) = self.store() {
                        if store.warning.is_none() {
                            store.warning = Some("上次的直连防火墙规则尚未清理，请在 IP 直连页检查。".into());
                        }
                    }
                }
            }
        }
        let mut next_recovery = tokio::time::Instant::now();
        let mut manual_login_required = false;
        let mut device_account = None;
        let mut refresh = tokio::time::interval(Duration::from_secs(4));
        refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            refresh.tick().await;
            if self.exiting.load(Ordering::SeqCst) {
                continue;
            }
            let account = self.nat.cached_account().await;
            if account.authenticated {
                manual_login_required = false;
            }
            if tokio::time::Instant::now() >= next_recovery && self.gate.available_permits() > 0 {
                if let Ok(guard) = self.administrative_lock.try_lock() {
                    let result = self.restore_background_services(!manual_login_required).await;
                    let warning = match result {
                        Ok(()) => None,
                        Err(error) => {
                            manual_login_required |= matches!(error.code, ErrorCode::LoginFailed | ErrorCode::AccountInUse);
                            tracing::warn!(code = ?error.code, "Background login or acceptance could not be restored");
                            Some(error.message)
                        }
                    };
                    if let Ok(mut value) = self.startup_network_warning.lock() {
                        if !manual_login_required || warning.is_some() && value.is_none() {
                            *value = warning;
                        }
                    }
                    next_recovery = tokio::time::Instant::now() + if self.nat.cached_account().await.authenticated {
                        Duration::from_secs(8)
                    } else {
                        Duration::from_secs(20)
                    };
                    drop(guard);
                    let account = self.nat.cached_account().await;
                    if account.authenticated && device_account != account.username {
                        if self.refresh().await.is_ok() {
                            device_account = account.username;
                        }
                    } else if !account.authenticated {
                        device_account = None;
                    }
                }
            }
            if let Ok(snapshot) = self.snapshot().await {
                let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
            }
        }
    }
    async fn show_quit_error(&self, error: AppError) {
        self.exiting.store(false, Ordering::SeqCst);
        tracing::warn!(code = ?error.code, "Application shutdown could not be confirmed");
        if let Ok(mut store) = self.store() {
            store.warning = Some(format!("退出尚未完成。{}", error.message));
        }
        if let Ok(snapshot) = self.refresh().await {
            let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
        }
        crate::show_main_window(&self.app);
    }
    pub async fn quit(&self, stop_accepting: bool) {
        if self.exiting.swap(true, Ordering::SeqCst) {
            return;
        }
        self.session.lock().await.cancel.cancel();
        self.cancel_role_restore().await;
        self.kill_stream().await;
        controls::close(&self.app);
        if self.session.lock().await.snapshot.id.is_some() {
            self.stage("disconnected", None).await;
        }
        let permit = match tokio::time::timeout(Duration::from_secs(20), self.gate.acquire()).await
        {
            Ok(Ok(permit)) => permit,
            _ => {
                self.show_quit_error(AppError::new(ErrorCode::Timeout, "连接清理仍在进行。"))
                    .await;
                return;
            }
        };
        let session_update = self.session_update.lock().await;
        let guard = self.administrative_lock.lock().await;
        let accept_control = self
            .store()
            .is_ok_and(|store| store.preferences.accept_control);
        let (sunshine, network) = tokio::join!(
            self.sunshine.stop_on_exit(stop_accepting || accept_control),
            async {
                if self.engines.owns_nat().await {
                    self.engines.release_owned_nat().await?;
                } else {
                    self.nat.cancel_owned_connection().await;
                    if (accept_control || stop_accepting)
                        && self.nat.cached_account().await.server_running
                    {
                        self.nat.set_server(false, "", "").await?;
                    }
                }
                Ok(())
            }
        );
        let firewall = self.sunshine.restore_vpn_firewall().await;
        let direct_firewall = self.sunshine.restore_direct_firewall().await;
        let result = sunshine.and(network).and(firewall).and(direct_firewall).and_then(|()| {
            if stop_accepting {
                let mut store = self.store()?;
                store.preferences.accept_control = false;
                store.preferences.always_accept_control = false;
                store.save_preferences()?;
            }
            Ok(())
        });
        drop(guard);
        drop(session_update);
        drop(permit);
        if let Err(error) = result {
            self.show_quit_error(error).await;
            return;
        }
        if let Ok(mut power) = self.power.lock() {
            let _ = power.apply(false, false);
        }
        if let Ok(mut audio) = self.host_audio.lock() {
            if let Err(error) = audio.update(false) {
                tracing::warn!(code = ?error.code, "Could not restore host audio on exit");
            }
        }
        self.app.exit(0);
    }
}

#[cfg(test)]
mod host_candidate_tests {
    use super::*;

    #[test]
    fn explicit_port_is_preserved_before_reported_default_endpoint() {
        let addresses = host_candidates(
            Some("10.8.0.1:48089"),
            &["10.8.0.1".into(), "10.8.0.1:47989".into()],
            Some("10.8.0.3"),
            &["10.8.0.1".into()],
            &[],
        )
        .unwrap();
        assert_eq!(addresses, ["10.8.0.1:48089", "10.8.0.1", "10.8.0.3"]);
    }

    #[test]
    fn discovery_cannot_select_the_controllers_client_or_server_address() {
        let own = ["10.8.0.2".into(), "10.9.0.1".into()];
        let addresses = host_candidates(
            None,
            &["10.8.0.2".into()],
            Some("10.9.0.1"),
            &["10.8.0.1".into(), "10.8.0.2".into()],
            &own,
        )
        .unwrap();
        assert_eq!(addresses, ["10.8.0.1"]);
        assert_eq!(
            host_candidates(Some("10.8.0.2"), &[], None, &[], &own)
                .unwrap_err()
                .code,
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn invalid_metadata_is_skipped_but_invalid_user_input_is_reported() {
        let addresses = host_candidates(
            None,
            &["fixture|mapping".into(), "user:fixture@host".into()],
            None,
            &["10.8.0.1".into()],
            &[],
        )
        .unwrap();
        assert_eq!(addresses, ["10.8.0.1"]);
        assert_eq!(
            host_candidates(Some("user:fixture@host"), &[], None, &[], &[])
                .unwrap_err()
                .code,
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn equivalent_default_dns_endpoints_are_not_probed_repeatedly() {
        assert_eq!(
            host_candidates(Some("MY-PC:47989"), &["my-pc".into()], None, &[], &[]).unwrap(),
            ["my-pc"]
        );
    }
}
