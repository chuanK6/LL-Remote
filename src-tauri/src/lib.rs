mod commands;
mod controls;
mod engines;
mod error;
mod host_audio;
mod model;
#[cfg(test)]
mod native_tests;
mod platform;
mod runtime;
mod storage;
mod sunshine_service;
#[cfg(test)]
mod tests;

pub(crate) fn show_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        let owner = window.clone();
        let _ = window.run_on_main_thread(move || {
            if !owner.is_visible().unwrap_or(false) {
                let _ = owner.show();
            }
            if owner.is_minimized().unwrap_or(false) {
                let _ = owner.unminimize();
            }
            let _ = owner.set_focus();
            #[cfg(windows)]
            if let Ok(hwnd) = owner.hwnd() {
                platform::activate_window(hwnd.0 as _);
            }
        });
    }
}

fn fit_main_window(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    use tauri::{LogicalSize, PhysicalPosition, PhysicalSize};

    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return Ok(());
    };
    let area = monitor.work_area();
    let inner = window.inner_size()?;
    let outer = window.outer_size()?;
    let frame_width = outer.width.saturating_sub(inner.width);
    let frame_height = outer.height.saturating_sub(inner.height);
    let scale = monitor.scale_factor();
    let margin = LogicalSize::new(24.0, 24.0).to_physical::<u32>(scale);
    let available = PhysicalSize::new(
        area.size.width.saturating_sub(frame_width + margin.width),
        area.size.height.saturating_sub(frame_height + margin.height),
    );
    let size = PhysicalSize::new(
        inner.width.min(available.width.max(1)),
        inner.height.min(available.height.max(1)),
    );
    let minimum = LogicalSize::new(620.0, 380.0).to_physical::<u32>(scale);
    let minimum = PhysicalSize::new(minimum.width.min(size.width), minimum.height.min(size.height));
    window.set_min_size(Some(minimum.to_logical::<f64>(scale)))?;
    window.set_size(size)?;
    window.set_position(PhysicalPosition::new(
        area.position.x + (area.size.width.saturating_sub(size.width + frame_width) / 2) as i32,
        area.position.y + (area.size.height.saturating_sub(size.height + frame_height) / 2) as i32,
    ))
}

pub fn run_sunshine_service_helper() -> Option<i32> {
    sunshine_service::run_helper()
}

pub fn run() {
    use runtime::Runtime;
    use std::sync::Arc;
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
        Manager,
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !args.iter().any(|arg| arg == "--background") {
                show_main_window(app);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::refresh_state,
            commands::login_jiaoyue,
            commands::authorize_jiaoyue,
            commands::logout_jiaoyue,
            commands::setup_sunshine,
            commands::control_sunshine,
            commands::set_accept_control,
            commands::get_local_server_passwords,
            commands::confirm_sunshine_pin,
            commands::save_sunshine_settings,
            commands::save_preferences,
            commands::edit_device,
            commands::delete_device,
            commands::wallpaper_path,
            commands::preview_wallpaper,
            commands::save_stream_preferences,
            commands::save_quick_apps,
            commands::associate_host,
            commands::connect_device,
            commands::cancel_connection,
            commands::disconnect_device,
            commands::get_session,
            commands::get_session_performance,
            commands::show_remote_window,
            commands::get_incoming_control,
            commands::disconnect_incoming_control,
            commands::disconnect_session,
            commands::update_session,
            commands::show_session_controls,
            commands::layout_session_controls,
            commands::set_session_performance_enabled,
            commands::drag_session_controls,
            commands::run_diagnostics,
            commands::refresh_device_apps,
            commands::open_advanced,
            commands::open_signin,
            commands::quit_app
        ])
        .setup(|app| {
            let runtime = Runtime::create(app.handle().clone())?;
            let log_directory = runtime.store()?.root.join("logs");
            let file = tracing_appender::rolling::daily(log_directory, "llremote.log");
            let (writer, guard) = tracing_appender::non_blocking(file);
            app.manage(guard);
            let _ = tracing_subscriber::fmt()
                .with_writer(writer)
                .with_max_level(tracing::Level::INFO)
                .with_ansi(false)
                .try_init();
            app.manage(runtime.clone());
            let open = MenuItem::with_id(app, "open", "显示主界面", true, None::<&str>)?;
            let disconnect =
                MenuItem::with_id(app, "disconnect", "断开本次连接", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "完全退出", true, None::<&str>)?;
            let stop_accepting = MenuItem::with_id(
                app,
                "stop-accepting",
                "停止本机接受控制",
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&open, &disconnect, &stop_accepting, &exit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("LL远程")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    let runtime = app.state::<Arc<Runtime>>().inner().clone();
                    match event.id.as_ref() {
                        "open" => show_main_window(app),
                        "disconnect" | "stop-accepting" => {
                            let stop = event.id.as_ref() == "stop-accepting";
                            tauri::async_runtime::spawn(async move {
                                let result = if stop {
                                    runtime.stop_accepting().await.map(|_| ())
                                } else {
                                    runtime.disconnect_from_tray().await
                                };
                                if let Err(error) = result {
                                    let _ = tauri::Emitter::emit_to(
                                        &runtime.app,
                                        "main",
                                        "ll:operation-error",
                                        &error,
                                    );
                                    show_main_window(&runtime.app);
                                }
                            });
                        }
                        "exit" => {
                            tauri::async_runtime::spawn(async move {
                                runtime.quit(false).await;
                            });
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = fit_main_window(&window) {
                    tracing::warn!(%error, "Could not fit the main window to the display work area");
                }
            }
            if !std::env::args().any(|arg| arg == "--background") {
                show_main_window(app.handle());
            }
            tauri::async_runtime::spawn(runtime.clone().monitor_incoming_control());
            tauri::async_runtime::spawn(async move {
                runtime.prepare_background().await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let runtime = window.state::<Arc<Runtime>>().inner().clone();
                if runtime.exiting.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                if runtime
                    .store()
                    .is_ok_and(|store| store.preferences.keep_running_in_tray_on_close)
                {
                    let _ = window.hide();
                } else {
                    tauri::async_runtime::spawn(async move {
                        runtime.quit(false).await;
                    });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("LL远程无法启动");
}
