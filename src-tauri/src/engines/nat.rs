use crate::{
    error::{AppError, ErrorCode, Result},
    model::{now, Account, Device, LocalServerPasswords},
    storage::SecretRecord,
};
use futures_util::{SinkExt, StreamExt};
use sha2::{Digest, Sha256};
use std::{net::IpAddr, time::Duration};
use tokio::{
    net::TcpStream,
    sync::Mutex,
    time::{timeout, Instant},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub const SEPARATOR: &str = "<$!$>";
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(90);
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(20);
const VPN_CONFIGURATION_TIMEOUT: Duration = Duration::from_secs(8);
const STOPPED_START_REPLY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
enum ServerPhase {
    VpnConfiguration,
    AdapterInitialization,
    ServiceReady,
    Stopping,
}
impl ServerPhase {
    fn label(self) -> &'static str {
        match self {
            Self::VpnConfiguration => "VPN 模式配置",
            Self::AdapterInitialization => "Wintun 初始化",
            Self::ServiceReady => "服务就绪",
            Self::Stopping => "停止服务",
        }
    }
    fn timeout_error(self) -> AppError {
        AppError::new(
            ErrorCode::Timeout,
            format!(
                "皎月连{}等待超时，请在连接诊断中查看网络宿主日志。",
                self.label()
            ),
        )
    }
    fn trace(self, message_type: &str, started: Instant) {
        tracing::info!(
            phase = self.label(),
            message_type,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Jiaoyuelian server operation"
        );
    }
}

fn server_message_type(frame: &str) -> &'static str {
    let mut fields = frame.split(SEPARATOR);
    match fields.next() {
        Some("0") => "0",
        Some("1") => "1",
        Some("2") => "2",
        Some("info") => "info",
        Some("loading") => match fields.next() {
            Some("show") => "loading/show",
            Some("hied") => "loading/hied",
            Some("hide") => "loading/hide",
            _ => "loading",
        },
        Some("pclist") => "pclist",
        Some("authorize") => "authorize",
        Some("vipend") => "vipend",
        _ => "other",
    }
}

fn server_frame_error(frame: &str, enabled: bool) -> Option<AppError> {
    let fields: Vec<_> = frame.split(SEPARATOR).collect();
    match fields.first().copied() {
        Some("info") => fields
            .get(1)
            .filter(|message| {
                !message.is_empty()
                    && (!message.contains("成功")
                        || message.contains("失败")
                        || message.contains("错误"))
            })
            .map(|message| AppError::server_info(message)),
        Some("1") if enabled => fields
            .get(10)
            .filter(|message| !message.is_empty())
            .map(|message| AppError::server_info(message)),
        Some("0") => Some(AppError::new(
            ErrorCode::LoginRequired,
            "皎月连登录状态已失效，请重新登录。",
        )),
        Some("authorize") => Some(AppError::new(
            ErrorCode::LocalAuthorizationRequired,
            "皎月连本地管理接口需要访问密码验证。",
        )),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct NatHost {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub service_ready: Option<bool>,
    pub online: bool,
}
#[derive(Clone, Debug, Default)]
pub struct NetConnection {
    pub peer_id: Option<String>,
    pub code: Option<String>,
    pub local_address: Option<String>,
    pub remote_address: Option<String>,
}
impl NetConnection {
    pub fn matches_device(&self, device: &Device) -> bool {
        let equal = |left: &Option<String>, right: &Option<String>| {
            left.as_ref()
                .zip(right.as_ref())
                .is_some_and(|(left, right)| {
                    !left.trim().is_empty() && left.trim().eq_ignore_ascii_case(right.trim())
                })
        };
        equal(&self.peer_id, &device.nat_id) || equal(&self.code, &device.peer_code)
    }
}

fn interface_address(value: &str) -> Option<String> {
    let address = value.trim().parse::<IpAddr>().ok()?;
    if address.is_unspecified()
        || address.is_loopback()
        || address.is_multicast()
        || matches!(address, IpAddr::V4(ip) if ip.is_broadcast())
    {
        return None;
    }
    Some(address.to_string())
}
fn vpn_address(mapping: &str) -> Option<String> {
    let values: Vec<&str> = mapping.split(['|', ';']).collect();
    if values.len() >= 5 && values[2].trim() == "all" && values[3].trim() == "all" {
        interface_address(values[1])
    } else {
        None
    }
}
fn vpn_enabled(mapping: &str) -> bool {
    let values: Vec<&str> = mapping.split(['|', ';']).collect();
    values.len() >= 5 && values[2].trim() == "all" && values[3].trim() == "all"
}
fn vpn_web_password(mapping: &str) -> &str {
    if vpn_enabled(mapping) {
        mapping.rsplit(['|', ';']).next().unwrap_or("")
    } else {
        ""
    }
}
fn validate_field(value: &str) -> Result<()> {
    if value.len() > 2048
        || value.contains(SEPARATOR)
        || value.contains('\0')
        || value.contains(['\r', '\n'])
    {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "输入包含当前皎月连版本不支持的字符或长度。",
        ));
    }
    Ok(())
}

