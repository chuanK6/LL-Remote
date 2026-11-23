use crate::{
    error::{AppError, ErrorCode, Result},
    model::{Device, PublishedApp, StreamPreferences},
};
use std::path::{Path, PathBuf};

#[cfg(windows)]
pub fn bring_window_forward(window: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::{
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            GetWindowThreadProcessId, IsIconic, ShowWindow, ShowWindowAsync, SW_RESTORE, SW_SHOW,
        },
    };
    unsafe {
        let show = if IsIconic(window) != 0 { SW_RESTORE } else { SW_SHOW };
        if GetWindowThreadProcessId(window, std::ptr::null_mut()) == GetCurrentThreadId() {
            ShowWindow(window, show);
        } else {
            ShowWindowAsync(window, show);
        }
    }
    activate_window(window)
}

#[cfg(windows)]
pub fn activate_window(window: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::{
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
        },
    };
    unsafe {
        if GetForegroundWindow() == window || SetForegroundWindow(window) != 0 {
            return true;
        }
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
        let current_thread = GetCurrentThreadId();
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, 1) != 0;
        BringWindowToTop(window);
        let focused = SetForegroundWindow(window) != 0 || GetForegroundWindow() == window;
        if attached {
            AttachThreadInput(current_thread, foreground_thread, 0);
        }
        focused
    }
}

pub struct PowerManager {
    handle: isize,
    system: bool,
    display: bool,
    pub status: String,
}
impl PowerManager {
    pub fn new() -> Self {
        Self {
            handle: 0,
            system: false,
            display: false,
            status: "尚未应用".into(),
        }
    }
    #[cfg(windows)]
    pub fn apply(&mut self, system: bool, display: bool) -> Result<()> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            System::{
                Power::{
                    PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired,
                    PowerRequestSystemRequired, PowerSetRequest,
                },
                Threading::REASON_CONTEXT,
            },
        };
        if self.system == system && self.display == display && self.status != "尚未应用" {
            return Ok(());
        }
        let handle = if system || display {
            let mut reason: Vec<u16> = "LL远程正在运行，保持远程连接可用"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut context: REASON_CONTEXT = unsafe { std::mem::zeroed() };
            context.Version = 0;
            context.Flags = 1;
            context.Reason.SimpleReasonString = reason.as_mut_ptr();
            let handle = unsafe { PowerCreateRequest(&context) };
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                self.status = "电源请求创建失败".into();
                return Err(AppError::new(
                    ErrorCode::SystemFailed,
                    "Windows 未能创建防休眠请求。",
                ));
            }
            // Windows requires a system request alongside display-required.
            let system_set = unsafe { PowerSetRequest(handle, PowerRequestSystemRequired) } != 0;
            let display_set = !display
                || (system_set
                    && unsafe { PowerSetRequest(handle, PowerRequestDisplayRequired) } != 0);
            if !system_set || !display_set {
                unsafe {
                    if system_set {
                        PowerClearRequest(handle, PowerRequestSystemRequired);
                    }
                    CloseHandle(handle);
                }
                return Err(AppError::new(
                    ErrorCode::SystemFailed,
                    "Windows 未能应用新的防休眠或常亮设置。",
                ));
            }
            handle as isize
        } else {
            0
        };
        if self.handle != 0 {
            unsafe {
                if self.display {
                    PowerClearRequest(self.handle as _, PowerRequestDisplayRequired);
                }
                PowerClearRequest(self.handle as _, PowerRequestSystemRequired);
                CloseHandle(self.handle as _);
            }
        }
        self.handle = handle;
        self.system = system;
        self.display = display;
        self.status = if display {
            "已请求防止自动休眠并保持显示器常亮"
        } else if system {
            "已请求防止自动休眠"
        } else {
            "已关闭"
        }
        .into();
        Ok(())
    }
    #[cfg(not(windows))]
    pub fn apply(&mut self, _: bool, _: bool) -> Result<()> {
        self.status = "仅支持 Windows".into();
        Ok(())
    }
}
impl Drop for PowerManager {
    fn drop(&mut self) {
        let _ = self.apply(false, false);
    }
}

