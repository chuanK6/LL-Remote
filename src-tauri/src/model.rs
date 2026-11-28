use crate::error::{AppError, ErrorCode, Result};
use serde::{Deserialize, Serialize};

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct StreamPreferences {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub codec: String,
    pub window_mode: String,
    pub mouse_mode: String,
    pub audio_config: String,
    pub host_audio: bool,
    pub volume: u32,
    pub muted: bool,
    pub audio_device: String,
    pub capture_keys: String,
    pub gamepad: bool,
}
impl Default for StreamPreferences {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_kbps: 15000,
            codec: "auto".into(),
            window_mode: "windowed".into(),
            mouse_mode: "office".into(),
            audio_config: "stereo".into(),
            host_audio: false,
            volume: 75,
            muted: false,
            audio_device: String::new(),
            capture_keys: "fullscreen".into(),
            gamepad: true,
        }
    }
}
impl StreamPreferences {
    pub fn validate(&self) -> Result<()> {
        if !(320..=7680).contains(&self.width)
            || !(200..=4320).contains(&self.height)
            || self.width % 2 != 0
            || self.height % 2 != 0
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "分辨率须为有效偶数，范围为 320–7680 × 200–4320。",
            ));
        }
        if !(10..=360).contains(&self.fps)
            || !(1000..=500000).contains(&self.bitrate_kbps)
            || self.volume > 100
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "请检查帧率、码率和音量范围。",
            ));
        }
        for (value, allowed) in [
            (self.codec.as_str(), &["auto", "h264", "hevc", "av1"][..]),
            (
                self.window_mode.as_str(),
                &["fullscreen", "windowed", "borderless"][..],
            ),
            (self.mouse_mode.as_str(), &["game", "office"][..]),
            (self.audio_config.as_str(), &["stereo", "5.1", "7.1"][..]),
            (
                self.capture_keys.as_str(),
                &["fullscreen", "always", "never"][..],
            ),
        ] {
            if !allowed.contains(&value) {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "设置包含不支持的选项。",
                ));
            }
        }
        if self.audio_device.len() > 256 || self.audio_device.contains('\0') {
            return Err(AppError::new(ErrorCode::InvalidInput, "音频设备名称无效。"));
        }
        Ok(())
    }
    pub fn patched(&self, patch: &SessionPatch) -> Result<Self> {
        let mut value = self.clone();
        match patch {
            SessionPatch::Resolution { width, height } => {
                value.width = *width;
                value.height = *height;
            }
            SessionPatch::Bitrate { bitrate_kbps } => value.bitrate_kbps = *bitrate_kbps,
            SessionPatch::Fps { fps } => value.fps = *fps,
            SessionPatch::Mouse { mode } => value.mouse_mode = mode.clone(),
            SessionPatch::Window { mode } => value.window_mode = mode.clone(),
            SessionPatch::Audio {
                volume,
                muted,
                device,
                host_audio,
            } => {
                value.volume = *volume;
                value.muted = *muted;
                value.audio_device = device.clone();
                value.host_audio = *host_audio;
            }
        }
        value.validate()?;
        Ok(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionPatch {
    Resolution {
        width: u32,
        height: u32,
    },
    #[serde(rename_all = "camelCase")]
    Bitrate {
        bitrate_kbps: u32,
    },
    Fps {
        fps: u32,
    },
    Mouse {
        mode: String,
    },
    Window {
        mode: String,
    },
    #[serde(rename_all = "camelCase")]
    Audio {
        volume: u32,
        muted: bool,
        device: String,
        host_audio: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub prevent_sleep: bool,
    pub keep_display_awake: bool,
    pub shortcuts: crate::shortcuts::Shortcuts,
    pub autostart: bool,
    // Existing explicit choices remain persisted; fresh installs close to tray.
    pub keep_running_in_tray_on_close: bool,
    pub remember_login: bool,
    pub auto_retry: bool,
    pub always_accept_control: bool,
    pub automatic_role_switch: bool,
    #[serde(skip_deserializing)]
    pub accept_control: bool,
    pub show_float_ball: bool,
    pub show_performance_monitor: bool,
    pub setup_complete: bool,
    pub onboarding_complete: bool,
    pub first_connection_complete: bool,
    pub sunshine_username: Option<String>,
    pub reuse_existing: bool,
    pub moonlight_imported: bool,
    pub stream: StreamPreferences,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            prevent_sleep: true,
            keep_display_awake: false,
            shortcuts: crate::shortcuts::Shortcuts::default(),
            autostart: false,
            keep_running_in_tray_on_close: true,
            remember_login: true,
            auto_retry: true,
            always_accept_control: false,
            automatic_role_switch: false,
            accept_control: false,
            show_float_ball: true,
            show_performance_monitor: false,
            setup_complete: false,
            onboarding_complete: false,
            first_connection_complete: false,
            sunshine_username: None,
            reuse_existing: true,
            moonlight_imported: false,
            stream: StreamPreferences::default(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralPreferencesPatch {
    pub prevent_sleep: Option<bool>,
    pub keep_display_awake: Option<bool>,
    pub autostart: Option<bool>,
    pub keep_running_in_tray_on_close: Option<bool>,
    pub remember_login: Option<bool>,
    pub auto_retry: Option<bool>,
    pub always_accept_control: Option<bool>,
    pub automatic_role_switch: Option<bool>,
    pub show_float_ball: Option<bool>,
    pub reuse_existing: Option<bool>,
}

impl GeneralPreferencesPatch {
    pub fn apply(self, preferences: &mut Preferences) {
        for (setting, value) in [
            (&mut preferences.prevent_sleep, self.prevent_sleep),
            (&mut preferences.keep_display_awake, self.keep_display_awake),
            (&mut preferences.autostart, self.autostart),
            (
                &mut preferences.keep_running_in_tray_on_close,
                self.keep_running_in_tray_on_close,
            ),
            (&mut preferences.remember_login, self.remember_login),
            (&mut preferences.auto_retry, self.auto_retry),
            (
                &mut preferences.always_accept_control,
                self.always_accept_control,
            ),
            (
                &mut preferences.automatic_role_switch,
                self.automatic_role_switch,
            ),
            (&mut preferences.show_float_ball, self.show_float_ball),
            (&mut preferences.reuse_existing, self.reuse_existing),
        ] {
            if let Some(value) = value {
                *setting = value;
            }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Account {
    pub username: Option<String>,
    pub authenticated: bool,
    pub quota_minutes: Option<u64>,
    pub quota_state: String,
    pub updated_at: Option<String>,
    pub local_code: Option<String>,
    pub local_name: Option<String>,
    pub local_vpn_address: Option<String>,
    pub server_running: bool,
    pub protocol_supported: bool,
}

#[derive(Default, Serialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct LocalServerPasswords {
    pub connection_password: String,
    pub web_password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedApp {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Device {
    pub id: String,
    pub host_name: String,
    pub display_name: String,
    pub source: String,
    pub nat_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_host_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_nat_ids: Vec<String>,
    pub uuid: Option<String>,
    pub address: Option<String>,
    pub peer_code: Option<String>,
    pub status: String,
    #[serde(skip_deserializing)]
    pub network_connected: bool,
    #[serde(skip_deserializing)]
    pub account_online: bool,
    pub service_ready: Option<bool>,
    pub observed_at: Option<String>,
    pub last_connected_at: Option<String>,
    pub history_fallback_hidden: bool,
    pub favorite: bool,
    pub note: String,
    pub wallpaper: Option<String>,
    pub credential_ref: Option<String>,
    pub credential_state: String,
    pub preferences: StreamPreferences,
    pub apps: Vec<PublishedApp>,
    pub quick_apps: Vec<String>,
}
impl Default for Device {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            host_name: String::new(),
            display_name: String::new(),
            source: "existing".into(),
            nat_id: None,
            account_host_name: None,
            known_nat_ids: Vec::new(),
            uuid: None,
            address: None,
            peer_code: None,
            status: "unknown".into(),
            network_connected: false,
            account_online: false,
            service_ready: None,
            observed_at: None,
            last_connected_at: None,
            history_fallback_hidden: false,
            favorite: false,
            note: String::new(),
            wallpaper: None,
            credential_ref: None,
            credential_state: "missing".into(),
            preferences: StreamPreferences::default(),
            apps: vec![],
            quick_apps: vec!["Desktop".into()],
        }
    }
}
impl Device {
    pub fn same_host(&self, other: &Device) -> bool {
        let equal = |a: &Option<String>, b: &Option<String>| {
            a.as_ref().zip(b.as_ref()).is_some_and(|(a, b)| {
                !a.trim().is_empty() && a.trim().eq_ignore_ascii_case(b.trim())
            })
        };
        let confirmed_uuid = |device: &Device| {
            device
                .uuid
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty())
        };
        if (self.source == "direct") != (other.source == "direct") {
            return false;
        }
        if confirmed_uuid(self) && confirmed_uuid(other) {
            return equal(&self.uuid, &other.uuid);
        }
        self.nat_id.iter().chain(&self.known_nat_ids).any(|left| {
            other
                .nat_id
                .iter()
                .chain(&other.known_nat_ids)
                .any(|right| {
                    !left.trim().is_empty() && left.trim().eq_ignore_ascii_case(right.trim())
                })
        }) || equal(&self.peer_code, &other.peer_code)
            || (!self.id.is_empty() && self.id == other.id)
    }
    pub fn credential_identity(&self) -> Result<String> {
        self.uuid
            .clone()
            .or_else(|| self.nat_id.clone())
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::CredentialUnavailable,
                    "尚未确认设备身份，不能复用保存的连接凭据。",
                )
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub device_id: Option<String>,
    pub peer_code: Option<String>,
    pub connected_at: String,
    pub method: String,
    pub result: String,
    pub error_code: Option<ErrorCode>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountData {
    pub devices: Vec<Device>,
    pub history: Vec<HistoryEntry>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatus {
    pub name: String,
    pub available: bool,
    pub running: bool,
    pub ready: bool,
    pub owned: bool,
    pub version: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SunshineStatus {
    pub running: bool,
    pub ready: bool,
    pub account_initialized: bool,
    pub account_username: Option<String>,
    pub credential_valid: bool,
    pub external: bool,
    pub port: u16,
    pub locale: String,
    pub apps: Vec<PublishedApp>,
    pub pending_pairings: Vec<PendingPairing>,
    pub config: SunshineSettings,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SunshineSettings {
    pub output_name: String,
    pub audio_sink: String,
    pub adapter_name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPairing {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingControl {
    pub count: usize,
    pub started_at: Option<String>,
    pub device_name: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub id: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub stage: String,
    pub attempt: u32,
    pub app: Option<String>,
    pub pin: Option<String>,
    pub error: Option<AppError>,
    pub preferences: Option<StreamPreferences>,
    pub started_at: Option<String>,
    pub metrics: Metrics,
    pub extension_available: bool,
    #[serde(default)]
    pub local_acceptance_paused: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub fps: Option<f64>,
    pub bitrate_mbps: Option<f64>,
    pub network_ms: Option<f64>,
    pub decode_ms: Option<f64>,
    pub render_ms: Option<f64>,
    pub dropped_percent: Option<f64>,
    pub updated_at: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub name: String,
    pub status: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub version: String,
    pub portable: bool,
    pub data_directory: String,
    pub preferences: Preferences,
    pub account: Account,
    pub devices: Vec<Device>,
    pub history: Vec<HistoryEntry>,
    pub sunshine: SunshineStatus,
    pub direct_acceptance: Option<DirectAcceptanceStatus>,
    pub session: SessionSnapshot,
    pub components: Vec<ComponentStatus>,
    pub power_status: String,
    pub storage_warning: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DirectAccess {
    Lan,
    Internet,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectAcceptanceStatus {
    pub access: DirectAccess,
    pub address: String,
    pub port: u16,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEdit {
    pub device_id: String,
    pub display_name: String,
    pub favorite: bool,
    pub note: String,
    pub wallpaper_path: Option<String>,
    pub reset_wallpaper: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub device_id: Option<String>,
    pub peer_code: Option<String>,
    pub password: Option<String>,
    pub app: Option<String>,
    pub address: Option<String>,
}
impl Drop for ConnectRequest {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        if let Some(password) = &mut self.password {
            password.zeroize();
        }
    }
}