struct Channel {
    socket: Option<Socket>,
    account: Account,
    hosts: Vec<NatHost>,
    connection: Option<NetConnection>,
    owned_connection: bool,
    server_password: Zeroizing<String>,
    server_max_clients: String,
    server_vpn: bool,
    server_vpn_address: Option<String>,
    server_web_password: Zeroizing<String>,
    server_start_pending: bool,
    local_authorization_required: bool,
    login_secret: Option<SecretRecord>,
}
impl Channel {
    fn disconnect_socket(&mut self) {
        self.socket = None;
        self.account.authenticated = false;
        self.account.server_running = false;
        self.account.protocol_supported = false;
        self.local_authorization_required = false;
        self.connection = None;
        self.owned_connection = false;
        self.server_vpn_address = None;
        self.update_local_vpn_address();
    }
    fn update_local_vpn_address(&mut self) {
        self.account.local_vpn_address = if !self.account.authenticated {
            None
        } else if self.account.server_running {
            self.server_vpn_address.clone()
        } else {
            self.connection
                .as_ref()
                .and_then(|connection| connection.local_address.clone())
        };
    }
    fn apply(&mut self, frame: &str) -> Result<()> {
        if frame.len() > 1024 * 1024 {
            return Err(AppError::new(
                ErrorCode::ProtocolUnsupported,
                "皎月连返回的数据超过当前版本允许的大小。",
            ));
        }
        let fields: Vec<&str> = frame.split(SEPARATOR).collect();
        let field = |index: usize| fields.get(index).copied().unwrap_or("");
        match field(0) {
            "authorize" => {
                self.local_authorization_required = true;
                self.account.authenticated = false;
            }
            "0" => {
                self.local_authorization_required = false;
                self.account.authenticated = false;
                self.account.server_running = false;
                self.connection = None;
                self.hosts.clear();
                self.server_vpn_address = None;
            }
            "1" | "2" => {
                self.local_authorization_required = false;
                self.account.authenticated = true;
                self.account.username = Some(field(1).into());
                self.account.local_code = Some(field(3).into());
                self.account.server_running = field(0) == "2";
                self.account.local_name = Some(field(if field(0) == "2" { 10 } else { 8 }).into());
                self.server_password =
                    Zeroizing::new(field(if field(0) == "2" { 7 } else { 5 }).into());
                self.server_max_clients = field(if field(0) == "2" { 8 } else { 6 }).into();
                self.server_vpn = vpn_enabled(field(4));
                self.server_vpn_address = vpn_address(field(4));
                self.server_web_password = Zeroizing::new(vpn_web_password(field(4)).into());
                self.account.updated_at = Some(now());
            }
            "pclist" if fields.len() >= 6 => {
                let end = fields.len() - 5;
                let previous = std::mem::take(&mut self.hosts);
                let mut hosts = vec![];
                for values in fields[1..end].chunks_exact(4) {
                    if values[0].is_empty() || values[0] == "me" {
                        continue;
                    }
                    // Empty list metadata means "not loaded" in the SDK UI;
                    // retain the last explicit pcinfo result for this peer.
                    let known = previous.iter().find(|host| host.id == values[0]);
                    let address = if values[3].is_empty() {
                        known.and_then(|host| host.address.clone())
                    } else {
                        vpn_address(values[3])
                    };
                    let service_ready = if values[3].is_empty() {
                        known.and_then(|host| host.service_ready)
                    } else {
                        Some(address.is_some())
                    };
                    hosts.push(NatHost {
                        id: values[0].into(),
                        name: values[1].into(),
                        address,
                        service_ready,
                        online: true,
                    });
                }
                self.hosts = hosts;
                let mode = field(fields.len() - 2);
                if !mode.is_empty() {
                    let peer_id = (mode == "1" && !field(fields.len() - 1).is_empty())
                        .then(|| field(fields.len() - 1).to_string());
                    let remote_address = peer_id
                        .as_ref()
                        .and_then(|id| self.hosts.iter().find(|host| &host.id == id))
                        .and_then(|host| host.address.clone());
                    self.connection = Some(NetConnection {
                        peer_id,
                        code: (mode != "1").then(|| field(fields.len() - 4).into()),
                        local_address: interface_address(field(fields.len() - 5)),
                        remote_address,
                    });
                } else {
                    self.connection = None;
                }
            }
            "pcinfo" if fields.len() >= 2 => {
                let online = fields.len() >= 5;
                let address = online.then(|| vpn_address(field(4))).flatten();
                if let Some(host) = self.hosts.iter_mut().find(|host| host.id == field(1)) {
                    if online {
                        host.name = field(3).into();
                    }
                    host.address = address.clone();
                    host.service_ready = Some(address.is_some());
                    host.online = online;
                } else if online && !field(1).is_empty() {
                    self.hosts.push(NatHost {
                        id: field(1).into(),
                        name: field(3).into(),
                        address: address.clone(),
                        service_ready: Some(address.is_some()),
                        online: true,
                    });
                }
                if let Some(connection) = &mut self.connection {
                    if connection.peer_id.as_deref() == Some(field(1)) {
                        connection.remote_address = address;
                    }
                }
            }
            "clientadd" => {
                if !self.hosts.iter().any(|host| host.id == field(1)) {
                    self.hosts.push(NatHost {
                        id: field(1).into(),
                        name: field(2).into(),
                        address: None,
                        service_ready: None,
                        online: true,
                    });
                }
            }
            "clientdel" => {
                if let Some(host) = self.hosts.iter_mut().find(|host| host.id == field(1)) {
                    host.online = false;
                    host.address = None;
                    host.service_ready = Some(false);
                }
                if let Some(connection) = &mut self.connection {
                    if connection.peer_id.as_deref() == Some(field(1)) {
                        connection.remote_address = None;
                    }
                }
            }
            "discon" => {
                self.connection = None;
                self.owned_connection = false;
            }
            "vipend" => {
                self.account.quota_state = "expired".into();
                return Err(AppError::new(
                    ErrorCode::ServiceExpired,
                    "皎月连服务已到期，请签到或在官网检查套餐。",
                ));
            }
            _ => {}
        }
        self.update_local_vpn_address();
        Ok(())
    }
    async fn receive(&mut self, cancel: &CancellationToken, deadline: Instant) -> Result<String> {
        loop {
            let socket = self
                .socket
                .as_mut()
                .ok_or_else(|| AppError::new(ErrorCode::NetworkFailed, "皎月连本地接口已断开。"))?;
            let message = tokio::select! { _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Canceled, "连接已取消。")), value = tokio::time::timeout_at(deadline, socket.next()) => value.map_err(|_| AppError::new(ErrorCode::Timeout, "皎月连接口响应超时。"))? };
            match message {
                Some(Ok(Message::Text(text))) => {
                    let text = text.to_string();
                    self.apply(&text)?;
                    return Ok(text);
                }
                Some(Ok(Message::Ping(data))) => {
                    socket.send(Message::Pong(data)).await.map_err(|_| {
                        AppError::new(ErrorCode::NetworkFailed, "皎月连本地连接中断。")
                    })?;
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => {
                    self.disconnect_socket();
                    return Err(AppError::new(
                        ErrorCode::NetworkFailed,
                        "皎月连本地接口已断开，请重新准备网络。",
                    ));
                }
                _ => {}
            }
        }
    }
    async fn send(&mut self, fields: &[&str]) -> Result<()> {
        if self.local_authorization_required && fields.first() != Some(&"authorize") {
            return Err(AppError::new(
                ErrorCode::LocalAuthorizationRequired,
                "皎月连本地管理接口需要访问密码，请先验证后登录。",
            ));
        }
        for field in fields {
            validate_field(field)?;
        }
        let message = Zeroizing::new(fields.join(SEPARATOR));
        let result = self.socket
            .as_mut()
            .ok_or_else(|| AppError::new(ErrorCode::NetworkFailed, "皎月连本地接口未连接。"))?
            .send(Message::Text(message.as_str().into()))
            .await;
        if result.is_err() {
            self.disconnect_socket();
            return Err(AppError::new(ErrorCode::NetworkFailed, "无法发送皎月连请求。"));
        }
        Ok(())
    }
    async fn send_server(
        &mut self,
        fields: &[&str],
        phase: ServerPhase,
        started: Instant,
        deadline: Instant,
    ) -> Result<()> {
        let message_type = match fields.first().copied() {
            Some("VPN") => "VPN",
            Some("startServer") => "startServer",
            Some("stopServer") => "stopServer",
            _ => "other",
        };
        phase.trace(message_type, started);
        tokio::time::timeout_at(deadline, self.send(fields))
            .await
            .map_err(|_| phase.timeout_error())?
    }
    async fn receive_server(
        &mut self,
        cancel: &CancellationToken,
        phase: ServerPhase,
        started: Instant,
        deadline: Instant,
    ) -> Result<String> {
        match self.receive(cancel, deadline).await {
            Ok(frame) => {
                phase.trace(server_message_type(&frame), started);
                Ok(frame)
            }
            Err(error) if error.code == ErrorCode::Timeout => {
                phase.trace("timeout", started);
                Err(phase.timeout_error())
            }
            Err(error) => {
                phase.trace("error", started);
                Err(error)
            }
        }
    }
    async fn drain(&mut self) {
        for _ in 0..32 {
            let cancel = CancellationToken::new();
            if self
                .receive(&cancel, Instant::now() + Duration::from_millis(15))
                .await
                .is_err()
            {
                break;
            }
        }
    }
    async fn refresh_hosts(&mut self) -> Result<()> {
        if !self.account.authenticated {
            return Ok(());
        }
        self.send(&["pclist", ""]).await?;
        let cancel = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let text = self.receive(&cancel, deadline).await?;
            if text.starts_with("pclist<$!$>") {
                return Ok(());
            }
            if !self.account.authenticated {
                return Err(AppError::new(
                    ErrorCode::LoginRequired,
                    "皎月连登录状态已失效。",
                ));
            }
        }
    }
    async fn disconnect(&mut self) -> Result<()> {
        if self.connection.is_none() && !self.owned_connection {
            return Ok(());
        }
        self.send(&["stopcon", ""]).await?;
        self.send(&["pclist", ""]).await?;
        let cancel = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let text = self.receive(&cancel, deadline).await?;
            let fields: Vec<&str> = text.split(SEPARATOR).collect();
            match fields.first().copied() {
                Some("discon" | "0") => {
                    self.owned_connection = false;
                    return Ok(());
                }
                Some("pclist") => {
                    if self.connection.is_none() {
                        self.owned_connection = false;
                        return Ok(());
                    }
                    // The first list can arrive before the SDK has removed its connection.
                    tokio::time::sleep_until(
                        (Instant::now() + Duration::from_millis(250)).min(deadline),
                    )
                    .await;
                    if Instant::now() >= deadline {
                        return Err(AppError::new(
                            ErrorCode::Timeout,
                            "皎月连尚未确认连接已断开。",
                        ));
                    }
                    self.send(&["pclist", ""]).await?;
                }
                Some("info")
                    if !fields
                        .get(1)
                        .is_some_and(|message| message.contains("成功")) =>
                {
                    return Err(AppError::native_info(
                        fields.get(1).copied().unwrap_or(""),
                        false,
                    ));
                }
                _ => {}
            }
        }
    }
}