#[cfg(windows)]
pub fn set_autostart(enabled: bool, executable: &Path) -> Result<()> {
    let sid = autostart_user_sid()?;
    let task_name = format!("LLRemote-{sid}");
    let previous = read_autostart_task(&task_name)?;
    if enabled {
        write_autostart_task(&task_name, &autostart_task_xml(&sid, executable))?;
    } else if previous.is_some() {
        delete_autostart_task(&task_name)?;
    }

    let finished = (|| {
        if enabled
            && !read_autostart_task(&task_name)?
                .is_some_and(|xml| autostart_task_matches(&xml, &sid, executable))
        {
            return Err(autostart_error(
                "Windows 登录任务校验未通过，请重新设置开机自启动。",
            ));
        }
        remove_legacy_autostart()
    })();
    if let Err(error) = finished {
        let restored = match previous {
            Some(xml) => write_autostart_task(&task_name, &xml),
            None if enabled => delete_autostart_task(&task_name),
            None => Ok(()),
        };
        if restored.is_err() {
            return Err(AppError::new(
                ErrorCode::SystemFailed,
                "自启动设置未完成，Windows 未能恢复原启动任务，请重新设置开机自启动。",
            ));
        }
        return Err(error);
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn set_autostart(_: bool, _: &Path) -> Result<()> {
    Ok(())
}
#[cfg(windows)]
pub fn autostart_enabled(executable: &Path) -> bool {
    let Ok(sid) = autostart_user_sid() else {
        return false;
    };
    read_autostart_task(&format!("LLRemote-{sid}"))
        .ok()
        .flatten()
        .is_some_and(|xml| autostart_task_matches(&xml, &sid, executable))
}
#[cfg(not(windows))]
pub fn autostart_enabled(_: &Path) -> bool {
    false
}

#[cfg(windows)]
pub fn migrate_autostart(executable: &Path) -> Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let key = match root.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(autostart_error("无法读取当前用户的原自启动设置。")),
    };
    match key.get_value::<String, _>("LLRemote") {
        Ok(value) if value == format!("\"{}\" --background", executable.display()) => {
            set_autostart(true, executable)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(autostart_error("无法读取当前用户的原自启动设置。")),
    }
}

#[cfg(not(windows))]
pub fn migrate_autostart(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn autostart_error(message: &str) -> AppError {
    AppError::new(ErrorCode::SystemFailed, message)
}

#[cfg(windows)]
fn autostart_user_sid() -> Result<String> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        Security::{
            GetSidIdentifierAuthority, GetSidSubAuthority, GetSidSubAuthorityCount,
            GetTokenInformation, IsValidSid, TokenUser, SID, TOKEN_QUERY, TOKEN_USER,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(autostart_error("无法读取 Windows 当前用户身份。"));
    }
    let mut needed = 0;
    unsafe {
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
    }
    let mut buffer = vec![0usize; needed as usize / std::mem::size_of::<usize>() + 1];
    let available = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr() as _,
            needed,
            &mut needed,
        )
    } != 0;
    unsafe { CloseHandle(token) };
    if !available {
        return Err(autostart_error("无法读取 Windows 当前用户身份。"));
    }
    let sid = unsafe { (*(buffer.as_ptr() as *const TOKEN_USER)).User.Sid };
    if unsafe { IsValidSid(sid) } == 0 {
        return Err(autostart_error("Windows 当前用户身份无效。"));
    }
    let authority = unsafe { (*GetSidIdentifierAuthority(sid)).Value }
        .into_iter()
        .fold(0u64, |value, byte| (value << 8) | u64::from(byte));
    let revision = unsafe { (*(sid as *const SID)).Revision };
    let mut value = format!("S-{revision}-{authority}");
    let count = unsafe { *GetSidSubAuthorityCount(sid) };
    for index in 0..count {
        let component = unsafe { *GetSidSubAuthority(sid, u32::from(index)) };
        value.push_str(&format!("-{component}"));
    }
    Ok(value)
}

