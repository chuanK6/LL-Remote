use super::*;
use tokio::sync::oneshot;

const RESTORE_DELAY: Duration = Duration::from_secs(3);
const RESTORE_RETRY: Duration = Duration::from_secs(8);

fn same_ip(local: &str, remote: &str) -> bool {
    let local = local.parse::<std::net::IpAddr>().ok();
    let remote = validate_address(remote).ok().and_then(|(host, _)| {
        host.trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .ok()
    });
    local.is_some() && local == remote
}

fn terminal(stage: &str) -> bool {
    ["disconnected", "canceled", "failed"].contains(&stage)
}

impl Runtime {
    pub async fn resolve_role_switch(&self, id: &str, enabled: bool) -> Result<()> {
        let mut session = self.session.lock().await;
        if session.snapshot.id.as_deref() != Some(id)
            || session.snapshot.stage != "awaitingRoleSwitch"
            || session.cancel.is_cancelled()
        {
            return Err(AppError::new(
                ErrorCode::SessionEnded,
                "此次连接确认已失效。",
            ));
        }
        session
            .role_switch_prompt
            .take()
            .ok_or_else(|| AppError::new(ErrorCode::SessionEnded, "此次连接确认已处理。"))?
            .send(enabled)
            .map_err(|_| AppError::new(ErrorCode::SessionEnded, "此次连接已结束。"))
    }

    pub async fn cancel_role_restore(&self) {
        self.acceptance_intent_epoch.fetch_add(1, Ordering::SeqCst);
        self.clear_role_resume().await;
    }

    pub async fn revoke_role_for_account_change(&self, username: &str) {
        let changed =
            self.session
                .lock()
                .await
                .role_resume
                .as_ref()
                .is_some_and(|resume| {
                    resume.username.as_deref().is_none_or(|previous| {
                        !previous.trim().eq_ignore_ascii_case(username.trim())
                    })
                });
        if changed {
            self.cancel_role_restore().await;
        }
    }

    async fn clear_role_resume(&self) {
        let mut session = self.session.lock().await;
        session.role_resume = None;
        session.snapshot.local_acceptance_paused = false;
        self.emit_session(&session.snapshot);
        self.set_role_warning(None);
    }

    pub async fn require_no_role_pause(&self) -> Result<()> {
        if self.session.lock().await.role_resume.is_some() {
            return Err(AppError::new(
                ErrorCode::Busy,
                "本机接受控制正因组网地址相同而暂时暂停，远控断开后会延迟检查并恢复。",
            ));
        }
        Ok(())
    }

    fn set_role_warning(&self, warning: Option<String>) {
        if let Ok(mut current) = self.role_switch_warning.lock() {
            *current = warning;
        }
    }

    pub(super) async fn check_role_conflict(&self, address: Option<&str>) -> Result<()> {
        let account = self.nat.cached_account().await;
        if let (true, Some(local), Some(address)) = (
            account.server_running,
            account.local_vpn_address.as_deref(),
            address,
        ) {
            if same_ip(local, address) {
                self.session.lock().await.role_conflict_address = Some(address.to_owned());
                return Err(AppError::new(
                    ErrorCode::RoleSwitchRequired,
                    "本机与被控制端的组网地址相同，需要切换角色后重新建立组网。",
                ));
            }
        }
        Ok(())
    }

