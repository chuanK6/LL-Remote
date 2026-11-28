use super::*;
use std::net::{IpAddr, SocketAddr};

pub(super) struct Acceptance {
    pub(super) status: DirectAcceptanceStatus,
    started_sunshine: bool,
}

fn direct_target(value: &str) -> Result<(IpAddr, u16)> {
    let value = value.trim();
    let target = value
        .parse::<IpAddr>()
        .map(|ip| (ip, 47989))
        .or_else(|_| {
            value
                .parse::<SocketAddr>()
                .map(|addr| (addr.ip(), addr.port()))
        })
        .ok()
        .filter(|(ip, port)| {
            !ip.is_unspecified() && !ip.is_multicast() && *port != 0 && !value.contains('%')
        });
    target.ok_or_else(|| {
        AppError::new(
            ErrorCode::InvalidInput,
            "请输入对方的 IPv4 或 IPv6 地址。自定义端口可使用 IP:端口，IPv6 请用 [IP]:端口。",
        )
    })
}

impl Runtime {
    pub async fn require_no_direct_acceptance(&self) -> Result<()> {
        if self.direct_acceptance.lock().await.is_some() {
            return Err(AppError::new(
                ErrorCode::Busy,
                "请先在 IP 直连页关闭直连接受控制。",
            ));
        }
        Ok(())
    }

    pub async fn set_direct_acceptance(
        &self,
        access: Option<DirectAccess>,
        address: Option<String>,
    ) -> Result<AppSnapshot> {
        let _guard = self.administrative_lock.lock().await;
        if let Some(access) = access {
            self.require_no_direct_acceptance().await?;
            self.require_no_role_pause().await?;
            if self.exiting.load(Ordering::SeqCst) {
                return Err(AppError::new(ErrorCode::Busy, "LL远程正在退出。"));
            }
            if self.store()?.preferences.accept_control
                || self.nat.cached_account().await.server_running
            {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "请先在开始协助页关闭皎月连接受控制。",
                ));
            }
            let ip = address
                .as_deref()
                .and_then(|value| value.parse::<IpAddr>().ok())
                .filter(|ip| !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast())
                .ok_or_else(|| {
                    AppError::new(ErrorCode::InvalidInput, "请选择本机网卡的 IP 地址。")
                })?;
            self.require_sunshine_account().await?;
            let was_running = self.sunshine.running().await;
            let port = self.sunshine.port()? - 1;
            let address = ip.to_string();
            self.sunshine
                .prepare_direct_firewall(&address, port, access == DirectAccess::Internet)
                .await?;
            let result = async {
                self.sunshine.start().await?;
                let endpoint = SocketAddr::new(ip, port).to_string();
                probe_host(&endpoint, None, &CancellationToken::new()).await.map_err(|_| {
                    AppError::new(ErrorCode::SunshineUnavailable, "Sunshine 未在所选地址提供串流服务，请核对 Sunshine 的 IPv4 / IPv6 绑定设置。")
                })?;
                Ok(())
            }.await;
            if let Err(error) = result {
                let firewall = self.sunshine.restore_direct_firewall().await;
                let stop = if was_running {
                    Ok(())
                } else {
                    self.sunshine.stop().await
                };
                firewall.and(stop)?;
                return Err(error);
            }
            *self.direct_acceptance.lock().await = Some(Acceptance {
                status: DirectAcceptanceStatus {
                    access,
                    address,
                    port,
                },
                started_sunshine: !was_running,
            });
        } else {
            self.stop_direct_acceptance().await?;
        }
        let snapshot = self.snapshot().await?;
        let _ = self.app.emit_to("main", "ll:snapshot", &snapshot);
        Ok(snapshot)
    }

    pub async fn stop_direct_acceptance(&self) -> Result<()> {
        self.sunshine.restore_direct_firewall().await?;
        let mut acceptance = self.direct_acceptance.lock().await;
        if acceptance
            .as_ref()
            .is_some_and(|acceptance| acceptance.started_sunshine)
        {
            self.sunshine.stop().await?;
        }
        *acceptance = None;
        Ok(())
    }

    pub async fn connect_direct(
        self: &Arc<Self>,
        device_id: Option<String>,
        address: Option<String>,
        app: Option<String>,
    ) -> Result<SessionSnapshot> {
        let permit =
            self.gate.clone().try_acquire_owned().map_err(|_| {
                AppError::new(ErrorCode::Busy, "已有连接正在处理，请先取消或断开。")
            })?;
        let device = device_id
            .as_deref()
            .map(|id| self.store()?.device(id))
            .transpose()?;
        if device
            .as_ref()
            .is_some_and(|device| device.source != "direct")
        {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "请选择 IP 直连设备。",
            ));
        }
        let supplied = address
            .as_deref()
            .filter(|address| !address.trim().is_empty());
        let address = supplied
            .or_else(|| device.as_ref().and_then(|device| device.address.as_deref()))
            .ok_or_else(|| {
                AppError::new(ErrorCode::AddressRequired, "请输入被控电脑的 IP 地址。")
            })?;
        let (ip, port) = direct_target(address)?;
        let request = ConnectRequest {
            device_id,
            peer_code: None,
            password: None,
            app,
            address: Some(SocketAddr::new(ip, port).to_string()),
        };
        self.begin_connection(request, device, permit, true).await
    }

    pub(super) async fn connect_direct_attempt(
        self: &Arc<Self>,
        request: &ConnectRequest,
        device: Option<&Device>,
    ) -> Result<()> {
        let cancel = self.session.lock().await.cancel.clone();
        let address = request
            .address
            .as_deref()
            .expect("validated direct connection address");
        let expected = device.and_then(|device| device.uuid.as_deref());
        let scope = self.store()?.account_scope.clone();
        let info = probe_host(address, expected, &cancel).await.map_err(|error| {
            if error.code == ErrorCode::SunshineUnavailable {
                AppError::new(ErrorCode::SunshineUnavailable, format!("无法通过 {address} 访问对方的 Sunshine。请确认对方已开启直连接受控制，并检查地址、Windows 防火墙或路由器端口转发。"))
            } else {
                error
            }
        })?;
        if cancel.is_cancelled() {
            return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
        }
        let (id, preferences) = {
            let mut store = self.store()?;
            if store.account_scope != scope {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "当前设备资料已切换，请重新连接。",
                ));
            }
            let saved = store.account_data.devices.iter().find(|device| {
                device.source == "direct"
                    && device
                        .uuid
                        .as_deref()
                        .is_some_and(|uuid| uuid.eq_ignore_ascii_case(&info.uuid))
            });
            let mut preferences = saved
                .map(|device| device.preferences.clone())
                .unwrap_or_else(|| store.preferences.stream.clone());
            preferences.window_mode = "windowed".into();
            let id = store.merge_device(Device {
                host_name: info.name.clone(),
                display_name: info.name.clone(),
                source: "direct".into(),
                uuid: Some(info.uuid.clone()),
                address: Some(info.address.clone()),
                service_ready: Some(true),
                observed_at: Some(now()),
                preferences: preferences.clone(),
                ..Device::default()
            });
            store.save_devices()?;
            (id, preferences)
        };
        {
            let mut session = self.session.lock().await;
            session.snapshot.device_id = Some(id);
            session.snapshot.device_name = Some(info.name.clone());
            session.address = Some(info.address.clone());
        }
        if let Ok(snapshot) = self.snapshot().await {
            let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
        }
        self.stream_host(request, info, preferences, None, true, &cancel)
            .await
    }
}