#[cfg(windows)]
fn autostart_task_xml(sid: &str, executable: &Path) -> String {
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    };
    let command = escape(&executable.to_string_lossy());
    let directory = escape(
        &executable
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy(),
    );
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers>
    <LogonTrigger><Enabled>true</Enabled><UserId>{sid}</UserId><Delay>PT5S</Delay></LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="User">
      <UserId>{sid}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <Enabled>true</Enabled>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
  </Settings>
  <Actions Context="User">
    <Exec>
      <Command>{command}</Command>
      <Arguments>--background</Arguments>
      <WorkingDirectory>{directory}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>"#
    )
}

#[cfg(windows)]
fn autostart_task_matches(xml: &str, sid: &str, executable: &Path) -> bool {
    fn child<'a, 'input: 'a>(
        node: roxmltree::Node<'a, 'input>,
        name: &str,
    ) -> Option<roxmltree::Node<'a, 'input>> {
        node.children().find(|candidate| candidate.has_tag_name(name))
    }
    fn value<'a, 'input: 'a>(node: roxmltree::Node<'a, 'input>, name: &str) -> Option<&'a str> {
        child(node, name).and_then(|item| item.text()).map(str::trim)
    }
    let Ok(document) = roxmltree::Document::parse(xml) else {
        return false;
    };
    let task = document.root_element();
    let Some(trigger) = child(task, "Triggers").and_then(|node| child(node, "LogonTrigger")) else {
        return false;
    };
    let Some(principal) = child(task, "Principals").and_then(|node| child(node, "Principal")) else {
        return false;
    };
    let Some(settings) = child(task, "Settings") else {
        return false;
    };
    let Some(action) = child(task, "Actions").and_then(|node| child(node, "Exec")) else {
        return false;
    };
    let enabled = |node| matches!(value(node, "Enabled"), None | Some("true" | "1"));
    let path_matches = |actual: &str, expected: &Path| {
        let actual = Path::new(actual.trim_matches('"'));
        let normalize = |path: &Path| {
            path.canonicalize()
                .unwrap_or_else(|_| path.to_path_buf())
                .to_string_lossy()
                .replace('/', "\\")
                .trim_start_matches(r"\\?\")
                .to_lowercase()
        };
        normalize(actual) == normalize(expected)
    };
    enabled(trigger)
        && enabled(settings)
        && value(trigger, "UserId") == Some(sid)
        && value(principal, "UserId") == Some(sid)
        && value(principal, "LogonType") == Some("InteractiveToken")
        && value(principal, "RunLevel") == Some("HighestAvailable")
        && value(action, "Command").is_some_and(|actual| path_matches(actual, executable))
        && value(action, "Arguments") == Some("--background")
}

#[cfg(windows)]
fn autostart_command(arguments: &[&std::ffi::OsStr]) -> Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    let executable =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32/schtasks.exe");
    std::process::Command::new(executable)
        .args(arguments)
        .creation_flags(0x08000000)
        .output()
        .map_err(|_| autostart_error("Windows 登录任务管理器无法运行。"))
}

#[cfg(windows)]
fn decode_autostart_xml(bytes: &[u8]) -> Result<String> {
    use windows_sys::Win32::Globalization::{GetACP, GetOEMCP};
    let code_pages = unsafe { [GetOEMCP(), GetACP()] };
    decode_autostart_xml_with_code_pages(bytes, &code_pages)
}

#[cfg(windows)]
fn decode_autostart_xml_with_code_pages(bytes: &[u8], code_pages: &[u32]) -> Result<String> {
    let utf16 = bytes.starts_with(&[0xff, 0xfe]) || bytes.get(1) == Some(&0);
    if utf16 {
        let payload = if bytes.starts_with(&[0xff, 0xfe]) {
            &bytes[2..]
        } else {
            bytes
        };
        if payload.len() % 2 != 0 {
            return Err(autostart_error("Windows 登录任务配置不完整。"));
        }
        let units: Vec<u16> = payload
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units).map_err(|_| autostart_error("Windows 登录任务配置无法读取。"))
    } else if let Ok(xml) = std::str::from_utf8(bytes) {
        Ok(xml.trim_start_matches('\u{feff}').to_owned())
    } else {
        code_pages
            .iter()
            .find_map(|&code_page| decode_windows_code_page(bytes, code_page))
            .ok_or_else(|| autostart_error("Windows 登录任务配置无法读取。"))
    }
}