pub struct NatAdapter {
    port: u16,
    expected_script: String,
    local_name: Option<String>,
    channel: Mutex<Channel>,
}
impl NatAdapter {
    pub fn new(port: u16, expected_script: String) -> Self {
        Self {
            port,
            expected_script,
            local_name: None,
            channel: Mutex::new(Channel {
                socket: None,
                account: Account {
                    quota_state: "unknown".into(),
                    ..Account::default()
                },
                hosts: vec![],
                connection: None,
                owned_connection: false,
                server_password: Zeroizing::new(String::new()),
                server_max_clients: "3".into(),
                server_vpn: false,
                server_vpn_address: None,
                server_web_password: Zeroizing::new(String::new()),
                server_start_pending: false,
                local_authorization_required: false,
                login_secret: None,
            }),
        }
    }
    pub fn with_local_name(mut self, name: Option<String>) -> Self {
        self.channel.get_mut().account.local_name = name.clone();
        self.local_name = name;
        self
    }
    async fn sync_local_name(&self, channel: &mut Channel) {
        if let Some(name) = self.local_name.as_deref() {
            if channel.account.authenticated && channel.account.local_name.as_deref() != Some(name) {
                match channel.send(&["newpcname", name, ""]).await {
                    Ok(()) => channel.account.local_name = Some(name.into()),
                    Err(error) => tracing::debug!(code = ?error.code, "Could not update the Jiaoyuelian device name"),
                }
            }
        }
    }
    async fn open(&self, channel: &mut Channel) -> Result<()> {
        if channel.socket.is_some() {
            if channel.local_authorization_required {
                return Err(AppError::new(
                    ErrorCode::LocalAuthorizationRequired,
                    "皎月连本地管理接口需要访问密码，请先验证后登录。",
                ));
            }
            return Ok(());
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(4))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AppError::new(ErrorCode::NetworkFailed, "无法创建本机接口连接。"))?;
        let script = client
            .get(format!("http://127.0.0.1:{}/web/script.js", self.port))
            .send()
            .await
            .map_err(|_| AppError::new(ErrorCode::NetworkFailed, "皎月连本地接口尚未就绪。"))?
            .bytes()
            .await
            .map_err(|_| AppError::new(ErrorCode::NetworkFailed, "无法验证皎月连版本。"))?;
        let actual = format!("{:x}", Sha256::digest(&script));
        if self.expected_script.is_empty() || actual != self.expected_script {
            return Err(AppError::new(
                ErrorCode::ProtocolUnsupported,
                "当前皎月连内核版本与适配清单不一致，请使用高级设置或匹配版本的完整安装包。",
            ));
        }
        let url = format!("ws://127.0.0.1:{}/ws", self.port);
        let mut request = url
            .into_client_request()
            .map_err(|_| AppError::new(ErrorCode::ProtocolUnsupported, "本地接口地址无效。"))?;
        request.headers_mut().insert(
            "Origin",
            format!("http://127.0.0.1:{}", self.port).parse().unwrap(),
        );
        let (socket, _) = timeout(Duration::from_secs(5), connect_async(request))
            .await
            .map_err(|_| AppError::new(ErrorCode::Timeout, "皎月连本地接口握手超时。"))?
            .map_err(|_| AppError::new(ErrorCode::NetworkFailed, "无法连接皎月连本地接口。"))?;
        channel.socket = Some(socket);
        channel.account.protocol_supported = true;
        let cancel = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let text = match channel.receive(&cancel, deadline).await {
                Ok(text) => text,
                Err(error) => {
                    channel.socket = None;
                    channel.account.protocol_supported = false;
                    return Err(if error.code == ErrorCode::Timeout {
                        AppError::new(
                            ErrorCode::Timeout,
                            "皎月连本地接口未返回本机状态，上次服务操作可能尚未完成，请重试或在高级设置中检查服务。",
                        )
                    } else {
                        error
                    });
                }
            };
            match text.split(SEPARATOR).next() {
                Some("0" | "1" | "2") => return Ok(()),
                Some("authorize") => {
                    return Err(AppError::new(
                        ErrorCode::LocalAuthorizationRequired,
                        "皎月连本地管理接口需要访问密码。",
                    ))
                }
                _ => {}
            }
        }
    }
    pub async fn snapshot(&self) -> Result<(Account, Vec<NatHost>)> {
        let mut channel = self.channel.lock().await;
        self.open(&mut channel).await?;
        channel.drain().await;
        self.open(&mut channel).await?;
        self.sync_local_name(&mut channel).await;
        channel.refresh_hosts().await?;
        Ok((channel.account.clone(), channel.hosts.clone()))
    }
    pub async fn cached_account(&self) -> Account {
        self.channel.lock().await.account.clone()
    }
    pub async fn cached_state(&self) -> (Account, Option<NetConnection>) {
        let channel = self.channel.lock().await;
        (channel.account.clone(), channel.connection.clone())
    }
    pub async fn cached_connection(&self) -> Option<NetConnection> {
        self.channel.lock().await.connection.clone()
    }
    pub async fn login(
        &self,
        username: &str,
        password: &str,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        let mut channel = self.channel.lock().await;
        self.open(&mut channel).await?;
        channel.drain().await;
        // The original native save flags are disabled. LL remembers secrets with DPAPI.
        channel
            .send(&["login", username, password, "0", "0"])
            .await?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let text = channel.receive(cancel, deadline).await?;
            let fields: Vec<&str> = text.split(SEPARATOR).collect();
            match fields[0] {
                "1" | "2" => {
                    channel.login_secret = Some(SecretRecord {
                        username: username.into(),
                        password: password.into(),
                    });
                    self.sync_local_name(&mut channel).await;
                    return Ok(channel.account.clone());
                }
                "authorize" => {
                    return Err(AppError::new(
                        ErrorCode::LocalAuthorizationRequired,
                        "皎月连本地管理接口需要访问密码，请先验证后登录。",
                    ))
                }
                "y?" => {
                    return Err(AppError::new(
                        ErrorCode::AccountInUse,
                        "此账号已在另一服务端登录，请在原界面确认账号使用方式。",
                    ))
                }
                "info" => {
                    return Err(AppError::native_info(
                        fields.get(1).copied().unwrap_or(""),
                        true,
                    ))
                }
                "0" if fields.get(5).is_some_and(|s| !s.is_empty()) => {
                    return Err(AppError::native_info(fields[5], true))
                }
                _ => {}
            }
        }
    }
    pub async fn authorize(&self, password: &str) -> Result<()> {
        let mut channel = self.channel.lock().await;
        if channel.socket.is_none() {
            let _ = self.open(&mut channel).await;
        }
        channel.send(&["authorize", password]).await?;
        let cancel = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let text = channel.receive(&cancel, deadline).await?;
            match text.split(SEPARATOR).next() {
                Some("0" | "1" | "2") => return Ok(()),
                Some("authorize" | "info") => {
                    return Err(AppError::new(
                        ErrorCode::LocalAuthorizationRequired,
                        "本地管理访问密码验证失败。",
                    ))
                }
                _ => {}
            }
        }
    }
    pub async fn logout(&self) -> Result<()> {
        let mut channel = self.channel.lock().await;
        self.open(&mut channel).await?;
        channel.send(&["exit"]).await?;
        channel.login_secret = None;
        let cancel = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if channel
                .receive(&cancel, deadline)
                .await?
                .split(SEPARATOR)
                .next()
                == Some("0")
            {
                channel.account.username = None;
                return Ok(());
            }
        }
    }
    pub async fn host_info(&self, id: &str, cancel: &CancellationToken) -> Result<NatHost> {
        let mut channel = self.channel.lock().await;
        self.open(&mut channel).await?;
        channel.drain().await;
        channel.send(&["pcinfo", id]).await?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let text = channel.receive(cancel, deadline).await?;
            let fields: Vec<&str> = text.split(SEPARATOR).collect();
            if fields.first() == Some(&"pcinfo") && fields.get(1) == Some(&id) {
                if fields.len() < 5 {
                    return Err(AppError::new(ErrorCode::Offline, "对方设备已离线。"));
                }
                return Ok(NatHost {
                    id: id.into(),
                    name: fields[3].into(),
                    address: vpn_address(fields[4]),
                    service_ready: Some(vpn_address(fields[4]).is_some()),
                    online: true,
                });
            }
        }
    }
    pub async fn connect(
        &self,
        id: Option<&str>,
        code: Option<&str>,
        password: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<NetConnection> {
        let mut channel = self.channel.lock().await;
        self.open(&mut channel).await?;
        channel.drain().await;
        if !channel.account.authenticated {
            return Err(AppError::new(ErrorCode::LoginRequired, "请先登录皎月连。"));
        }
        if let Some(connection) = &channel.connection {
            if (id.is_some() && connection.peer_id.as_deref() == id)
                || (code.is_some() && connection.code.as_deref() == code)
            {
                return Ok(connection.clone());
            }
            if !channel.owned_connection {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "皎月连已有另一条组网连接，请先在原界面断开该连接。",
                ));
            }
            channel.disconnect().await?;
        }
        if let Some(code) = code {
            channel
                .send(&["concode", code, password.unwrap_or("")])
                .await?;
        } else if let Some(id) = id {
            channel.send(&["conpc", id]).await?;
        } else {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "请选择设备或填写识别码。",
            ));
        }
        channel.owned_connection = true;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let text = channel.receive(cancel, deadline).await?;
            let fields: Vec<&str> = text.split(SEPARATOR).collect();
            match fields[0] {
                "conpc" if fields.get(1) == Some(&"yes") => {
                    let peer_id = fields
                        .get(2)
                        .filter(|id| !id.is_empty())
                        .map(|id| id.to_string())
                        .or_else(|| id.filter(|_| code.is_none()).map(str::to_owned));
                    let remote_address = peer_id
                        .as_ref()
                        .and_then(|id| channel.hosts.iter().find(|host| &host.id == id))
                        .and_then(|host| host.address.clone());
                    let connection = NetConnection {
                        peer_id,
                        code: code.map(str::to_owned),
                        local_address: fields.get(3).and_then(|address| interface_address(address)),
                        remote_address,
                    };
                    channel.connection = Some(connection.clone());
                    channel.update_local_vpn_address();
                    return Ok(connection);
                }
                "conpc" | "info" => {
                    return Err(AppError::native_info(
                        fields
                            .get(if fields[0] == "conpc" { 2 } else { 1 })
                            .copied()
                            .unwrap_or(""),
                        false,
                    ))
                }
                "0" => {
                    return Err(AppError::new(
                        ErrorCode::LoginRequired,
                        "皎月连登录状态已失效，请重新登录。",
                    ))
                }
                _ => {}
            }
        }
    }
    pub async fn cancel_owned_connection(&self) {
        let mut channel = self.channel.lock().await;
        if channel.owned_connection {
            let _ = channel.disconnect().await;
        }
    }
    pub async fn end_session_connection(&self, device: Option<&Device>) -> Result<()> {
        let mut channel = self.channel.lock().await;
        if !channel.owned_connection && device.is_none() {
            return Ok(());
        }
        self.open(&mut channel).await?;
        channel.drain().await;
        if channel.owned_connection
            || device.is_some_and(|device| {
                channel
                    .connection
                    .as_ref()
                    .is_some_and(|connection| connection.matches_device(device))
            })
        {
            channel.disconnect().await?;
        }
        Ok(())
    }
    pub async fn disconnect_current(&self) -> Result<()> {
        let mut channel = self.channel.lock().await;
        self.open(&mut channel).await?;
        channel.drain().await;
        channel.refresh_hosts().await?;
        channel.disconnect().await
    }
    pub async fn disconnect_device(&self, device: &Device) -> Result<()> {
        let mut channel = self.channel.lock().await;
        self.open(&mut channel).await?;
        channel.drain().await;
        channel.refresh_hosts().await?;
        if channel.socket.is_none() {
            return Err(AppError::new(
                ErrorCode::NetworkFailed,
                "皎月连管理接口已断开，暂时无法确认网络连接已停止。",
            ));
        }
        if channel
            .connection
            .as_ref()
            .is_some_and(|connection| connection.matches_device(device))
        {
            channel.disconnect().await?;
        }
        Ok(())
    }
    pub async fn set_server(
        &self,
        enabled: bool,
        password: &str,
        web_password: &str,
    ) -> Result<()> {
        let mut channel = self.channel.lock().await;
        self.open(&mut channel).await?;
        channel.drain().await;
        if enabled {
            self.sync_local_name(&mut channel).await;
        }
        if enabled && channel.server_start_pending {
            Self::set_server_on_channel(&mut channel, false, "", "").await?;
        }
        let result =
            Self::set_server_on_channel(&mut channel, enabled, password, web_password).await;
        if let Err(error) = &result {
            if enabled && channel.server_start_pending {
                channel.drain().await;
                if let Err(cleanup_error) =
                    Self::set_server_on_channel(&mut channel, false, "", "").await
                {
                    tracing::warn!(code = ?cleanup_error.code, "Could not confirm cleanup of failed Jiaoyuelian server startup");
                    return Err(AppError::new(
                        error.code.clone(),
                        format!(
                            "{} 本次启动的停止尚未确认，再次开启前会先重试清理。",
                            error.message
                        ),
                    ));
                }
            }
        }
        result
    }
    async fn set_server_on_channel(
        channel: &mut Channel,
        enabled: bool,
        password: &str,
        web_password: &str,
    ) -> Result<()> {
        if !channel.account.authenticated {
            return Err(AppError::new(ErrorCode::LoginRequired, "请先登录皎月连。"));
        }
        if enabled == channel.account.server_running && !channel.server_start_pending {
            return Ok(());
        }
        if enabled && channel.connection.is_some() && !channel.account.server_running {
            return Err(AppError::new(
                ErrorCode::Busy,
                "本机正在连接对方的皎月连组网，请先断开该连接，再开启本机接受控制。",
            ));
        }
        let started = Instant::now();
        let total_deadline = started
            + if enabled {
                SERVER_START_TIMEOUT
            } else {
                SERVER_READY_TIMEOUT
            };
        let initialization_deadline = total_deadline - SERVER_READY_TIMEOUT;
        let mut phase = if enabled {
            ServerPhase::AdapterInitialization
        } else {
            ServerPhase::Stopping
        };
        let mut deadline = if enabled {
            initialization_deadline
        } else {
            total_deadline
        };
        if enabled {
            let secret = if password.is_empty() {
                channel.server_password.to_string()
            } else {
                password.to_owned()
            };
            let secret = Zeroizing::new(secret);
            if secret.is_empty() {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "首次开启接受控制需要设置本机连接密码。",
                ));
            }
            validate_field(&secret)?;
            let access_password = Zeroizing::new(if !web_password.is_empty() {
                web_password.to_owned()
            } else if !channel.server_web_password.is_empty() {
                channel.server_web_password.to_string()
            } else {
                secret.to_string()
            });
            if !(6..=20).contains(&access_password.len()) {
                return Err(AppError::new(ErrorCode::InvalidInput, "组网页面访问密码需要 6–20 位。可单独填写；留空时使用已有页面密码或本机连接密码。"));
            }
            validate_field(&access_password)?;
            // Keep a validated draft when driver initialization fails.
            channel.server_password = Zeroizing::new(secret.to_string());
            channel.server_web_password = Zeroizing::new(access_password.to_string());
            // Only change to VPN on a newly stopped service, never overwrite running legacy mappings.
            if !channel.server_vpn {
                let configuration_deadline =
                    (Instant::now() + VPN_CONFIGURATION_TIMEOUT).min(initialization_deadline);
                channel
                    .send_server(
                        &["VPN", "1"],
                        ServerPhase::VpnConfiguration,
                        started,
                        configuration_deadline,
                    )
                    .await?;
                let cancel = CancellationToken::new();
                loop {
                    let text = channel
                        .receive_server(
                            &cancel,
                            ServerPhase::VpnConfiguration,
                            started,
                            configuration_deadline,
                        )
                        .await?;
                    if let Some(error) = server_frame_error(&text, true) {
                        return Err(error);
                    }
                    if channel.server_vpn {
                        break;
                    }
                }
            }
            if channel.account.server_running {
                return Ok(());
            }
            let max_clients = channel.server_max_clients.clone();
            channel.server_start_pending = true;
            channel
                .send_server(
                    &[
                        "startServer",
                        &secret,
                        if max_clients.is_empty() {
                            "3"
                        } else {
                            &max_clients
                        },
                        &access_password,
                        "",
                    ],
                    phase,
                    started,
                    deadline,
                )
                .await?;
        } else {
            channel
                .send_server(&["stopServer"], phase, started, deadline)
                .await?;
        }
        let cancel = CancellationToken::new();
        let mut stopped_reply = false;
        loop {
            let text = match channel
                .receive_server(&cancel, phase, started, deadline)
                .await
            {
                Err(error) if enabled && stopped_reply && error.code == ErrorCode::Timeout => {
                    return Err(AppError::new(
                        ErrorCode::NetworkFailed,
                        "皎月连返回服务未启动状态，未完成本机接受控制启动，请重试。",
                    ));
                }
                result => result?,
            };
            if let Some(error) = server_frame_error(&text, enabled) {
                return Err(error);
            }
            let mut fields = text.split(SEPARATOR);
            match fields.next() {
                Some("2") if enabled => {
                    channel.server_start_pending = false;
                    return Ok(());
                }
                Some("1") if !enabled => {
                    channel.server_start_pending = false;
                    return Ok(());
                }
                Some("1") if enabled => {
                    stopped_reply = true;
                    deadline = deadline.min(Instant::now() + STOPPED_START_REPLY_TIMEOUT);
                }
                Some("loading") if enabled => match fields.next() {
                    Some("show") => {
                        stopped_reply = false;
                        phase = ServerPhase::AdapterInitialization;
                        deadline = initialization_deadline;
                    }
                    Some("hied" | "hide")
                        if matches!(phase, ServerPhase::AdapterInitialization) =>
                    {
                        stopped_reply = false;
                        phase = ServerPhase::ServiceReady;
                        deadline = (Instant::now() + SERVER_READY_TIMEOUT).min(total_deadline);
                        phase.trace(server_message_type(&text), started);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    pub async fn local_server_passwords(&self) -> LocalServerPasswords {
        let mut channel = self.channel.lock().await;
        if self.open(&mut channel).await.is_ok() {
            channel.drain().await;
        }
        LocalServerPasswords {
            connection_password: channel.server_password.to_string(),
            web_password: channel.server_web_password.to_string(),
        }
    }
    pub async fn remember_server_passwords(&self, password: &str, web_password: &str) {
        let mut channel = self.channel.lock().await;
        if !password.is_empty() {
            channel.server_password = Zeroizing::new(password.into());
        }
        if !web_password.is_empty() {
            channel.server_web_password = Zeroizing::new(web_password.into());
        }
    }
    pub async fn restart_login_secret(&self) -> Option<SecretRecord> {
        let channel = self.channel.lock().await;
        channel.login_secret.as_ref().map(|secret| SecretRecord {
            username: secret.username.clone(),
            password: secret.password.clone(),
        })
    }
    pub async fn disconnect_for_restart(&self) {
        self.channel.lock().await.disconnect_socket();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture_socket(
        listener: tokio::net::TcpListener,
        script: &str,
    ) -> WebSocketStream<TcpStream> {
        fixture_socket_with_state(listener, script, true).await
    }
    async fn fixture_socket_with_state(
        listener: tokio::net::TcpListener,
        script: &str,
        send_initial_state: bool,
    ) -> WebSocketStream<TcpStream> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut http, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            http.read_exact(&mut byte).await.unwrap();
            request.push(byte[0]);
        }
        http.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{script}",
                script.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        drop(http);
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        if send_initial_state {
            socket
                .send(Message::Text(
                    ["1", "fixture", "0", "12345678", "", "", "3", "", "Fixture"]
                        .join(SEPARATOR)
                        .into(),
                ))
                .await
                .unwrap();
        }
        socket
    }
    async fn server_fixture(vpn: bool) -> (std::sync::Arc<NatAdapter>, WebSocketStream<TcpStream>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = "server startup phase fixture";
        let worker = tokio::spawn(async move {
            let mut socket = fixture_socket(listener, script).await;
            if vpn {
                socket
                    .send(Message::Text(
                        [
                            "1",
                            "fixture",
                            "0",
                            "12345678",
                            "VPN|10.6.22.1|all|all|all|fixture-web",
                            "",
                            "3",
                            "",
                            "Fixture",
                            "0",
                            "",
                        ]
                        .join(SEPARATOR)
                        .into(),
                    ))
                    .await
                    .unwrap();
            }
            socket
        });
        let adapter = std::sync::Arc::new(NatAdapter::new(
            port,
            format!("{:x}", Sha256::digest(script.as_bytes())),
        ));
        {
            let mut channel = adapter.channel.lock().await;
            adapter.open(&mut channel).await.unwrap();
            channel.drain().await;
        }
        (adapter, worker.await.unwrap())
    }
    async fn advance_server_time(duration: Duration) {
        tokio::time::pause();
        tokio::time::advance(duration).await;
        tokio::task::yield_now().await;
        tokio::time::resume();
    }

    fn stopped_server_frame() -> Message {
        Message::Text(
            [
                "1",
                "fixture",
                "0",
                "12345678",
                "VPN|10.6.22.1|all|all|all|fixture-web",
                "fixture-password",
                "3",
                "",
                "Fixture",
                "0",
                "",
            ]
            .join(SEPARATOR)
            .into(),
        )
    }

    fn running_server_frame() -> Message {
        Message::Text(
            [
                "2",
                "fixture",
                "0",
                "12345678",
                "VPN|10.6.22.1|all|all|all|fixture-web",
                "",
                "",
                "fixture-password",
                "3",
                "10.6.22.1",
                "Fixture",
                "0",
                "",
            ]
            .join(SEPARATOR)
            .into(),
        )
    }

    async fn acknowledge_failed_start_cleanup(socket: &mut WebSocketStream<TcpStream>) {
        let request = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        assert_eq!(request.as_str(), "stopServer");
        socket.send(stopped_server_frame()).await.unwrap();
    }

    #[tokio::test]
    async fn missing_initial_state_does_not_leave_a_reusable_uninitialized_socket() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = "missing initial state fixture";
        let worker =
            tokio::spawn(async move { fixture_socket_with_state(listener, script, false).await });
        let adapter = std::sync::Arc::new(NatAdapter::new(
            port,
            format!("{:x}", Sha256::digest(script.as_bytes())),
        ));
        let client = adapter.clone();
        let opening = tokio::spawn(async move {
            let mut channel = client.channel.lock().await;
            client.open(&mut channel).await
        });
        let _socket = worker.await.unwrap();
        advance_server_time(Duration::from_secs(6)).await;
        let error = opening.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert!(error.message.contains("本机状态"));
        assert!(!adapter.cached_account().await.protocol_supported);
        // The old listener is gone: another open must attempt a real handshake,
        // rather than succeed using the socket from the failed initialization.
        let error = adapter.snapshot().await.unwrap_err();
        assert_eq!(error.code, ErrorCode::NetworkFailed);
    }

    #[tokio::test]
    async fn delayed_driver_failure_remains_visible_after_the_old_twenty_second_deadline() {
        let (adapter, mut socket) = server_fixture(true).await;
        let server = adapter.clone();
        let starting = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(request.starts_with("startServer<$!$>"));
        socket
            .send(Message::Text("loading<$!$>show".into()))
            .await
            .unwrap();

        advance_server_time(Duration::from_secs(25)).await;
        assert!(!starting.is_finished());
        socket
            .send(Message::Text(
                ["info", "虚拟网卡创建失败!"].join(SEPARATOR).into(),
            ))
            .await
            .unwrap();
        acknowledge_failed_start_cleanup(&mut socket).await;
        let error = tokio::time::timeout(Duration::from_secs(2), starting)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::VirtualAdapterFailed);
        assert!(!error.message.contains("fixture-password"));
    }

    #[tokio::test]
    async fn completing_slow_initialization_keeps_the_full_service_ready_wait() {
        let (adapter, mut socket) = server_fixture(true).await;
        let server = adapter.clone();
        let starting = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(request.starts_with("startServer<$!$>"));
        socket
            .send(Message::Text("loading<$!$>show".into()))
            .await
            .unwrap();
        advance_server_time(Duration::from_secs(65)).await;
        assert!(!starting.is_finished());
        socket
            .send(Message::Text("loading<$!$>hied".into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        advance_server_time(Duration::from_secs(19)).await;
        assert!(!starting.is_finished());
        socket
            .send(Message::Text(
                [
                    "2",
                    "fixture",
                    "0",
                    "12345678",
                    "VPN|10.6.22.1|all|all|all|fixture-web",
                    "",
                    "",
                    "fixture-password",
                    "3",
                    "10.6.22.1",
                    "Fixture",
                    "0",
                    "",
                ]
                .join(SEPARATOR)
                .into(),
            ))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), starting)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(adapter.cached_account().await.server_running);
    }

    #[tokio::test]
    async fn service_ready_timeout_reports_its_phase_and_duplicate_completion_does_not_extend_it() {
        let (adapter, mut socket) = server_fixture(true).await;
        let server = adapter.clone();
        let starting = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        socket.next().await.unwrap().unwrap();
        socket
            .send(Message::Text("loading<$!$>hide".into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        advance_server_time(Duration::from_secs(19)).await;
        assert!(!starting.is_finished());
        socket
            .send(Message::Text("loading<$!$>hied".into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        advance_server_time(Duration::from_secs(2)).await;
        acknowledge_failed_start_cleanup(&mut socket).await;
        let error = starting.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert!(error.message.contains("服务就绪"));
    }

    #[tokio::test]
    async fn vpn_configuration_timeout_reports_its_phase() {
        let (adapter, mut socket) = server_fixture(false).await;
        let server = adapter.clone();
        let starting = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert_eq!(request.as_str(), "VPN<$!$>1");
        advance_server_time(Duration::from_secs(9)).await;
        let error = starting.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert!(error.message.contains("VPN 模式配置"));
    }

    #[tokio::test]
    async fn initialization_timeout_is_bounded_and_stopping_keeps_twenty_seconds() {
        let (adapter, mut socket) = server_fixture(true).await;
        let server = adapter.clone();
        let starting = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        socket.next().await.unwrap().unwrap();
        socket
            .send(Message::Text("loading<$!$>show".into()))
            .await
            .unwrap();
        advance_server_time(Duration::from_secs(71)).await;
        acknowledge_failed_start_cleanup(&mut socket).await;
        let error = starting.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert!(error.message.contains("Wintun 初始化"));

        adapter.channel.lock().await.account.server_running = true;
        let server = adapter.clone();
        let stopping = tokio::spawn(async move { server.set_server(false, "", "").await });
        let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert_eq!(request.as_str(), "stopServer");
        advance_server_time(Duration::from_secs(21)).await;
        let error = stopping.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert!(error.message.contains("停止服务"));
    }
    #[tokio::test]
    async fn stopped_start_reply_is_cleaned_up_and_a_retry_can_start() {
        let (adapter, mut socket) = server_fixture(true).await;
        let server = adapter.clone();
        let starting = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        assert!(socket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap()
            .starts_with("startServer<$!$>"));
        socket.send(stopped_server_frame()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        advance_server_time(Duration::from_secs(3)).await;
        acknowledge_failed_start_cleanup(&mut socket).await;
        let error = starting.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::NetworkFailed);
        assert!(error.message.contains("服务未启动"));
        assert!(!adapter.channel.lock().await.server_start_pending);

        let server = adapter.clone();
        let retry = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        assert!(socket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap()
            .starts_with("startServer<$!$>"));
        socket
            .send(Message::Text("loading<$!$>show".into()))
            .await
            .unwrap();
        socket
            .send(Message::Text("loading<$!$>hied".into()))
            .await
            .unwrap();
        socket.send(running_server_frame()).await.unwrap();
        retry.await.unwrap().unwrap();
        assert!(adapter.cached_account().await.server_running);
    }

    #[tokio::test]
    async fn a_stopped_snapshot_during_start_does_not_reject_later_initialization() {
        let (adapter, mut socket) = server_fixture(true).await;
        let server = adapter.clone();
        let starting = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        socket.next().await.unwrap().unwrap();
        socket.send(stopped_server_frame()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        advance_server_time(Duration::from_secs(1)).await;
        assert!(!starting.is_finished());
        socket
            .send(Message::Text("loading<$!$>show".into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        advance_server_time(Duration::from_secs(4)).await;
        assert!(!starting.is_finished());
        socket.send(running_server_frame()).await.unwrap();
        starting.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn an_unconfirmed_cleanup_must_be_retried_before_another_start() {
        let (adapter, mut socket) = server_fixture(true).await;
        let server = adapter.clone();
        let starting = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        socket.next().await.unwrap().unwrap();
        socket
            .send(Message::Text("info<$!$>本机服务启动失败".into()))
            .await
            .unwrap();
        assert_eq!(
            socket
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap()
                .as_str(),
            "stopServer"
        );
        advance_server_time(Duration::from_secs(21)).await;
        let error = starting.await.unwrap().unwrap_err();
        assert!(error.message.contains("停止尚未确认"));
        assert!(adapter.channel.lock().await.server_start_pending);

        // A background stopped-state frame cannot erase the unfinished operation.
        socket.send(stopped_server_frame()).await.unwrap();
        let server = adapter.clone();
        let retry = tokio::spawn(async move {
            server
                .set_server(true, "fixture-password", "fixture-web")
                .await
        });
        acknowledge_failed_start_cleanup(&mut socket).await;
        assert!(socket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap()
            .starts_with("startServer<$!$>"));
        socket.send(running_server_frame()).await.unwrap();
        retry.await.unwrap().unwrap();
        assert!(!adapter.channel.lock().await.server_start_pending);
    }

    #[tokio::test]
    async fn starting_a_server_during_a_client_connection_preserves_that_connection() {
        let (adapter, mut socket) = server_fixture(true).await;
        {
            let mut channel = adapter.channel.lock().await;
            channel.connection = Some(NetConnection {
                peer_id: Some("fixture-peer".into()),
                ..NetConnection::default()
            });
        }
        let error = adapter
            .set_server(true, "fixture-password", "fixture-web")
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Busy);
        assert_eq!(
            adapter
                .cached_connection()
                .await
                .unwrap()
                .peer_id
                .as_deref(),
            Some("fixture-peer")
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(75), socket.next())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn client_driver_failure_uses_the_native_reason_field() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = "client driver fixture";
        let worker = tokio::spawn(async move {
            let mut socket = fixture_socket(listener, script).await;
            let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(request.as_str(), "conpc<$!$>fixture-peer");
            socket
                .send(Message::Text(
                    ["conpc", "no", "虚拟网卡创建失败"].join(SEPARATOR).into(),
                ))
                .await
                .unwrap();
        });
        let adapter = NatAdapter::new(port, format!("{:x}", Sha256::digest(script.as_bytes())));
        let error = adapter
            .connect(Some("fixture-peer"), None, None, &CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::VirtualAdapterFailed);
        assert!(!error.retryable);
        worker.await.unwrap();
    }
    #[tokio::test]
    async fn device_disconnect_waits_for_confirmation_and_preserves_other_devices() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = "disconnect fixture";
        let (stopping, stopped_request) = tokio::sync::oneshot::channel();
        let (confirm, confirmation) = tokio::sync::oneshot::channel();
        let worker = tokio::spawn(async move {
            let mut socket = fixture_socket(listener, script).await;
            let connected = [
                "pclist",
                "10.8.0.2",
                "45881236",
                "fixture-password",
                "2",
                "",
            ]
            .join(SEPARATOR);
            // Initial snapshot, an unrelated device, then the matching device.
            for _ in 0..3 {
                let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
                assert_eq!(request.as_str(), "pclist<$!$>");
                socket
                    .send(Message::Text(connected.clone().into()))
                    .await
                    .unwrap();
            }
            let stop = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(stop.as_str(), "stopcon<$!$>");
            let refresh = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(refresh.as_str(), "pclist<$!$>");
            stopping.send(()).unwrap();
            confirmation.await.unwrap();
            socket.send(Message::Text("discon".into())).await.unwrap();
        });
        let adapter = std::sync::Arc::new(NatAdapter::new(
            port,
            format!("{:x}", Sha256::digest(script.as_bytes())),
        ));
        adapter.snapshot().await.unwrap();
        let connection = adapter.cached_connection().await.unwrap();
        assert_eq!(connection.code.as_deref(), Some("45881236"));
        assert_eq!(connection.local_address.as_deref(), Some("10.8.0.2"));
        adapter
            .disconnect_device(&Device {
                peer_code: Some("99999999".into()),
                ..Device::default()
            })
            .await
            .unwrap();
        assert!(adapter.cached_connection().await.is_some());
        let target = Device {
            peer_code: Some("45881236".into()),
            ..Device::default()
        };
        let disconnect_adapter = adapter.clone();
        let mut disconnect =
            tokio::spawn(async move { disconnect_adapter.disconnect_device(&target).await });
        stopped_request.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(75), &mut disconnect)
                .await
                .is_err()
        );
        confirm.send(()).unwrap();
        disconnect.await.unwrap().unwrap();
        assert!(adapter.cached_connection().await.is_none());
        worker.await.unwrap();
    }
    #[tokio::test]
    async fn enabling_vpn_waits_for_mapping_and_sends_both_password_fields() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = "server settings fixture";
        let worker = tokio::spawn(async move {
            let (mut http, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                http.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
            }
            http.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{script}",
                    script.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
            drop(http);
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let stopped = |mapping: &str| {
                [
                    "1",
                    "fixture",
                    "0",
                    "12345678",
                    mapping,
                    "old-password",
                    "3",
                    "",
                    "Fixture",
                    "0",
                    "",
                ]
                .join(SEPARATOR)
            };
            socket
                .send(Message::Text(stopped("").into()))
                .await
                .unwrap();
            let vpn = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(vpn.as_str(), "VPN<$!$>1");
            socket
                .send(Message::Text(stopped("VPN||all|all|all|").into()))
                .await
                .unwrap();
            let start = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let values: Vec<_> = start.split(SEPARATOR).collect();
            assert!(
                values[0] == "startServer"
                    && values[1] == "new-password"
                    && values[3] == "new-password"
            );
            socket
                .send(Message::Text(
                    [
                        "2",
                        "fixture",
                        "0",
                        "12345678",
                        "VPN|10.8.0.1|all|all|all|new-password",
                        "",
                        "",
                        "new-password",
                        "3",
                        "10.8.0.1",
                        "Fixture",
                        "0",
                        "",
                    ]
                    .join(SEPARATOR)
                    .into(),
                ))
                .await
                .unwrap();
            assert_eq!(
                socket
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .into_text()
                    .unwrap()
                    .as_str(),
                "stopServer"
            );
            socket
                .send(Message::Text(
                    stopped("VPN||all|all|all|new-password").into(),
                ))
                .await
                .unwrap();
            let start = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let values: Vec<_> = start.split(SEPARATOR).collect();
            assert!(values[1] == "a-long-connection-password" && values[3] == "web123");
            socket
                .send(Message::Text(
                    [
                        "2",
                        "fixture",
                        "0",
                        "12345678",
                        "VPN|10.8.0.1|all|all|all|web123",
                        "",
                        "",
                        "a-long-connection-password",
                        "3",
                        "10.8.0.1",
                        "Fixture",
                        "0",
                        "",
                    ]
                    .join(SEPARATOR)
                    .into(),
                ))
                .await
                .unwrap();
        });
        let adapter = NatAdapter::new(port, format!("{:x}", Sha256::digest(script.as_bytes())));
        adapter.set_server(true, "new-password", "").await.unwrap();
        assert!(adapter.cached_account().await.server_running);
        adapter.set_server(false, "", "").await.unwrap();
        adapter
            .set_server(true, "a-long-connection-password", "web123")
            .await
            .unwrap();
        worker.await.unwrap();
    }
    #[test]
    fn local_server_password_errors_do_not_invalidate_remote_credentials() {
        let error = AppError::server_info("页面访问密码错误");
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(!error.reauthenticate);
        assert!(!error.message.contains("授权已失效"));
    }
    #[test]
    fn vpn_uses_host_mapping_not_client_ip() {
        assert_eq!(
            vpn_address("组网|10.9.0.1|all|all|all|"),
            Some("10.9.0.1".into())
        );
        assert_eq!(vpn_address("web|127.0.0.1|80|tcp|80|"), None);
        assert_eq!(
            vpn_address("组网; 10.9.0.1 ;all;all;all;"),
            Some("10.9.0.1".into())
        );
        for unusable in ["0.0.0.0", "127.0.0.1", "224.0.0.1", "255.255.255.255", "::"] {
            assert_eq!(vpn_address(&format!("VPN|{unusable}|all|all|all|")), None);
        }
    }
    #[tokio::test]
    async fn local_address_tracks_active_client_and_server_without_stopped_drafts() {
        let adapter = NatAdapter::new(0, String::new());
        let mut channel = adapter.channel.lock().await;
        let server = |state| {
            [
                state,
                "fixture",
                "0",
                "12345678",
                "VPN|10.6.22.1|all|all|all|fixture-web",
                "",
                "",
                "fixture-password",
                "3",
                "",
                "Fixture",
            ]
            .join(SEPARATOR)
        };
        channel.apply(&server("2")).unwrap();
        assert_eq!(
            channel.account.local_vpn_address.as_deref(),
            Some("10.6.22.1")
        );
        channel
            .apply(
                &[
                    "pclist",
                    "10.8.0.2",
                    "45881236",
                    "fixture-password",
                    "2",
                    "",
                ]
                .join(SEPARATOR),
            )
            .unwrap();
        assert_eq!(
            channel.account.local_vpn_address.as_deref(),
            Some("10.6.22.1")
        );
        channel.apply("discon").unwrap();
        assert_eq!(
            channel.account.local_vpn_address.as_deref(),
            Some("10.6.22.1")
        );
        channel
            .apply(
                &[
                    "pclist",
                    "10.8.0.2",
                    "45881236",
                    "fixture-password",
                    "2",
                    "",
                ]
                .join(SEPARATOR),
            )
            .unwrap();
        channel.apply(&server("1")).unwrap();
        assert_eq!(
            channel.account.local_vpn_address.as_deref(),
            Some("10.8.0.2")
        );
        channel.apply("discon").unwrap();
        assert_eq!(channel.account.local_vpn_address, None);
        channel.apply(&server("2")).unwrap();
        channel.apply("0").unwrap();
        assert_eq!(channel.account.local_vpn_address, None);
        channel
            .apply(&server("2").replace("10.6.22.1", "0.0.0.0"))
            .unwrap();
        channel
            .apply(
                &[
                    "pclist",
                    "10.8.0.2",
                    "45881236",
                    "fixture-password",
                    "2",
                    "",
                ]
                .join(SEPARATOR),
            )
            .unwrap();
        // A client address cannot be shared as an unknown host service address.
        assert_eq!(channel.account.local_vpn_address, None);
    }
    #[tokio::test]
    async fn peer_information_survives_unloaded_list_metadata_and_clears_on_offline_reply() {
        let adapter = NatAdapter::new(0, String::new());
        let mut channel = adapter.channel.lock().await;
        channel.apply("1<$!$>fixture").unwrap();
        let list = [
            "pclist",
            "fixture-peer",
            "Peer",
            "203.0.113.8",
            "",
            "10.8.0.2",
            "",
            "",
            "1",
            "fixture-peer",
        ]
        .join(SEPARATOR);
        channel.apply(&list).unwrap();
        channel
            .apply(
                &[
                    "pcinfo",
                    "fixture-peer",
                    "203.0.113.8",
                    "Peer",
                    "组网;10.9.0.1;all;all;all;",
                ]
                .join(SEPARATOR),
            )
            .unwrap();
        channel.apply(&list).unwrap();
        assert_eq!(channel.hosts[0].address.as_deref(), Some("10.9.0.1"));
        assert_eq!(
            channel
                .connection
                .as_ref()
                .unwrap()
                .remote_address
                .as_deref(),
            Some("10.9.0.1")
        );
        assert_eq!(
            channel.account.local_vpn_address.as_deref(),
            Some("10.8.0.2")
        );
        channel.apply("pcinfo<$!$>fixture-peer").unwrap();
        assert!(!channel.hosts[0].online);
        assert_eq!(channel.hosts[0].address, None);
        assert_eq!(channel.connection.as_ref().unwrap().remote_address, None);
    }
    #[tokio::test]
    async fn same_account_success_retains_requested_identity_when_reply_omits_it() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = "connection identity fixture";
        let worker = tokio::spawn(async move {
            let mut socket = fixture_socket(listener, script).await;
            let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(request.as_str(), "conpc<$!$>fixture-peer");
            socket
                .send(Message::Text("conpc<$!$>yes<$!$><$!$>10.8.0.2".into()))
                .await
                .unwrap();
        });
        let adapter = NatAdapter::new(port, format!("{:x}", Sha256::digest(script.as_bytes())));
        let connection = adapter
            .connect(Some("fixture-peer"), None, None, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(connection.peer_id.as_deref(), Some("fixture-peer"));
        assert_eq!(connection.local_address.as_deref(), Some("10.8.0.2"));
        assert_eq!(connection.remote_address, None);
        assert_eq!(
            adapter.cached_account().await.local_vpn_address.as_deref(),
            Some("10.8.0.2")
        );
        worker.await.unwrap();
    }
    #[tokio::test]
    async fn code_connection_keeps_client_ip_separate_from_unknown_peer_address() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = "access code address fixture";
        let worker = tokio::spawn(async move {
            let mut socket = fixture_socket(listener, script).await;
            let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert_eq!(
                request.as_str(),
                "concode<$!$>45881236<$!$>fixture-password"
            );
            socket
                .send(Message::Text("conpc<$!$>yes<$!$><$!$>10.8.0.2".into()))
                .await
                .unwrap();
        });
        let adapter = NatAdapter::new(port, format!("{:x}", Sha256::digest(script.as_bytes())));
        let connection = adapter
            .connect(
                Some("unrelated-cached-peer"),
                Some("45881236"),
                Some("fixture-password"),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(connection.code.as_deref(), Some("45881236"));
        assert_eq!(connection.peer_id, None);
        assert_eq!(connection.local_address.as_deref(), Some("10.8.0.2"));
        assert_eq!(connection.remote_address, None);
        worker.await.unwrap();
    }
    #[test]
    fn rejects_delimiter_in_secret_without_echo() {
        let error = validate_field("secret<$!$>stopServer").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(!error.message.contains("secret"));
    }
    #[tokio::test]
    async fn background_authorization_challenge_remains_visible_to_login_and_clears_after_verification(
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let script = "isolated protocol fixture";
        let worker = tokio::spawn(async move {
            let (mut http, _) = listener.accept().await.unwrap();
            let mut request = vec![];
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                http.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
            }
            http.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{script}",
                    script.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
            drop(http);
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket
                .send(Message::Text("authorize".into()))
                .await
                .unwrap();
            let authorization = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert!(authorization.starts_with("authorize<$!$>"));
            socket.send(Message::Text("0".into())).await.unwrap();
            let login = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert!(login.starts_with("login<$!$>fixture-account<$!$>"));
            socket
                .send(Message::Text(
                    "1<$!$>fixture-account<$!$><$!$>12345678<$!$><$!$><$!$>3<$!$><$!$>Fixture"
                        .into(),
                ))
                .await
                .unwrap();
        });
        let adapter = NatAdapter::new(port, format!("{:x}", Sha256::digest(script.as_bytes())));
        assert_eq!(
            adapter.snapshot().await.unwrap_err().code,
            ErrorCode::LocalAuthorizationRequired
        );
        assert_eq!(
            adapter
                .login(
                    "fixture-account",
                    "fixture-password",
                    &CancellationToken::new()
                )
                .await
                .unwrap_err()
                .code,
            ErrorCode::LocalAuthorizationRequired
        );
        adapter.authorize("fixture-local-password").await.unwrap();
        let account = adapter
            .login(
                "fixture-account",
                "fixture-password",
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(account.authenticated);
        assert_eq!(account.username.as_deref(), Some("fixture-account"));
        worker.await.unwrap();
    }
}
