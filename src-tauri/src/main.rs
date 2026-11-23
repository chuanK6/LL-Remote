#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if let Some(code) = ll_remote::run_sunshine_service_helper() {
        std::process::exit(code);
    }
    std::env::remove_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER");
    #[cfg(windows)]
    if !ensure_system_webview2() {
        return;
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(root) = executable.parent() {
            let portable = root.join("portable.flag").is_file()
                || std::env::args().any(|arg| arg == "--portable");
            let data = if cfg!(debug_assertions) {
                std::env::var_os("LLREMOTE_TEST_DATA").map(std::path::PathBuf::from)
            } else {
                None
            }
            .or_else(|| portable.then(|| root.join("Data")))
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .map(|path| std::path::PathBuf::from(path).join("LLRemote"))
            });
            if let Some(data) = data {
                std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", data.join("WebView2"));
            }
        }
    }
    ll_remote::run();
}

#[cfg(windows)]
fn ensure_system_webview2() -> bool {
    if tauri::webview_version().is_ok_and(|version| !version.trim().is_empty()) {
        return true;
    }
    use windows_sys::Win32::UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::{MessageBoxW, IDOK, MB_ICONWARNING, MB_OKCANCEL, SW_SHOWNORMAL},
    };
    let title: Vec<u16> = "LL远程 · 安装 WebView2"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let message: Vec<u16> = "LL远程需要系统 Microsoft Edge WebView2 Runtime。\n\n点击“确定”打开微软官方安装页面，完成安装后重新启动 LL远程。\n点击“取消”退出。".encode_utf16().chain(Some(0)).collect();
    let answer = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OKCANCEL | MB_ICONWARNING,
        )
    };
    if answer == IDOK {
        let action: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let url: Vec<u16> =
            "https://developer.microsoft.com/microsoft-edge/webview2/#download-section"
                .encode_utf16()
                .chain(Some(0))
                .collect();
        unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                action.as_ptr(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    }
    false
}
