use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    ComponentMissing,
    ProtocolUnsupported,
    LocalAuthorizationRequired,
    LoginRequired,
    LoginFailed,
    AccountInUse,
    Offline,
    NetworkFailed,
    QuotaInsufficient,
    ServiceExpired,
    CredentialUnavailable,
    PasswordRejected,
    VirtualAdapterFailed,
    SunshineUnavailable,
    AddressRequired,
    RoleSwitchRequired,
    SunshineAccountRequired,
    PairingRequired,
    PairingFailed,
    StreamFailed,
    Busy,
    Canceled,
    Timeout,
    StorageFailed,
    SystemFailed,
    PermissionDenied,
    SessionEnded,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub reauthenticate: bool,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let retryable = matches!(code, ErrorCode::NetworkFailed | ErrorCode::Timeout);
        let reauthenticate = matches!(
            code,
            ErrorCode::PasswordRejected | ErrorCode::CredentialUnavailable
        );
        Self {
            code,
            message: message.into(),
            retryable,
            reauthenticate,
        }
    }

    // Do not propagate arbitrary native responses: they can echo a secret.
    pub fn native_info(message: &str, login: bool) -> Self {
        if message.contains("网卡") || message.to_lowercase().contains("wintun") {
            Self::new(
                ErrorCode::VirtualAdapterFailed,
                "皎月连 Wintun 网卡初始化失败，请查看连接诊断中的 Windows 原始错误。",
            )
        } else if message.contains("权限")
            || message.contains("拒绝访问")
            || message.to_lowercase().contains("access is denied")
        {
            Self::new(
                ErrorCode::PermissionDenied,
                "皎月连网络初始化权限不足，请接受 Windows 管理员授权后重试。",
            )
        } else if message.contains("到期") || message.contains("已过期") {
            Self::new(
                ErrorCode::ServiceExpired,
                "皎月连服务已到期，请点击签到或检查套餐。",
            )
        } else if message.contains("时长不足") || message.contains("余额不足") {
            Self::new(
                ErrorCode::QuotaInsufficient,
                "可用连接时长不足，请签到后刷新再重试。",
            )
        } else if message.contains("密码") || message.contains("认证失败") {
            if login {
                Self::new(ErrorCode::LoginFailed, "皎月连账号或密码不正确。")
            } else {
                Self::new(
                    ErrorCode::PasswordRejected,
                    "连接密码或授权已失效，请更新密码。",
                )
            }
        } else if message.contains("离线")
            || message.contains("识别码不存在")
            || message.contains("未找到")
        {
            Self::new(
                ErrorCode::Offline,
                "未找到在线设备，请确认识别码和对方服务状态。",
            )
        } else if message.contains("未开启") || message.contains("未启动") {
            Self::new(ErrorCode::Offline, "对方尚未开启皎月连组网服务。")
        } else if login {
            Self::new(
                ErrorCode::LoginFailed,
                "皎月连登录失败，请检查账号或打开高级设置。",
            )
        } else {
            Self::new(
                ErrorCode::NetworkFailed,
                "皎月连连接失败，请检查网络和对方状态。",
            )
        }
    }
    pub fn server_info(message: &str) -> Self {
        if message.contains("网卡") || message.to_lowercase().contains("wintun") {
            Self::new(
                ErrorCode::VirtualAdapterFailed,
                "皎月连 Wintun 网卡初始化失败，请查看连接诊断中的 Windows 原始错误。",
            )
        } else if message.contains("访问密码")
            || message.to_lowercase().contains("web") && message.contains("密码")
        {
            Self::new(
                ErrorCode::InvalidInput,
                "皎月连组网页面访问密码需要 6–20 位，请在本机接受控制设置中填写。",
            )
        } else if message.contains("密码") || message.contains("认证") {
            Self::new(
                ErrorCode::InvalidInput,
                "皎月连未接受本机密码设置，请核对连接密码和组网页面访问密码。",
            )
        } else if message.contains("权限")
            || message.contains("拒绝访问")
            || message.to_lowercase().contains("access is denied")
        {
            Self::new(
                ErrorCode::PermissionDenied,
                "皎月连组网初始化权限不足，请在高级设置中完成驱动初始化后重试。",
            )
        } else {
            Self::native_info(message, false)
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AppError {}
impl From<std::io::Error> for AppError {
    fn from(_: std::io::Error) -> Self {
        Self::new(
            ErrorCode::StorageFailed,
            "无法读取或写入本机数据，请检查目录权限。",
        )
    }
}
impl From<serde_json::Error> for AppError {
    fn from(_: serde_json::Error) -> Self {
        Self::new(
            ErrorCode::StorageFailed,
            "配置文件格式有误，请在诊断页检查备份。",
        )
    }
}