#[cfg(windows)]
pub(crate) fn decode_system_ansi(bytes: &[u8]) -> Option<String> {
    use windows_sys::Win32::Globalization::CP_ACP;
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};

    // The UTF-8 manifest changes this process's ACP, while a child without
    // that manifest still uses the system's configured ACP.
    let code_page = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Nls\CodePage")
        .and_then(|key| key.get_value::<String, _>("ACP"))
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(CP_ACP);
    decode_windows_code_page(bytes, code_page)
}

#[cfg(windows)]
fn decode_windows_code_page(bytes: &[u8], code_page: u32) -> Option<String> {
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, MB_ERR_INVALID_CHARS};
    let length = i32::try_from(bytes.len()).ok()?;
    let needed = unsafe {
        MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            length,
            std::ptr::null_mut(),
            0,
        )
    };
    if needed == 0 {
        return None;
    }
    let mut units = vec![0u16; needed as usize];
    let written = unsafe {
        MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            length,
            units.as_mut_ptr(),
            needed,
        )
    };
    if written != needed {
        return None;
    }
    String::from_utf16(&units).ok()
}

#[cfg(windows)]
fn read_autostart_task(name: &str) -> Result<Option<String>> {
    use base64::Engine;
    use std::os::windows::process::CommandExt;
    // Read Scheduler's native Unicode XML independently of the console code page.
    let script = r#"
$ErrorActionPreference = 'Stop'
$scheduler = New-Object -ComObject 'Schedule.Service'
$scheduler.Connect()
$task = $scheduler.GetFolder('\').GetTasks(1) | Where-Object { $_.Name -eq '__TASK__' } | Select-Object -First 1
if ($null -eq $task) { exit 0 }
[xml]$document = $task.Xml
$ns = [System.Xml.XmlNamespaceManager]::new($document.NameTable)
$ns.AddNamespace('t', $document.DocumentElement.NamespaceURI)
foreach ($user in $document.SelectNodes('//t:UserId', $ns)) {
    if ($user.InnerText -and $user.InnerText -notlike 'S-1-*') {
        try { $user.InnerText = ([Security.Principal.NTAccount]::new($user.InnerText)).Translate([Security.Principal.SecurityIdentifier]).Value } catch {}
    }
}
$principal = $document.SelectSingleNode('/t:Task/t:Principals/t:Principal', $ns)
if ($principal -and $task.Definition.Principal.LogonType -eq 3 -and -not $principal.SelectSingleNode('t:LogonType', $ns)) {
    $logonType = $document.CreateElement('LogonType', $principal.NamespaceURI)
    $logonType.InnerText = 'InteractiveToken'
    [void]$principal.AppendChild($logonType)
}
$encoding = [Text.Encoding]::Unicode
$preamble = $encoding.GetPreamble()
$bytes = $encoding.GetBytes($document.OuterXml)
$stdout = [Console]::OpenStandardOutput()
$stdout.Write($preamble, 0, $preamble.Length)
$stdout.Write($bytes, 0, $bytes.Length)
"#;
    let script = script.replace("__TASK__", &name.replace('\'', "''"));
    let script = base64::engine::general_purpose::STANDARD
        .encode(script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>());
    let powershell =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let result = std::process::Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &script])
        .creation_flags(0x08000000)
        .output()
        .map_err(|_| autostart_error("Windows 登录任务管理器无法运行。"))?;
    if !result.status.success() {
        return Err(autostart_error("无法读取 Windows 当前用户的登录任务。"));
    }
    if result.stdout.is_empty() {
        return Ok(None);
    }
    decode_autostart_xml(&result.stdout).map(Some)
}

