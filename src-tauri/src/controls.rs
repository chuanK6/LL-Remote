use crate::{
    error::{AppError, ErrorCode, Result},
    runtime::Runtime,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const BALL_DIAMETER: f64 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

#[derive(Clone, Copy)]
struct Area {
    bounds: Rect,
    work: Rect,
    scale: f64,
}

#[derive(Clone, Copy)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    fn visible_half(self, mut ball: Rect) -> Rect {
        match self {
            Self::Left => ball.width = (ball.width + 1) / 2,
            Self::Right => {
                ball.left += ball.width / 2;
                ball.width = (ball.width + 1) / 2;
            }
            Self::Top => ball.height = (ball.height + 1) / 2,
            Self::Bottom => {
                ball.top += ball.height / 2;
                ball.height = (ball.height + 1) / 2;
            }
        }
        ball
    }
}

#[derive(Clone, Copy)]
struct Anchor {
    edge: Edge,
    offset: f64,
}

impl Anchor {
    fn position(self, bounds: Rect, size: i32) -> (i32, i32) {
        let x = bounds.width - size;
        let y = bounds.height - size;
        let along = |span| (self.offset * f64::from(span)).round() as i32;
        let (left, top) = match self.edge {
            Edge::Left => (0, along(y)),
            Edge::Right => (x, along(y)),
            Edge::Top => (along(x), 0),
            Edge::Bottom => (along(x), y),
        };
        (bounds.left + left, bounds.top + top)
    }

    fn nearest(bounds: Rect, ball: Rect) -> Self {
        let x = (ball.left - bounds.left).clamp(0, bounds.width - ball.width);
        let y = (ball.top - bounds.top).clamp(0, bounds.height - ball.height);
        let room_x = bounds.width - ball.width;
        let room_y = bounds.height - ball.height;
        let (edge, _) = [
            (Edge::Left, x),
            (Edge::Right, room_x - x),
            (Edge::Top, y),
            (Edge::Bottom, room_y - y),
        ]
        .into_iter()
        .min_by_key(|&(_, distance)| distance)
        .expect("four edges");
        let offset = match edge {
            Edge::Left | Edge::Right => f64::from(y) / f64::from(room_y.max(1)),
            Edge::Top | Edge::Bottom => f64::from(x) / f64::from(room_x.max(1)),
        };
        Self { edge, offset }
    }
}

pub struct Overlay {
    anchor: Anchor,
    expanded: bool,
    drag_position: Option<(i32, i32)>,
    rect: Option<Rect>,
    visible: bool,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            anchor: Anchor {
                edge: Edge::Right,
                offset: 0.25,
            },
            expanded: false,
            drag_position: None,
            rect: None,
            visible: false,
        }
    }
}

#[cfg(windows)]
fn stream_window(pid: u32) -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };
    struct Search {
        pid: u32,
        window: HWND,
    }
    unsafe extern "system" fn enumerate(window: HWND, data: LPARAM) -> i32 {
        let search = &mut *(data as *mut Search);
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        if pid != search.pid || IsWindowVisible(window) == 0 {
            return 1;
        }
        let mut class = [0; 32];
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        if length > 0
            && class[..length as usize]
                .iter()
                .copied()
                .eq("SDL_app".encode_utf16())
        {
            search.window = window;
            return 0;
        }
        1
    }
    unsafe {
        let mut search = Search {
            pid,
            window: std::ptr::null_mut(),
        };
        EnumWindows(Some(enumerate), &mut search as *mut _ as isize);
        (!search.window.is_null()).then_some(search.window)
    }
}

