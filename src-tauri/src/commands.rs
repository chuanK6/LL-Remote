use crate::{
    controls,
    engines::moonlight::AppList,
    error::{AppError, ErrorCode, Result},
    model::*,
    platform,
    runtime::Runtime,
    signin,
    storage::{SecretRecord, Vault},
};
use std::{sync::Arc, time::SystemTime};
use tauri::{Manager, State, WebviewWindow};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;
type Backend<'a> = State<'a, Arc<Runtime>>;

fn main_only(window: &WebviewWindow) -> Result<()> {
    if window.label() != "main" {
        Err(AppError::new(
            ErrorCode::PermissionDenied,
            "此操作仅允许在 LL远程主界面执行。",
        ))
    } else {
        Ok(())
    }
}
fn session_only(window: &WebviewWindow) -> Result<()> {
    if !["main", "session-controls"].contains(&window.label()) {
        Err(AppError::new(
            ErrorCode::PermissionDenied,
            "窗口没有本次会话的控制权限。",
        ))
    } else {
        Ok(())
    }
}
fn no_active_session(runtime: &Runtime) -> Result<()> {
    if runtime.gate.available_permits() == 0 {
        Err(AppError::new(
            ErrorCode::Busy,
            "请先断开本次远控，再修改账号或本机服务。",
        ))
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn bootstrap(window: WebviewWindow, runtime: Backend<'_>) -> Result<AppSnapshot> {
    main_only(&window)?;
    runtime.refresh().await
}
#[tauri::command]
pub async fn refresh_state(window: WebviewWindow, runtime: Backend<'_>) -> Result<AppSnapshot> {
    main_only(&window)?;
    runtime.refresh().await
}
#[tauri::command]
pub async fn login_jiaoyue(
    window: WebviewWindow,
    runtime: Backend<'_>,
    username: String,
    password: String,
    remember: bool,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    no_active_session(&runtime)?;
    let password = Zeroizing::new(password);
    if username.trim().is_empty() || password.is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "请填写皎月连账号和密码。",
        ));
    }
    runtime.revoke_role_for_account_change(&username).await;
    let _guard = runtime.administrative_lock.lock().await;
    runtime.require_sunshine_account().await?;
    runtime.engines.ensure_nat().await?;
    let account = runtime
        .nat
        .login(username.trim(), &password, &CancellationToken::new())
        .await?;
    signin::close(&window);
    let reference = Vault::reference("local", "jiaoyue-login");
    if remember {
        runtime.vault.put(
            &reference,
            &SecretRecord {
                username: username.trim().into(),
                password: password.to_string(),
            },
        )?;
    } else {
        runtime.vault.delete(&reference)?;
    }
    {
        let mut store = runtime.store()?;
        store.switch_account(account.username.as_deref())?;
        store.preferences.remember_login = remember;
        store.save_preferences()?;
    }
    runtime.refresh().await
}
#[tauri::command]
pub async fn authorize_jiaoyue(
    window: WebviewWindow,
    runtime: Backend<'_>,
    password: String,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    let _guard = runtime.administrative_lock.lock().await;
    let password = Zeroizing::new(password);
    runtime.nat.authorize(&password).await?;
    if runtime.store()?.preferences.remember_login {
        runtime.vault.put(
            &Vault::reference("local", "jiaoyue-local-access"),
            &SecretRecord {
                username: String::new(),
                password: password.to_string(),
            },
        )?;
    }
    runtime.refresh().await
}
#[tauri::command]
pub async fn logout_jiaoyue(window: WebviewWindow, runtime: Backend<'_>) -> Result<AppSnapshot> {
    main_only(&window)?;
    no_active_session(&runtime)?;
    runtime.cancel_role_restore().await;
    let _guard = runtime.administrative_lock.lock().await;
    runtime.nat.logout().await?;
    signin::close(&window);
    runtime
        .vault
        .delete(&Vault::reference("local", "jiaoyue-login"))?;
    runtime.vault.delete(&Vault::reference("local", "jiaoyue-local-access"))?;
    runtime.store()?.switch_account(None)?;
    runtime.refresh().await
}
#[tauri::command]
pub async fn setup_sunshine(
    window: WebviewWindow,
    runtime: Backend<'_>,
    username: String,
    password: String,
    confirm_password: String,
    mode: String,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    no_active_session(&runtime)?;
    let _guard = runtime.administrative_lock.lock().await;
    let password = Zeroizing::new(password);
    let confirm = Zeroizing::new(confirm_password);
    runtime
        .sunshine
        .setup(&username, &password, &confirm, &mode, &runtime.vault)
        .await?;
    {
        let mut store = runtime.store()?;
        store.preferences.setup_complete = true;
        store.preferences.sunshine_username = Some(username.trim().into());
        store.save_preferences()?;
    }
    if runtime.store()?.preferences.accept_control {
        runtime.sunshine.start().await?;
    }
    runtime.snapshot().await
}
#[tauri::command]
pub async fn control_sunshine(
    window: WebviewWindow,
    runtime: Backend<'_>,
    action: String,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    no_active_session(&runtime)?;
    let _guard = runtime.administrative_lock.lock().await;
    match action.as_str() {
        "start" => {
            runtime.sunshine.start().await?;
        }
        "stop" => {
            runtime.stop_direct_acceptance().await?;
            runtime.sunshine.stop().await?;
        }
        "restart" => {
            runtime.sunshine.stop().await?;
            runtime.sunshine.start().await?;
        }
        _ => return Err(AppError::new(ErrorCode::InvalidInput, "服务操作不受支持。")),
    }
    runtime.refresh().await
}
#[tauri::command]
pub async fn get_local_server_passwords(
    window: WebviewWindow,
    runtime: Backend<'_>,
) -> Result<LocalServerPasswords> {
    main_only(&window)?;
    Ok(runtime.local_server_passwords().await)
}