#[cfg(windows)]
fn write_autostart_task(name: &str, xml: &str) -> Result<()> {
    use std::ffi::OsStr;
    let temporary =
        std::env::temp_dir().join(format!("LLRemote-autostart-{}.xml", uuid::Uuid::new_v4()));
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&temporary, bytes)
        .map_err(|_| autostart_error("无法准备 Windows 登录任务配置。"))?;
    let result = autostart_command(&[
        OsStr::new("/Create"),
        OsStr::new("/TN"),
        OsStr::new(name),
        OsStr::new("/XML"),
        temporary.as_os_str(),
        OsStr::new("/F"),
    ]);
    let _ = std::fs::remove_file(&temporary);
    let result = result?;
    if result.status.success() {
        Ok(())
    } else {
        tracing::warn!(exit_code = ?result.status.code(), "Windows could not register LLRemote logon task");
        Err(autostart_error(
            "Windows 未接受管理员登录任务，请检查开机自启动设置。",
        ))
    }
}

#[cfg(windows)]
fn delete_autostart_task(name: &str) -> Result<()> {
    use std::ffi::OsStr;
    let result = autostart_command(&[
        OsStr::new("/Delete"),
        OsStr::new("/TN"),
        OsStr::new(name),
        OsStr::new("/F"),
    ])?;
    if result.status.success() {
        Ok(())
    } else {
        Err(autostart_error("Windows 未能移除当前用户的登录任务。"))
    }
}

#[cfg(windows)]
fn remove_legacy_autostart() -> Result<()> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_SET_VALUE},
        RegKey,
    };
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let key = match root.open_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        KEY_SET_VALUE,
    ) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(autostart_error("无法移除原 Windows 自启动入口。")),
    };
    match key.delete_value("LLRemote") {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(autostart_error("无法移除原 Windows 自启动入口。")),
    }
}

#[cfg(all(test, windows))]
mod autostart_tests {
    use super::*;

    #[test]
    fn elevated_logon_task_preserves_unicode_path_and_background_arguments() {
        let executable = Path::new("E:\\中文 路径 & 文件\\LLRemote.exe");
        let sid = "S-1-5-21-1-2-3-1001";
        let xml = autostart_task_xml(sid, executable);
        assert!(autostart_task_matches(&xml, sid, executable));
        assert!(!autostart_task_matches(
            &xml,
            "S-1-5-21-1-2-3-1002",
            executable
        ));
        assert!(!autostart_task_matches(
            &xml.replace("<Enabled>true", "<Enabled>false"),
            sid,
            executable
        ));
        assert!(!autostart_task_matches(
            &xml.replace("LogonTrigger", "TimeTrigger"),
            sid,
            executable
        ));
        let document = roxmltree::Document::parse(&xml).unwrap();
        assert!(document
            .descendants()
            .any(|node| node.has_tag_name("ExecutionTimeLimit") && node.text() == Some("PT0S")));
        assert!(document
            .descendants()
            .any(|node| node.has_tag_name("DisallowStartIfOnBatteries")
                && node.text() == Some("false")));
    }

    #[test]
    fn scheduled_task_output_accepts_utf16_and_utf8_without_losing_unicode() {
        let executable = Path::new("E:\\中文 路径\\LLRemote.exe");
        let sid = "S-1-5-21-1-2-3-1001";
        let xml = autostart_task_xml(sid, executable);
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
        assert!(autostart_task_matches(
            &decode_autostart_xml(&bytes).unwrap(),
            sid,
            executable
        ));
        assert!(autostart_task_matches(
            &decode_autostart_xml(xml.as_bytes()).unwrap(),
            sid,
            executable
        ));
        assert!(!autostart_task_matches(
            &xml.replace("HighestAvailable", "LeastPrivilege"),
            sid,
            executable
        ));
    }