#[cfg(windows)]
fn stream_area(pid: u32, overlay: &WebviewWindow, foreground_only: bool) -> Option<Area> {
    use windows_sys::Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::Gdi::{
            ClientToScreen, GetMonitorInfoW, MonitorFromWindow, MONITORINFO,
            MONITOR_DEFAULTTONEAREST,
        },
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{GetClientRect, GetForegroundWindow, IsIconic},
        },
    };
    let window = stream_window(pid)?;
    unsafe {
        if IsIconic(window) != 0 {
            return None;
        }
        let foreground = GetForegroundWindow();
        if foreground_only
            && foreground != window
            && foreground != overlay.hwnd().ok()?.0 as HWND
        {
            return None;
        }
        let mut client: RECT = std::mem::zeroed();
        let mut origin = POINT { x: 0, y: 0 };
        if GetClientRect(window, &mut client) == 0 || ClientToScreen(window, &mut origin) == 0
        {
            return None;
        }
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let left = origin.x.max(info.rcMonitor.left);
        let top = origin.y.max(info.rcMonitor.top);
        let width = (origin.x + client.right).min(info.rcMonitor.right) - left;
        let height = (origin.y + client.bottom).min(info.rcMonitor.bottom) - top;
        (width > 0 && height > 0).then_some(Area {
            bounds: Rect {
                left,
                top,
                width,
                height,
            },
            work: Rect {
                left: info.rcWork.left,
                top: info.rcWork.top,
                width: info.rcWork.right - info.rcWork.left,
                height: info.rcWork.bottom - info.rcWork.top,
            },
            scale: f64::from(GetDpiForWindow(window).max(96)) / 96.0,
        })
    }
}

#[cfg(not(windows))]
fn stream_area(_: u32, _: &WebviewWindow, _: bool) -> Option<Area> {
    None
}

pub async fn show_remote_window(runtime: &Runtime, owner: &WebviewWindow) -> Result<()> {
    let pid = {
        let session = runtime.session.lock().await;
        if session.snapshot.stage != "streaming" {
            return Err(AppError::new(ErrorCode::SessionEnded, "当前没有活动的远控会话。"));
        }
        session.child.as_ref().and_then(tokio::process::Child::id)
    };
    let (send, receive) = tokio::sync::oneshot::channel();
    owner
        .run_on_main_thread(move || {
            #[cfg(windows)]
            let focused = pid
                .and_then(stream_window)
                .is_some_and(crate::platform::bring_window_forward);
            #[cfg(not(windows))]
            let focused = { let _ = pid; false };
            let _ = send.send(focused);
        })
        .map_err(window_error)?;
    if receive.await == Ok(true) {
        Ok(())
    } else {
        Err(AppError::new(ErrorCode::SystemFailed, "Windows 未能恢复远控窗口。"))
    }
}

#[cfg(windows)]
fn cursor_position() -> Option<(i32, i32)> {
    use windows_sys::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0).then_some((point.x, point.y))
}

#[cfg(not(windows))]
fn cursor_position() -> Option<(i32, i32)> {
    None
}

fn window_error(_: tauri::Error) -> AppError {
    AppError::new(ErrorCode::SystemFailed, "无法调整会话悬浮窗口。")
}

fn overlay_window(runtime: &Runtime) -> Result<WebviewWindow> {
    runtime
        .app
        .get_webview_window("session-controls")
        .ok_or_else(|| AppError::new(ErrorCode::SessionEnded, "悬浮窗口已关闭。"))
}

fn ball_size(area: Area) -> i32 {
    ((BALL_DIAMETER * area.scale).round() as i32)
        .min(area.bounds.width)
        .min(area.bounds.height)
}

fn ball_rect(overlay: &Overlay, area: Area) -> Rect {
    let size = ball_size(area);
    let bounds = area.bounds;
    let (left, top) = match overlay.drag_position {
        Some((x, y)) => (
            (x - size / 2).clamp(bounds.left, bounds.left + bounds.width - size),
            (y - size / 2).clamp(bounds.top, bounds.top + bounds.height - size),
        ),
        None => overlay.anchor.position(bounds, size),
    };
    Rect {
        left,
        top,
        width: size,
        height: size,
    }
}

fn position(window: &WebviewWindow, overlay: &mut Overlay, area: Option<Area>) -> Result<()> {
    let Some(area) = area else {
        if overlay.visible {
            window.hide().map_err(window_error)?;
            overlay.visible = false;
        }
        return Ok(());
    };
    let ball = ball_rect(overlay, area);
    let bounds = area.bounds;
    let rect = if overlay.expanded {
        let width = ((300.0 * area.scale).round() as i32).min(bounds.width);
        let height = ((560.0 * area.scale).round() as i32).min(bounds.height);
        Rect {
            left: (ball.left + ball.width / 2 - width / 2)
                .clamp(bounds.left, bounds.left + bounds.width - width),
            top: (ball.top + ball.height / 2 - height / 2)
                .clamp(bounds.top, bounds.top + bounds.height - height),
            width,
            height,
        }
    } else if overlay.drag_position.is_some() {
        ball
    } else {
        overlay.anchor.edge.visible_half(ball)
    };
    if !overlay.visible {
        window.show().map_err(window_error)?;
    }
    if !overlay.visible || overlay.rect != Some(rect) {
        set_bounds(window, rect)?;
        overlay.rect = Some(rect);
    }
    overlay.visible = true;
    Ok(())
}