#[tauri::command]
pub async fn set_accept_control(
    window: WebviewWindow,
    runtime: Backend<'_>,
    enabled: bool,
    password: String,
    web_password: Option<String>,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    if !enabled {
        return runtime.stop_accepting().await;
    }
    let _guard = runtime.administrative_lock.lock().await;
    let started = SystemTime::now();
    runtime.require_no_direct_acceptance().await?;
    let password = Zeroizing::new(password);
    runtime.require_no_role_pause().await?;
    let web_password = Zeroizing::new(web_password.unwrap_or_default());
    runtime.require_sunshine_account().await?;
    if let Err(error) = runtime.engines.ensure_nat().await {
        return Err(runtime.network_error(error, started).await);
    }
    {
        let configured = runtime.local_server_passwords().await;
        let connection_password = Zeroizing::new(if !password.is_empty() {
            password.to_string()
        } else if !configured.connection_password.is_empty() {
            configured.connection_password.clone()
        } else {
            runtime
                .vault
                .get(&Vault::reference("local", "jiaoyue-server-password"))
                .map(|secret| secret.password.clone())
                .unwrap_or_default()
        });
        let access_password = Zeroizing::new(if !web_password.is_empty() {
            web_password.to_string()
        } else if !configured.web_password.is_empty() {
            configured.web_password.clone()
        } else {
            runtime
                .vault
                .get(&Vault::reference("local", "jiaoyue-web-password"))
                .map(|secret| secret.password.clone())
                .unwrap_or_else(|_| connection_password.to_string())
        });
        if connection_password.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "首次开启接受控制需要设置本机连接密码。",
            ));
        }
        if !(6..=20).contains(&access_password.len()) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "组网页面访问密码需要 6–20 位。",
            ));
        }
        for (identity, value) in [
            ("jiaoyue-server-password", connection_password.as_str()),
            ("jiaoyue-web-password", access_password.as_str()),
        ] {
            runtime.vault.put(
                &Vault::reference("local", identity),
                &SecretRecord {
                    username: String::new(),
                    password: value.into(),
                },
            )?;
        }
        runtime
            .nat
            .remember_server_passwords(&connection_password, &access_password)
            .await;
        runtime
            .start_accepting(&connection_password, &access_password, started)
            .await?;
    }
    runtime.store()?.preferences.accept_control = true;
    runtime.pause_automatic_acceptance(false);
    runtime.inner().schedule_acceptance_firewall();
    runtime.snapshot().await
}
#[tauri::command]
pub async fn confirm_sunshine_pin(
    window: WebviewWindow,
    runtime: Backend<'_>,
    pin: String,
    name: String,
    pairing_id: String,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    let pin = Zeroizing::new(pin);
    if name.trim().is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "请输入配对设备名称。",
        ));
    }
    runtime
        .sunshine
        .confirm_pin(&pin, &name, &pairing_id, &runtime.vault)
        .await?;
    runtime.refresh().await
}
#[tauri::command]
pub async fn save_sunshine_settings(
    window: WebviewWindow,
    runtime: Backend<'_>,
    settings: SunshineSettings,
    restart: bool,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    let _guard = runtime.administrative_lock.lock().await;
    runtime
        .sunshine
        .save_settings(&settings, &runtime.vault)
        .await?;
    if restart {
        runtime.sunshine.stop().await?;
        runtime.sunshine.start().await?;
    }
    runtime.refresh().await
}
#[tauri::command]
pub async fn save_shortcuts(
    window: WebviewWindow,
    runtime: Backend<'_>,
    shortcuts: crate::shortcuts::Shortcuts,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    runtime.save_shortcuts(shortcuts).await
}