    #[test]
    fn scheduled_task_query_decodes_cp936_bytes_even_with_utf16_declaration() {
        let sid = "S-1-5-21-1-2-3-1001";
        let template = autostart_task_xml(sid, Path::new("E:\\PLACEHOLDER\\LLRemote.exe"));
        let chinese_path = [0xd6, 0xd0, 0xce, 0xc4, b' ', 0xc2, 0xb7, 0xbe, 0xb6];
        let mut bytes = Vec::new();
        for (index, part) in template.split("PLACEHOLDER").enumerate() {
            if index != 0 {
                bytes.extend_from_slice(&chinese_path);
            }
            bytes.extend_from_slice(part.as_bytes());
        }
        let decoded = decode_autostart_xml_with_code_pages(&bytes, &[936]).unwrap();
        assert!(decoded.contains("encoding=\"UTF-16\""));
        assert!(autostart_task_matches(
            &decoded,
            sid,
            Path::new("E:\\中文 路径\\LLRemote.exe")
        ));
    }
}
#[derive(Clone)]
pub struct ServiceInfo {
    pub running: bool,
    pub sunshine_root: PathBuf,
    pub process_id: u32,
}
#[cfg(windows)]
pub fn sunshine_service() -> Option<ServiceInfo> {
    use windows_sys::Win32::System::Services::*;
    let name: Vec<u16> = "SunshineService".encode_utf16().chain(Some(0)).collect();
    unsafe {
        let manager = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if manager.is_null() {
            return None;
        }
        let service = OpenServiceW(
            manager,
            name.as_ptr(),
            SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS,
        );
        if service.is_null() {
            CloseServiceHandle(manager);
            return None;
        }
        let mut needed = 0;
        QueryServiceConfigW(service, std::ptr::null_mut(), 0, &mut needed);
        let mut buffer = vec![0usize; needed as usize / std::mem::size_of::<usize>() + 1];
        let config = buffer.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;
        let result = if QueryServiceConfigW(service, config, needed, &mut needed) != 0 {
            let pointer = (*config).lpBinaryPathName;
            let mut len = 0;
            while *pointer.add(len) != 0 {
                len += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, len));
            let path = if text.starts_with('"') {
                text.split('"').nth(1).unwrap_or("")
            } else {
                text.trim()
            };
            let root = Path::new(path).parent().and_then(|p| {
                if p.file_name()?
                    .to_string_lossy()
                    .eq_ignore_ascii_case("tools")
                {
                    p.parent()
                } else {
                    Some(p)
                }
            });
            let mut status: SERVICE_STATUS_PROCESS = std::mem::zeroed();
            let mut status_size = 0;
            let running = QueryServiceStatusEx(
                service,
                SC_STATUS_PROCESS_INFO,
                &mut status as *mut _ as _,
                std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut status_size,
            ) != 0
                && status.dwCurrentState == SERVICE_RUNNING;
            root.map(|p| ServiceInfo {
                running,
                sunshine_root: p.into(),
                process_id: status.dwProcessId,
            })
        } else {
            None
        };
        CloseServiceHandle(service);
        CloseServiceHandle(manager);
        result
    }
}
#[cfg(not(windows))]
pub fn sunshine_service() -> Option<ServiceInfo> {
    None
}

pub fn control_sunshine_service(start: bool) -> Result<()> {
    crate::sunshine_service::control(start)
}

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let mut elevation: TOKEN_ELEVATION = unsafe { std::mem::zeroed() };
    let mut size = 0;
    let available = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        )
    } != 0;
    unsafe {
        CloseHandle(token);
    }
    available && elevation.TokenIsElevated != 0
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

#[cfg(windows)]
pub fn launch_elevated_nat_host(executable: &Path, directory: &Path, port: u16) -> Result<isize> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_CANCELLED},
        System::Com::{
            CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
        },
        UI::{
            Shell::{
                ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::SW_HIDE,
        },
    };
    if executable.file_name().and_then(|name| name.to_str()) != Some("LLRemote.EngineHost.exe")
        || !executable.is_file()
        || !directory.is_dir()
    {
        return Err(AppError::new(
            ErrorCode::ComponentMissing,
            "皎月连网络宿主或数据目录缺失。",
        ));
    }
    let file: Vec<u16> = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let cwd: Vec<u16> = directory.as_os_str().encode_wide().chain(Some(0)).collect();
    let verb: Vec<u16> = "runas".encode_utf16().chain(Some(0)).collect();
    let args: Vec<u16> = format!("-p {port}").encode_utf16().chain(Some(0)).collect();
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = args.as_ptr();
    info.lpDirectory = cwd.as_ptr();
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
        return Err(AppError::new(
            if error == ERROR_CANCELLED {
                ErrorCode::Canceled
            } else {
                ErrorCode::PermissionDenied
            },
            if error == ERROR_CANCELLED {
                "已取消网络宿主的 Windows 授权，可重新开启接受控制。"
            } else {
                "Windows 无法启动管理员网络宿主，请检查权限或系统策略。"
            },
        ));
    }
    if info.hProcess.is_null() {
        return Err(AppError::new(
            ErrorCode::SystemFailed,
            "Windows 未返回网络宿主进程。",
        ));
    }
    Ok(info.hProcess as isize)
}