#[cfg(windows)]
fn set_bounds(window: &WebviewWindow, rect: Rect) -> Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowLongW, SetWindowPos, GWL_STYLE, HWND_TOPMOST, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOSENDCHANGING, WS_CAPTION, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
    };
    let owner = window.clone();
    window
        .run_on_main_thread(move || {
            let Ok(hwnd) = owner.hwnd() else { return };
            unsafe {
                let hwnd = hwnd.0 as _;
                let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
                let popup = (style & !(WS_CAPTION | WS_THICKFRAME | WS_SYSMENU)) | WS_POPUP;
                let frame = if style != popup {
                    SetWindowLongW(hwnd, GWL_STYLE, popup as i32);
                    SWP_FRAMECHANGED
                } else {
                    0
                };
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    rect.left,
                    rect.top,
                    rect.width,
                    rect.height,
                    frame | SWP_NOACTIVATE | SWP_NOSENDCHANGING,
                );
            }
        })
        .map_err(window_error)
}

#[cfg(not(windows))]
fn set_bounds(window: &WebviewWindow, rect: Rect) -> Result<()> {
    window
        .set_position(tauri::PhysicalPosition::new(rect.left, rect.top))
        .map_err(window_error)?;
    window
        .set_size(tauri::PhysicalSize::new(
            rect.width as u32,
            rect.height as u32,
        ))
        .map_err(window_error)
}

pub async fn show(runtime: &Arc<Runtime>) -> Result<()> {
    let mut overlay = runtime.overlay.lock().await;
    if runtime.app.get_webview_window("session-controls").is_some() {
        return Ok(());
    }
    let (id, cancel) = {
        let session = runtime.session.lock().await;
        (session.snapshot.id.clone(), session.cancel.clone())
    };
    if cancel.is_cancelled() {
        return Err(AppError::new(ErrorCode::SessionEnded, "此会话已经结束。"));
    }
    let window = WebviewWindowBuilder::new(
        &runtime.app,
        "session-controls",
        WebviewUrl::App("index.html".into()),
    )
    .title("LL远程 · 本次连接")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .focused(false)
    .visible(false)
    .inner_size(BALL_DIAMETER, BALL_DIAMETER)
    .min_inner_size(1.0, 1.0)
    .build()
    .map_err(window_error)?;
    *overlay = Overlay::default();
    let runtime = runtime.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(80));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tick.tick() => {}
            }
            let mut overlay = runtime.overlay.lock().await;
            let pid = {
                let session = runtime.session.lock().await;
                if session.snapshot.id != id
                    || !["streaming", "reconnecting"].contains(&session.snapshot.stage.as_str())
                {
                    break;
                }
                session.child.as_ref().and_then(tokio::process::Child::id)
            };
            let area = pid.and_then(|pid| stream_area(pid, &window, true));
            if position(&window, &mut overlay, area).is_err() {
                break;
            }
        }
    });
    Ok(())
}

pub async fn activate(runtime: &Runtime) -> Result<()> {
    let mut overlay = runtime.overlay.lock().await;
    let window = overlay_window(runtime)?;
    let pid = runtime
        .session
        .lock()
        .await
        .child
        .as_ref()
        .and_then(tokio::process::Child::id);
    let area = pid
        .and_then(|pid| stream_area(pid, &window, false))
        .ok_or_else(|| AppError::new(ErrorCode::SessionEnded, "请先恢复远控窗口。"))?;
    position(&window, &mut overlay, Some(area))?;
    window.set_focus().map_err(window_error)
}

pub fn close(app: &AppHandle) {
    for label in ["session-controls", "session-performance"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.destroy();
        }
    }
}

