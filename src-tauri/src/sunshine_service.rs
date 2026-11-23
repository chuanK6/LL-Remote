use crate::error::{AppError, ErrorCode, Result};

#[cfg(windows)]
mod windows {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        os::windows::ffi::OsStrExt,
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_CANCELLED, HANDLE, WAIT_OBJECT_0,
        },
        System::{
            Com::{
                CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            },
            Services::*,
            Threading::WaitForSingleObject,
        },
        UI::{
            Shell::{
                ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::SW_HIDE,
        },
    };

    struct ServiceHandle(SC_HANDLE);
    impl Drop for ServiceHandle {
        fn drop(&mut self) {
            unsafe {
                CloseServiceHandle(self.0);
            }
        }
    }
    struct ProcessHandle(HANDLE);
    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    #[derive(Serialize, Deserialize)]
    struct Reply {
        token: String,
        result: Result<()>,
    }

    fn system_error(code: u32) -> AppError {
        if code == ERROR_ACCESS_DENIED {
            AppError::new(
                ErrorCode::PermissionDenied,
                "Sunshine 服务需要 Windows 授权。",
            )
        } else if code == 1058 {
            AppError::new(ErrorCode::SystemFailed, "Sunshine 服务已被禁用，无法启动。")
        } else {
            AppError::new(
                ErrorCode::SystemFailed,
                format!("Windows 未能完成 Sunshine 服务操作（错误 {code}）。"),
            )
        }
    }
    fn state(handle: SC_HANDLE) -> Result<u32> {
        let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        if unsafe { QueryServiceStatus(handle, &mut status) } == 0 {
            return Err(system_error(unsafe { GetLastError() }));
        }
        Ok(status.dwCurrentState)
    }
    fn wait_state(handle: SC_HANDLE, target: u32) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if state(handle)? == target {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(AppError::new(
                    ErrorCode::Timeout,
                    "Sunshine 服务状态切换超时，请稍后在 LL 中重试。",
                ));
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    fn direct(start: bool) -> Result<()> {
        let name: Vec<u16> = "SunshineService".encode_utf16().chain(Some(0)).collect();
        let manager =
            unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
        if manager.is_null() {
            return Err(system_error(unsafe { GetLastError() }));
        }
        let manager = ServiceHandle(manager);
        let query = unsafe { OpenServiceW(manager.0, name.as_ptr(), SERVICE_QUERY_STATUS) };
        if query.is_null() {
            return Err(system_error(unsafe { GetLastError() }));
        }
        let query = ServiceHandle(query);
        let target = if start {
            SERVICE_RUNNING
        } else {
            SERVICE_STOPPED
        };
        let current = state(query.0)?;
        if current == target {
            return Ok(());
        }
        if current
            == if start {
                SERVICE_START_PENDING
            } else {
                SERVICE_STOP_PENDING
            }
        {
            return wait_state(query.0, target);
        }
        let service = unsafe {
            OpenServiceW(
                manager.0,
                name.as_ptr(),
                SERVICE_QUERY_STATUS | if start { SERVICE_START } else { SERVICE_STOP },
            )
        };
        if service.is_null() {
            return Err(system_error(unsafe { GetLastError() }));
        }
        let service = ServiceHandle(service);
        let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        let success = unsafe {
            if start {
                StartServiceW(service.0, 0, std::ptr::null())
            } else {
                ControlService(service.0, SERVICE_CONTROL_STOP, &mut status)
            }
        };
        if success == 0 {
            let code = unsafe { GetLastError() };
            if !((start && code == 1056) || (!start && code == 1062)) {
                return Err(system_error(code));
            }
        }
        wait_state(service.0, target)
    }

    fn elevated(start: bool) -> Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|_| AppError::new(ErrorCode::SystemFailed, "无法创建本机权限组件通道。"))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| AppError::new(ErrorCode::SystemFailed, "无法准备本机权限组件通道。"))?;
        let port = listener
            .local_addr()
            .map_err(|_| AppError::new(ErrorCode::SystemFailed, "无法读取本机权限组件端口。"))?
            .port();
        let token = uuid::Uuid::new_v4().simple().to_string();
        let executable = std::env::current_exe()?;
        let file: Vec<u16> = executable
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let directory: Vec<u16> = executable
            .parent()
            .unwrap()
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let verb: Vec<u16> = "runas".encode_utf16().chain(Some(0)).collect();
        let arguments: Vec<u16> = format!(
            "--sunshine-service-helper {} --response-port {port} --response-token {token}",
            if start { "start" } else { "stop" }
        )
        .encode_utf16()
        .chain(Some(0))
        .collect();
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = arguments.as_ptr();
        info.lpDirectory = directory.as_ptr();
        info.nShow = SW_HIDE;
        let initialized = unsafe {
            CoInitializeEx(
                std::ptr::null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        } >= 0;
        let launched = unsafe { ShellExecuteExW(&mut info) };
        let error = unsafe { GetLastError() };
        if initialized {
            unsafe {
                CoUninitialize();
            }
        }
        if launched == 0 {
            return Err(if error == ERROR_CANCELLED {
                AppError::new(
                    ErrorCode::Canceled,
                    "已取消 Windows 授权，可以稍后在 LL 中重试。",
                )
            } else {
                system_error(error)
            });
        }
        if info.hProcess.is_null() {
            return Err(AppError::new(
                ErrorCode::SystemFailed,
                "Windows 未返回权限组件进程。",
            ));
        }
        let process = ProcessHandle(info.hProcess);
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            if let Ok((stream, _)) = listener.accept() {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                let mut bytes = Vec::new();
                if stream.take(16 * 1024).read_to_end(&mut bytes).is_ok() {
                    if let Ok(reply) = serde_json::from_slice::<Reply>(&bytes) {
                        if reply.token == token {
                            return reply.result;
                        }
                    }
                }
            }
            if unsafe { WaitForSingleObject(process.0, 0) } == WAIT_OBJECT_0 {
                return Err(AppError::new(
                    ErrorCode::SystemFailed,
                    "权限组件已退出，未完成 Sunshine 服务操作。",
                ));
            }
            thread::sleep(Duration::from_millis(50));
        }
        Err(AppError::new(
            ErrorCode::Timeout,
            "等待 Windows 权限组件超时，请稍后重试。",
        ))
    }
    pub fn control(start: bool) -> Result<()> {
        match direct(start) {
            Err(error) if error.code == ErrorCode::PermissionDenied => elevated(start),
            result => result,
        }
    }
    pub fn run_helper() -> Option<i32> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.first().map(String::as_str) != Some("--sunshine-service-helper") {
            return None;
        }
        if args.len() != 6 || args[2] != "--response-port" || args[4] != "--response-token" {
            return Some(2);
        }
        let start = match args[1].as_str() {
            "start" => true,
            "stop" => false,
            _ => return Some(2),
        };
        let port = match args[3].parse::<u16>() {
            Ok(port) if port != 0 => port,
            _ => return Some(2),
        };
        let token = &args[5];
        if token.len() != 32 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Some(2);
        }
        // This elevated entry point can only start or stop the fixed Sunshine
        // service. It never launches a webview, reads passwords, or writes files.
        let result = direct(start);
        let code = if result.is_ok() { 0 } else { 1 };
        if let Ok(mut stream) =
            TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2))
        {
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            if let Ok(bytes) = serde_json::to_vec(&Reply {
                token: token.clone(),
                result,
            }) {
                let _ = stream.write_all(&bytes);
            }
        }
        Some(code)
    }
}

pub fn control(start: bool) -> Result<()> {
    #[cfg(windows)]
    {
        windows::control(start)
    }
    #[cfg(not(windows))]
    {
        let _ = start;
        Err(AppError::new(
            ErrorCode::SystemFailed,
            "服务管理仅支持 Windows。",
        ))
    }
}
pub fn run_helper() -> Option<i32> {
    #[cfg(windows)]
    {
        windows::run_helper()
    }
    #[cfg(not(windows))]
    {
        None
    }
}