#[cfg(windows)]
fn registry_number(key: &winreg::RegKey, name: &str) -> Option<u32> {
    key.get_value::<u32, _>(name)
        .ok()
        .or_else(|| {
            key.get_value::<u64, _>(name)
                .ok()
                .and_then(|value| value.try_into().ok())
        })
        .or_else(|| {
            key.get_value::<String, _>(name)
                .ok()
                .and_then(|value| match value.as_str() {
                    "true" => Some(1),
                    "false" => Some(0),
                    _ => value.parse().ok(),
                })
        })
}

#[cfg(windows)]
pub fn imported_moonlight_hosts(default: &mut StreamPreferences) -> Vec<Device> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let Ok(root) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Moonlight Game Streaming Project\\Moonlight")
    else {
        return vec![];
    };
    for (key, target) in [
        ("width", &mut default.width),
        ("height", &mut default.height),
        ("fps", &mut default.fps),
        ("bitrate", &mut default.bitrate_kbps),
    ] {
        if let Some(value) = registry_number(&root, key) {
            *target = value;
        }
    }
    if let Some(value) = registry_number(&root, "mouseacceleration") {
        default.mouse_mode = if value != 0 { "office" } else { "game" }.into();
    }
    if let Some(value) = registry_number(&root, "windowmode") {
        default.window_mode = match value {
            0 => "fullscreen",
            1 => "borderless",
            2 => "windowed",
            _ => "windowed",
        }
        .into();
    }
    if let Some(value) = registry_number(&root, "videocfg") {
        default.codec = match value {
            1 => "h264",
            2 | 3 => "hevc",
            4 => "av1",
            _ => "auto",
        }
        .into();
    }
    if let Some(value) = registry_number(&root, "hostaudio") {
        default.host_audio = value != 0;
    }
    if let Some(value) = registry_number(&root, "backgroundgamepad") {
        default.gamepad = value != 0;
    }
    if let Some(value) = registry_number(&root, "capturesyskeys") {
        default.capture_keys = match value {
            0 => "never",
            2 => "always",
            _ => "fullscreen",
        }
        .into();
    }
    if let Some(value) = registry_number(&root, "audiocfg") {
        default.audio_config = match value {
            1 => "5.1",
            2 => "7.1",
            _ => "stereo",
        }
        .into();
    }
    let Ok(hosts) = root.open_subkey("hosts") else {
        return vec![];
    };
    let count = registry_number(&hosts, "size").unwrap_or(0).min(2000);
    let mut devices = vec![];
    for index in 1..=count {
        let Ok(host) = hosts.open_subkey(index.to_string()) else {
            continue;
        };
        // Read only these public fields, never Moonlight certificate/private-key values.
        let name = host.get_value::<String, _>("hostname").unwrap_or_default();
        let uuid = host
            .get_value::<String, _>("uuid")
            .ok()
            .filter(|s| !s.is_empty());
        if name.is_empty() || uuid.is_none() {
            continue;
        }
        let address = [
            "manualaddress",
            "localaddress",
            "remoteaddress",
            "ipv6address",
        ]
        .iter()
        .find_map(|key| {
            host.get_value::<String, _>(key)
                .ok()
                .filter(|s| !s.is_empty())
        });
        let mut apps = vec![];
        if let Ok(list) = host.open_subkey("apps") {
            let count = registry_number(&list, "size").unwrap_or(0).min(500);
            for index in 1..=count {
                if let Ok(application) = list.open_subkey(index.to_string()) {
                    if let Ok(name) = application.get_value::<String, _>("name") {
                        let id = registry_number(&application, "id")
                            .unwrap_or(index)
                            .to_string();
                        apps.push(PublishedApp { id, name });
                    }
                }
            }
        }
        devices.push(Device {
            host_name: name.clone(),
            display_name: name,
            uuid,
            address,
            preferences: default.clone(),
            apps,
            ..Device::default()
        });
    }
    devices
}
#[cfg(not(windows))]
pub fn imported_moonlight_hosts(_: &mut StreamPreferences) -> Vec<Device> {
    vec![]
}