pub async fn show_performance(runtime: &Arc<Runtime>) -> Result<()> {
    if runtime.app.get_webview_window("session-performance").is_some() {
        return Ok(());
    }
    let (id, cancel) = {
        let session = runtime.session.lock().await;
        (session.snapshot.id.clone(), session.cancel.clone())
    };
    if cancel.is_cancelled() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(
        &runtime.app,
        "session-performance",
        WebviewUrl::App("index.html".into()),
    )
    .title("LL远程 · 性能监测")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .focusable(false)
    .visible(false)
    .inner_size(192.0, 145.0)
    .min_inner_size(1.0, 1.0)
    .build()
    .map_err(window_error)?;
    if let Err(error) = window.set_ignore_cursor_events(true) {
        let _ = window.destroy();
        return Err(window_error(error));
    }
    let runtime = runtime.clone();
    tokio::spawn(async move {
        let mut current = None;
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tick.tick() => {}
            }
            let pid = {
                let session = runtime.session.lock().await;
                if session.snapshot.id != id
                    || !["streaming", "reconnecting"].contains(&session.snapshot.stage.as_str())
                {
                    break;
                }
                session.child.as_ref().and_then(tokio::process::Child::id)
            };
            let enabled = runtime
                .store()
                .is_ok_and(|store| store.preferences.show_performance_monitor);
            if !enabled {
                if current.is_some() && window.hide().is_ok() {
                    current = None;
                }
                continue;
            }
            let Some(area) = pid.and_then(|pid| stream_area(pid, &window, false)) else {
                continue;
            };
            let margin = (16.0 * area.scale).round() as i32;
            let width = ((192.0 * area.scale).round() as i32).min(area.work.width);
            let height = ((145.0 * area.scale).round() as i32).min(area.work.height);
            let rect = Rect {
                left: area.work.left + (area.work.width - width - margin).max(0),
                top: area.work.top + (area.work.height - height - margin).max(0),
                width,
                height,
            };
            if current != Some(rect) {
                if set_bounds(&window, rect).is_err() || (current.is_none() && window.show().is_err()) {
                    let _ = window.destroy();
                    break;
                }
                current = Some(rect);
            }
        }
    });
    Ok(())
}

pub async fn layout(runtime: &Runtime, expanded: bool) -> Result<()> {
    let mut overlay = runtime.overlay.lock().await;
    let window = overlay_window(runtime)?;
    let stream = {
        let session = runtime.session.lock().await;
        session
            .child
            .as_ref()
            .and_then(tokio::process::Child::id)
            .map(|pid| (session.pipe_name.clone(), session.token.clone(), pid))
    };
    let area = stream
        .as_ref()
        .and_then(|(_, _, pid)| stream_area(*pid, &window, true));
    if expanded {
        let (pipe, token, pid) =
            stream.ok_or_else(|| AppError::new(ErrorCode::SessionEnded, "Moonlight 已退出。"))?;
        if area.is_none() {
            return Err(AppError::new(
                ErrorCode::SessionEnded,
                "远控窗口当前不可用。",
            ));
        }
        runtime
            .moonlight
            .ipc(&pipe, &token, "input", json!({ "suspended": true }), pid)
            .await?;
        window.set_focus().map_err(window_error)?;
    }
    overlay.expanded = expanded;
    overlay.drag_position = None;
    position(&window, &mut overlay, area)
}

pub async fn drag(runtime: &Runtime, finished: bool) -> Result<&'static str> {
    let mut overlay = runtime.overlay.lock().await;
    if overlay.expanded {
        return Ok(overlay.anchor.edge.name());
    }
    let window = overlay_window(runtime)?;
    let pid = runtime
        .session
        .lock()
        .await
        .child
        .as_ref()
        .and_then(tokio::process::Child::id);
    let area = pid.and_then(|pid| stream_area(pid, &window, true));
    overlay.drag_position = cursor_position();
    if finished {
        if let Some(area) = area {
            overlay.anchor = Anchor::nearest(area.bounds, ball_rect(&overlay, area));
        }
        overlay.drag_position = None;
    }
    position(&window, &mut overlay, area)?;
    Ok(overlay.anchor.edge.name())
}