#[tauri::command]
pub async fn trigger_shortcut(
    window: WebviewWindow,
    runtime: Backend<'_>,
    action: crate::shortcuts::ShortcutAction,
) -> Result<()> {
    main_only(&window)?;
    runtime.inner().trigger_shortcut(action).await
}

#[tauri::command]
pub async fn save_preferences(
    window: WebviewWindow,
    runtime: Backend<'_>,
    preferences: GeneralPreferencesPatch,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    let _guard = runtime.administrative_lock.lock().await;
    let remember_secret = if preferences.remember_login == Some(true) {
        runtime.nat.restart_login_secret().await
    } else {
        None
    };
    let resume_acceptance = preferences.always_accept_control == Some(true);
    {
        let mut store = runtime.store()?;
        let mut desired = store.preferences.clone();
        preferences.apply(&mut desired);
        let previous = &store.preferences;
        let power_changed = previous.prevent_sleep != desired.prevent_sleep
            || previous.keep_display_awake != desired.keep_display_awake;
        let autostart_changed = previous.autostart != desired.autostart;
        let mut power = runtime
            .power
            .lock()
            .map_err(|_| AppError::new(ErrorCode::SystemFailed, "电源管理不可用。"))?;
        if power_changed {
            power.apply(desired.prevent_sleep, desired.keep_display_awake)?;
        }
        let restore_power = |power: &mut platform::PowerManager, previous: &Preferences| {
            if power_changed {
                let _ = power.apply(previous.prevent_sleep, previous.keep_display_awake);
            }
        };
        if autostart_changed {
            if let Err(error) = platform::set_autostart(desired.autostart, &runtime.executable) {
                restore_power(&mut power, previous);
                return Err(error);
            }
        }
        {
            let previous = std::mem::replace(&mut store.preferences, desired);
            let saved = store.save_preferences().and_then(|()| {
                let reference = Vault::reference("local", "jiaoyue-login");
                if previous.remember_login && !store.preferences.remember_login {
                    runtime.vault.delete(&reference)?;
                } else if let Some(secret) = &remember_secret {
                    runtime.vault.put(&reference, secret)?;
                }
                Ok(())
            });
            if let Err(error) = saved {
                if autostart_changed {
                    let _ = platform::set_autostart(previous.autostart, &runtime.executable);
                }
                restore_power(&mut power, &previous);
                store.preferences = previous;
                let _ = store.save_preferences();
                return Err(error);
            }
            if autostart_changed {
                if let Ok(mut warning) = runtime.autostart_warning.lock() {
                    *warning = None;
                }
            }
        }
    }
    if resume_acceptance {
        runtime.pause_automatic_acceptance(false);
    }
    runtime.snapshot().await
}
#[tauri::command]
pub async fn edit_device(
    window: WebviewWindow,
    runtime: Backend<'_>,
    edit: DeviceEdit,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    runtime.store()?.edit_device(&edit)?;
    runtime.snapshot().await
}
#[tauri::command]
pub async fn delete_device(
    window: WebviewWindow,
    runtime: Backend<'_>,
    device_id: String,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    let _connection = runtime
        .gate
        .try_acquire()
        .map_err(|_| AppError::new(ErrorCode::Busy, "请先断开当前远控，再删除设备。"))?;
    let _guard = runtime.administrative_lock.lock().await;
    let device = runtime.store()?.device(&device_id)?;
    if runtime.nat.cached_connection().await.is_some_and(|connection| connection.matches_device(&device)) {
        runtime.nat.disconnect_device(&device).await?;
    }
    runtime.store()?.remove_device(&device_id)?;
    runtime.snapshot().await
}
#[tauri::command]
pub async fn delete_history_entry(
    window: WebviewWindow,
    runtime: Backend<'_>,
    record_id: String,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    let _guard = runtime.administrative_lock.lock().await;
    runtime.store()?.remove_history_entry(&record_id)?;
    runtime.snapshot().await
}
#[tauri::command]
pub async fn wallpaper_path(
    window: WebviewWindow,
    runtime: Backend<'_>,
    relative: String,
) -> Result<String> {
    main_only(&window)?;
    let path = runtime.store()?.wallpaper_path(&relative)?;
    // The data root can be portable or a debug override. Grant only this
    // validated, canonical file instead of relying on config-time glob paths.
    window
        .asset_protocol_scope()
        .allow_file(&path)
        .map_err(|_| {
            AppError::new(
                ErrorCode::PermissionDenied,
                "无法授权读取此壁纸，请重新选择图片。",
            )
        })?;
    Ok(path.to_string_lossy().into())
}
#[tauri::command]
pub fn preview_wallpaper(window: WebviewWindow, path: String) -> Result<String> {
    use base64::Engine;
    main_only(&window)?;
    let (bytes, format) = crate::storage::read_wallpaper(std::path::Path::new(&path))?;
    Ok(format!(
        "data:image/{};base64,{}",
        if format == "jpg" { "jpeg" } else { format },
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
#[tauri::command]
pub async fn save_stream_preferences(
    window: WebviewWindow,
    runtime: Backend<'_>,
    device_id: Option<String>,
    preferences: StreamPreferences,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    let _guard = runtime.administrative_lock.lock().await;
    preferences.validate()?;
    runtime.write_moonlight_preferences(device_id.as_deref(), &preferences)?;
    {
        let mut store = runtime.store()?;
        if let Some(id) = device_id {
            store.device(&id)?;
            store
                .account_data
                .devices
                .iter_mut()
                .find(|device| device.id == id)
                .unwrap()
                .preferences = preferences;
            store.save_devices()?;
        } else {
            store.preferences.stream = preferences;
            store.save_preferences()?;
        }
    }
    runtime.snapshot().await
}
#[tauri::command]
pub async fn save_quick_apps(
    window: WebviewWindow,
    runtime: Backend<'_>,
    device_id: String,
    apps: Vec<String>,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    {
        let mut store = runtime.store()?;
        let device = store.device(&device_id)?;
        if apps.len() > 100
            || apps
                .iter()
                .any(|app| !device.apps.iter().any(|published| &published.name == app))
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "快速启动只能选择已发布的远程应用。",
            ));
        }
        store
            .account_data
            .devices
            .iter_mut()
            .find(|d| d.id == device_id)
            .unwrap()
            .quick_apps = apps;
        store.save_devices()?;
    }
    runtime.snapshot().await
}
#[tauri::command]
pub async fn associate_host(
    window: WebviewWindow,
    runtime: Backend<'_>,
    device_id: String,
    nat_id: String,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    {
        let mut store = runtime.store()?;
        let mut device = store.device(&device_id)?;
        if device.source == "direct" {
            return Err(AppError::new(ErrorCode::InvalidInput, "IP 直连记录使用独立地址。"));
        }
        if !store
            .account_data
            .devices
            .iter()
            .any(|d| d.nat_id.as_deref() == Some(&nat_id))
        {
            return Err(AppError::new(
                ErrorCode::Offline,
                "关联主机不在当前账号目录中。",
            ));
        }
        let target = store
            .account_data
            .devices
            .iter()
            .find(|device| device.nat_id.as_deref() == Some(&nat_id))
            .unwrap();
        if device
            .uuid
            .as_ref()
            .zip(target.uuid.as_ref())
            .is_some_and(|(selected, target)| !selected.eq_ignore_ascii_case(target))
        {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "两台设备的 Sunshine 身份不同，请选择对应的同账号主机。",
            ));
        }
        device.nat_id = Some(nat_id);
        store.merge_device(device);
        store.save_devices()?;
    }
    runtime.snapshot().await
}
#[tauri::command]
pub async fn connect_device(
    window: WebviewWindow,
    runtime: Backend<'_>,
    request: ConnectRequest,
) -> Result<SessionSnapshot> {
    main_only(&window)?;
    runtime.inner().connect(request).await
}
#[tauri::command]
pub async fn connect_direct(
    window: WebviewWindow,
    runtime: Backend<'_>,
    device_id: Option<String>,
    address: Option<String>,
    app: Option<String>,
) -> Result<SessionSnapshot> {
    main_only(&window)?;
    runtime.inner().connect_direct(device_id, address, app).await
}