    pub(super) async fn prepare_role_for_connection(
        self: &Arc<Self>,
        request: &ConnectRequest,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let mut enable_from_prompt = false;
        let intent_epoch = self.acceptance_intent_epoch.load(Ordering::SeqCst);
        loop {
            let guard = self.administrative_lock.lock().await;
            if cancel.is_cancelled() || self.exiting.load(Ordering::SeqCst) {
                return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
            }
            if enable_from_prompt {
                let mut store = self.store()?;
                let previous = store.preferences.automatic_role_switch;
                store.preferences.automatic_role_switch = true;
                if let Err(error) = store.save_preferences() {
                    store.preferences.automatic_role_switch = previous;
                    return Err(error);
                }
            }
            if self.session.lock().await.role_resume.is_some() {
                return Ok(());
            }
            let account = self.restore_jiaoyue(cancel).await?;
            if !account.authenticated || !account.server_running {
                return Ok(());
            }
            let device_id =
                request
                    .device_id
                    .clone()
                    .or(self.session.lock().await.snapshot.device_id.clone());
            let device = device_id
                .as_deref()
                .map(|id| self.store()?.device(id))
                .transpose()?;
            let mut target =
                request
                    .address
                    .clone()
                    .or(self.session.lock().await.role_conflict_address.clone());
            if target.is_none()
                && request.peer_code.is_none()
                && device
                    .as_ref()
                    .is_some_and(|device| device.peer_code.is_none())
            {
                if let Some(id) = device.as_ref().and_then(|device| device.nat_id.as_deref()) {
                    if let Ok(Ok(host)) =
                        tokio::time::timeout(Duration::from_secs(3), self.nat.host_info(id, cancel))
                            .await
                    {
                        target = host.address;
                    }
                }
            }
            target = target.or_else(|| device.as_ref().and_then(|device| device.address.clone()));
            if !account
                .local_vpn_address
                .as_deref()
                .zip(target.as_deref())
                .is_some_and(|(local, remote)| same_ip(local, remote))
            {
                return Ok(());
            }
            if !self.store()?.preferences.automatic_role_switch {
                let (send, receive) = oneshot::channel();
                {
                    let mut session = self.session.lock().await;
                    session.role_switch_prompt = Some(send);
                    session.snapshot.stage = "awaitingRoleSwitch".into();
                    session.snapshot.error = None;
                    self.emit_session(&session.snapshot);
                }
                // The decision command and settings must remain usable while
                // the product dialog is pending; do not hold the admin lock.
                drop(guard);
                crate::show_main_window(&self.app);
                let enabled = tokio::select! {
                    _ = cancel.cancelled() => false,
                    decision = receive => decision.unwrap_or(false),
                };
                self.session.lock().await.role_switch_prompt = None;
                if !enabled {
                    return Err(AppError::new(ErrorCode::Canceled, "已停止此次连接。"));
                }
                enable_from_prompt = true;
                continue;
            }
            // A collision discovered after connecting must close that client
            // tunnel first. Never silently disconnect an unrelated legacy one.
            self.nat.end_session_connection(None).await?;
            if self.nat.cached_connection().await.is_some() {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "请先断开已有皎月连组网，再切换本机角色。",
                ));
            }
            let passwords = self.local_server_passwords().await;
            if passwords.connection_password.is_empty() {
                return Err(AppError::new(
                    ErrorCode::CredentialUnavailable,
                    "本机连接密码不可用，请先在本机接受控制页面保存密码，以便断开后恢复。",
                ));
            }
            {
                let mut session = self.session.lock().await;
                if cancel.is_cancelled()
                    || self.exiting.load(Ordering::SeqCst)
                    || self.acceptance_intent_epoch.load(Ordering::SeqCst) != intent_epoch
                {
                    return Err(AppError::new(ErrorCode::Canceled, "连接已取消。"));
                }
                session.role_resume = Some(RoleResume {
                    session_id: session.snapshot.id.clone().unwrap(),
                    username: account.username,
                    connection_password: Zeroizing::new(passwords.connection_password.clone()),
                    web_password: Zeroizing::new(passwords.web_password.clone()),
                    previous_auto_pause: self.pause_automatic_acceptance.load(Ordering::SeqCst),
                    intent_epoch,
                });
            }
            self.pause_automatic_acceptance(true);
            self.nat.set_server(false, "", "").await?;
            self.store()?.preferences.accept_control = false;
            self.sunshine.stop().await?;
            let warning = self
                .sunshine
                .restore_vpn_firewall()
                .await
                .err()
                .map(|error| {
                    format!(
                        "本机接受控制已暂时暂停，防火墙恢复尚未完成。{}",
                        error.message
                    )
                });
            self.set_acceptance_firewall_warning(warning);
            {
                let mut session = self.session.lock().await;
                session.snapshot.local_acceptance_paused = true;
                session.snapshot.stage = "preparingNetwork".into();
                session.role_conflict_address = None;
                self.emit_session(&session.snapshot);
            }
            if let Ok(snapshot) = self.snapshot().await {
                let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
            }
            return Ok(());
        }
    }

    pub(super) async fn finish_role_session(&self) -> Result<()> {
        if self.session.lock().await.role_resume.is_none() {
            return Ok(());
        }
        let _update = self.session_update.lock().await;
        let _guard = self.administrative_lock.lock().await;
        {
            let session = self.session.lock().await;
            if session.changing || !terminal(&session.snapshot.stage) {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "会话仍在连接或重连，接受控制将保持暂停。",
                ));
            }
        }
        self.kill_stream().await;
        if self.session.lock().await.child.is_some() {
            return Err(AppError::new(
                ErrorCode::Timeout,
                "Moonlight 尚未确认退出，请重试断开。",
            ));
        }
        let device_id = self.session.lock().await.snapshot.device_id.clone();
        let device = device_id.and_then(|id| self.store().ok()?.device(&id).ok());
        self.nat.end_session_connection(device.as_ref()).await?;
        if self.nat.cached_connection().await.is_some() {
            return Err(AppError::new(
                ErrorCode::Busy,
                "组网连接尚未断开，接受控制将保持暂停。",
            ));
        }
        Ok(())
    }

    pub(super) async fn schedule_role_restore(self: &Arc<Self>) {
        let id = {
            let session = self.session.lock().await;
            let Some(resume) = &session.role_resume else {
                return;
            };
            resume.session_id.clone()
        };
        let runtime = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(RESTORE_DELAY).await;
            loop {
                if runtime.exiting.load(Ordering::SeqCst) {
                    return;
                }
                let permit = match runtime.gate.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                };
                let update = runtime.session_update.lock().await;
                let guard = runtime.administrative_lock.lock().await;
                let mut permit = Some(permit);
                let resume = {
                    let session = runtime.session.lock().await;
                    if session.snapshot.id.as_deref() != Some(&id) || session.role_resume.is_none()
                    {
                        return;
                    }
                    // Preserve this lease across every Moonlight process restart.
                    // Restoring requires a terminal logical session, not just an
                    // exited/replaced process or a temporarily empty child slot.
                    if session.changing
                        || !terminal(&session.snapshot.stage)
                        || session.child.is_some()
                    {
                        drop(session);
                        drop(guard);
                        drop(update);
                        drop(permit);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                    session.role_resume.clone().unwrap()
                };
                let result: Result<bool> = async {
                    let account = runtime.nat.snapshot().await?.0;
                    if runtime.nat.cached_connection().await.is_some() {
                        return Ok(false);
                    }
                    if !account.authenticated {
                        return Err(AppError::new(
                            ErrorCode::LoginRequired,
                            "恢复本机接受控制前需要恢复皎月连登录。",
                        ));
                    }
                    if account
                        .username
                        .as_deref()
                        .map(|name| name.trim().to_lowercase())
                        != resume
                            .username
                            .as_deref()
                            .map(|name| name.trim().to_lowercase())
                    {
                        runtime.cancel_role_restore().await;
                        return Ok(true);
                    }
                    if runtime.exiting.load(Ordering::SeqCst) {
                        return Ok(true);
                    }
                    if runtime.acceptance_intent_epoch.load(Ordering::SeqCst) != resume.intent_epoch
                    {
                        return Ok(true);
                    }
                    // The pending lease still rejects new outgoing sessions and
                    // manual acceptance starts. Release the gate for the existing
                    // elevation fallback while retaining the admin/update locks.
                    drop(permit.take());
                    runtime
                        .start_accepting(
                            &resume.connection_password,
                            &resume.web_password,
                            SystemTime::now(),
                        )
                        .await?;
                    if runtime.acceptance_intent_epoch.load(Ordering::SeqCst) != resume.intent_epoch
                    {
                        return Ok(true);
                    }
                    runtime.store()?.preferences.accept_control = true;
                    runtime.pause_automatic_acceptance(resume.previous_auto_pause);
                    runtime.clear_role_resume().await;
                    runtime.schedule_acceptance_firewall();
                    Ok(true)
                }
                .await;
                let done = match result {
                    Ok(done) => done,
                    Err(error) => {
                        runtime.set_role_warning(Some(format!(
                            "远控已结束，本机接受控制暂未恢复，将继续尝试。{}",
                            error.message
                        )));
                        false
                    }
                };
                if let Ok(snapshot) = runtime.snapshot().await {
                    let _ = runtime.app.emit_to("main", "ll:snapshot", snapshot);
                }
                drop(guard);
                drop(update);
                drop(permit);
                if done {
                    return;
                }
                tokio::time::sleep(RESTORE_RETRY).await;
            }
        });
    }
}