pub fn open_url(url: &str) -> Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let action: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                action.as_ptr(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result as isize <= 32 {
            return Err(AppError::new(
                ErrorCode::SystemFailed,
                "无法打开系统浏览器。",
            ));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        return Err(AppError::new(
            ErrorCode::SystemFailed,
            "此操作仅支持 Windows。",
        ));
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(windows)]
pub fn verify_listener(port: u16, executable: &Path) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        NetworkManagement::IpHelper::*,
        Networking::WinSock::{AF_INET, AF_INET6},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    };
    let normalize = |path: &Path| {
        path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .replace('/', "\\")
            .trim_start_matches("\\\\?\\")
            .to_lowercase()
    };
    let expected = normalize(executable);
    // SCM exposes the service PID to ordinary users, even when querying its
    // LocalSystem child's image path is denied. Only that service's Sunshine
    // child is accepted as the fallback identity for its registered directory.
    let service = sunshine_service().filter(|service| {
        service.running
            && service.process_id != 0
            && normalize(&service.sunshine_root.join("sunshine.exe")) == expected
    });
    let mut owners = vec![];
    unsafe {
        for family in [AF_INET, AF_INET6] {
            let mut needed = 0;
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut needed,
                0,
                family as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            );
            if needed == 0 || needed > 16 * 1024 * 1024 {
                continue;
            }
            let mut buffer = vec![0usize; needed as usize / std::mem::size_of::<usize>() + 1];
            if GetExtendedTcpTable(
                buffer.as_mut_ptr() as _,
                &mut needed,
                0,
                family as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            ) != 0
            {
                continue;
            }
            if family == AF_INET {
                let table = buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID;
                for row in std::slice::from_raw_parts(
                    (*table).table.as_ptr(),
                    (*table).dwNumEntries as usize,
                ) {
                    if u16::from_be(row.dwLocalPort as u16) == port
                        && (row.dwLocalAddr == 0
                            || row.dwLocalAddr == u32::from_ne_bytes([127, 0, 0, 1]))
                    {
                        owners.push(row.dwOwningPid);
                    }
                }
            } else {
                let table = buffer.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID;
                for row in std::slice::from_raw_parts(
                    (*table).table.as_ptr(),
                    (*table).dwNumEntries as usize,
                ) {
                    if owners.is_empty()
                        && u16::from_be(row.dwLocalPort as u16) == port
                        && row.ucLocalAddr == [0; 16]
                    {
                        owners.push(row.dwOwningPid);
                    }
                }
            }
        }
        let mut service_children = vec![];
        if let Some(service) = service {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot != INVALID_HANDLE_VALUE {
                let mut entry: PROCESSENTRY32W = std::mem::zeroed();
                entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
                let mut next = Process32FirstW(snapshot, &mut entry);
                while next != 0 {
                    let length = entry
                        .szExeFile
                        .iter()
                        .position(|value| *value == 0)
                        .unwrap_or(entry.szExeFile.len());
                    if entry.th32ParentProcessID == service.process_id
                        && String::from_utf16_lossy(&entry.szExeFile[..length])
                            .eq_ignore_ascii_case("sunshine.exe")
                    {
                        service_children.push(entry.th32ProcessID);
                    }
                    next = Process32NextW(snapshot, &mut entry);
                }
                CloseHandle(snapshot);
            }
        }
        owners.into_iter().any(|pid| {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return service_children.contains(&pid);
            }
            let mut path = vec![0u16; 32768];
            let mut length = path.len() as u32;
            let success = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length);
            CloseHandle(process);
            if success == 0 {
                return service_children.contains(&pid);
            }
            normalize(Path::new(&String::from_utf16_lossy(
                &path[..length as usize],
            ))) == expected
        })
    }
}
#[cfg(not(windows))]
pub fn verify_listener(_: u16, _: &Path) -> bool {
    false
}