#[tauri::command]
pub async fn direct_addresses(window: WebviewWindow, runtime: Backend<'_>) -> Result<Vec<String>> {
    main_only(&window)?;
    runtime.sunshine.direct_addresses().await
}

#[tauri::command]
pub async fn set_direct_acceptance(
    window: WebviewWindow,
    runtime: Backend<'_>,
    access: Option<DirectAccess>,
    address: Option<String>,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    runtime.set_direct_acceptance(access, address).await
}
#[tauri::command]
pub async fn resolve_role_switch(
    window: WebviewWindow,
    runtime: Backend<'_>,
    session_id: String,
    enabled: bool,
) -> Result<()> {
    main_only(&window)?;
    runtime.resolve_role_switch(&session_id, enabled).await
}
#[tauri::command]
pub async fn cancel_connection(
    window: WebviewWindow,
    runtime: Backend<'_>,
) -> Result<SessionSnapshot> {
    main_only(&window)?;
    runtime.cancel().await
}
#[tauri::command]
pub async fn disconnect_device(
    window: WebviewWindow,
    runtime: Backend<'_>,
    device_id: String,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    runtime.disconnect_device(&device_id).await
}
#[tauri::command]
pub async fn get_session(window: WebviewWindow, runtime: Backend<'_>) -> Result<SessionSnapshot> {
    session_only(&window)?;
    Ok(runtime.session.lock().await.snapshot.clone())
}
#[tauri::command]
pub async fn get_session_performance(
    window: WebviewWindow,
    runtime: Backend<'_>,
) -> Result<Metrics> {
    if window.label() != "session-performance" {
        return Err(AppError::new(
            ErrorCode::PermissionDenied,
            "仅性能监测窗口可读取此状态。",
        ));
    }
    Ok(runtime.session.lock().await.snapshot.metrics.clone())
}
#[tauri::command]
pub async fn show_remote_window(window: WebviewWindow, runtime: Backend<'_>) -> Result<()> {
    main_only(&window)?;
    controls::show_remote_window(&runtime, &window).await
}
#[tauri::command]
pub async fn get_incoming_control(
    window: WebviewWindow,
    runtime: Backend<'_>,
) -> Result<IncomingControl> {
    main_only(&window)?;
    Ok(runtime.sunshine.incoming_control().await)
}
#[tauri::command]
pub async fn disconnect_incoming_control(
    window: WebviewWindow,
    runtime: Backend<'_>,
) -> Result<()> {
    main_only(&window)?;
    runtime.disconnect_incoming_control().await
}
#[tauri::command]
pub async fn disconnect_session(
    window: WebviewWindow,
    runtime: Backend<'_>,
    session_id: String,
) -> Result<SessionSnapshot> {
    session_only(&window)?;
    runtime.disconnect(&session_id).await
}
#[tauri::command]
pub async fn update_session(
    window: WebviewWindow,
    runtime: Backend<'_>,
    session_id: String,
    patch: SessionPatch,
) -> Result<SessionSnapshot> {
    session_only(&window)?;
    runtime.update_session(&session_id, patch).await
}
#[tauri::command]
pub async fn show_session_controls(window: WebviewWindow, runtime: Backend<'_>) -> Result<()> {
    main_only(&window)?;
    if runtime.session.lock().await.snapshot.stage != "streaming" {
        return Err(AppError::new(
            ErrorCode::SessionEnded,
            "当前没有活动的远控会话。",
        ));
    }
    controls::show(runtime.inner()).await?;
    controls::activate(&runtime).await
}
#[tauri::command]
pub async fn layout_session_controls(
    window: WebviewWindow,
    runtime: Backend<'_>,
    expanded: bool,
) -> Result<bool> {
    if window.label() != "session-controls" {
        return Err(AppError::new(
            ErrorCode::PermissionDenied,
            "仅悬浮窗口能修改自身布局。",
        ));
    }
    controls::layout(&runtime, expanded).await?;
    Ok(runtime.store()?.preferences.show_performance_monitor)
}
#[tauri::command]
pub async fn set_session_performance_enabled(
    window: WebviewWindow,
    runtime: Backend<'_>,
    session_id: String,
    enabled: bool,
) -> Result<()> {
    if window.label() != "session-controls" {
        return Err(AppError::new(
            ErrorCode::PermissionDenied,
            "仅悬浮球面板可更改性能监测开关。",
        ));
    }
    let previous = {
        let session = runtime.session.lock().await;
        if session.snapshot.id.as_deref() != Some(session_id.as_str())
            || !matches!(
                session.snapshot.stage.as_str(),
                "streaming" | "reconnecting"
            )
            || session.cancel.is_cancelled()
        {
            return Err(AppError::new(
                ErrorCode::SessionEnded,
                "当前没有活动的远控会话。",
            ));
        }
        let mut store = runtime.store()?;
        let previous = store.preferences.show_performance_monitor;
        if enabled != previous {
            store.preferences.show_performance_monitor = enabled;
            if let Err(error) = store.save_preferences() {
                store.preferences.show_performance_monitor = previous;
                return Err(error);
            }
        }
        previous
    };
    if enabled {
        if let Err(error) = controls::show_performance(runtime.inner()).await {
            let mut store = runtime.store()?;
            store.preferences.show_performance_monitor = previous;
            store.save_preferences()?;
            return Err(error);
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn drag_session_controls(
    window: WebviewWindow,
    runtime: Backend<'_>,
    finished: bool,
) -> Result<&'static str> {
    if window.label() != "session-controls" {
        return Err(AppError::new(
            ErrorCode::PermissionDenied,
            "仅悬浮窗口能移动自身。",
        ));
    }
    controls::drag(&runtime, finished).await
}
#[tauri::command]
pub async fn run_diagnostics(
    window: WebviewWindow,
    runtime: Backend<'_>,
    device_id: Option<String>,
) -> Result<Vec<Diagnostic>> {
    main_only(&window)?;
    runtime.diagnostics(device_id.as_deref()).await
}
#[tauri::command]
pub async fn refresh_device_apps(
    window: WebviewWindow,
    runtime: Backend<'_>,
    device_id: String,
) -> Result<AppSnapshot> {
    main_only(&window)?;
    let device = runtime.store()?.device(&device_id)?;
    let address = device.address.ok_or_else(|| {
        AppError::new(
            ErrorCode::AddressRequired,
            "尚未取得对方主机地址，请先完成连接。",
        )
    })?;
    match runtime
        .moonlight
        .list_apps(&address, device.uuid.as_deref(), &CancellationToken::new())
        .await?
    {
        AppList::Paired(apps) => {
            let mut store = runtime.store()?;
            if let Some(device) = store
                .account_data
                .devices
                .iter_mut()
                .find(|d| d.id == device_id)
            {
                device.apps = apps;
            }
            store.save_devices()?;
        }
        AppList::NeedsPairing => {
            return Err(AppError::new(
                ErrorCode::PairingRequired,
                "对方尚未授权此电脑，请先完成配对。",
            ))
        }
    }
    runtime.snapshot().await
}
#[tauri::command]
pub async fn open_advanced(
    window: WebviewWindow,
    runtime: Backend<'_>,
    app: String,
    device_id: Option<String>,
) -> Result<Option<String>> {
    main_only(&window)?;
    match app.as_str() {
        "jiaoyue" => {
            runtime.engines.ensure_nat().await?;
            platform::open_url(&format!(
                "http://127.0.0.1:{}",
                runtime.engines.paths.manifest.jiaoyuelian.port
            ))?;
            Ok(None)
        }
        "moonlight" => {
            let _guard = runtime.administrative_lock.lock().await;
            let preferences = {
                let store = runtime.store()?;
                if let Some(id) = &device_id {
                    store.device(id)?.preferences
                } else {
                    store.preferences.stream.clone()
                }
            };
            runtime.write_moonlight_preferences(device_id.as_deref(), &preferences)?;
            runtime.moonlight.open_settings()
        }
        "sunshine" => {
            runtime.sunshine.start().await?;
            platform::open_url(&runtime.sunshine.config_url()?)?;
            Ok(None)
        }
        _ => Err(AppError::new(ErrorCode::InvalidInput, "应用入口不受支持。")),
    }
}
#[tauri::command]
pub async fn open_signin(window: WebviewWindow, runtime: Backend<'_>) -> Result<()> {
    main_only(&window)?;
    let _guard = runtime.administrative_lock.lock().await;
    signin::open(&window, &runtime).await
}
#[tauri::command]
pub async fn get_signin_status(
    window: WebviewWindow,
    runtime: Backend<'_>,
) -> Result<Option<crate::signin_status::SigninStatus>> {
    main_only(&window)?;
    let account = runtime.nat.cached_account().await;
    let Some(username) = account.username.filter(|_| account.authenticated) else {
        return Ok(None);
    };
    let store = runtime.store()?;
    crate::signin_status::load(&store, username)
}
#[tauri::command]
pub async fn quit_app(
    window: WebviewWindow,
    runtime: Backend<'_>,
    stop_accepting: bool,
) -> Result<()> {
    main_only(&window)?;
    runtime.quit(stop_accepting).await;
    Ok(())
}
