use super::*;
use crate::shortcuts::{ShortcutAction, Shortcuts};

impl Runtime {
    pub(super) fn watch_stream_shortcuts(
        &self,
        session: &SessionData,
        child: &Child,
        mut events: tokio::sync::mpsc::Receiver<ShortcutAction>,
    ) {
        let (Some(session_id), Some(pid)) = (session.snapshot.id.clone(), child.id()) else {
            return;
        };
        let runtime = self.app.state::<Arc<Runtime>>().inner().clone();
        tauri::async_runtime::spawn(async move {
            while let Some(action) = events.recv().await {
                let is_current = {
                    let session = runtime.session.lock().await;
                    session.snapshot.id.as_deref() == Some(&session_id)
                        && session.child.as_ref().and_then(Child::id) == Some(pid)
                        && !session.cancel.is_cancelled()
                };
                if !is_current {
                    break;
                }
                let result = match action {
                    ShortcutAction::Disconnect => runtime.disconnect(&session_id).await.map(|_| ()),
                    ShortcutAction::Display => runtime.toggle_display_awake().await,
                    ShortcutAction::Mouse => continue,
                };
                if let Err(error) = result {
                    let _ = runtime.app.emit_to("main", "ll:operation-error", error);
                }
            }
        });
    }

    pub async fn trigger_shortcut(self: &Arc<Self>, action: ShortcutAction) -> Result<()> {
        match action {
            ShortcutAction::Display => self.toggle_display_awake().await,
            ShortcutAction::Disconnect => self.disconnect_from_tray().await,
            ShortcutAction::Mouse => {
                let _update = self.session_update.lock().await;
                let session = self.session.lock().await;
                if session.snapshot.stage != "streaming" || session.cancel.is_cancelled() {
                    return Ok(());
                }
                if let Some(pid) = session.child.as_ref().and_then(Child::id) {
                    self.moonlight
                        .ipc(
                            &session.pipe_name,
                            &session.token,
                            "input",
                            json!({"toggle": true}),
                            pid,
                        )
                        .await?;
                }
                Ok(())
            }
        }
    }

    async fn toggle_display_awake(&self) -> Result<()> {
        let _guard = self.administrative_lock.lock().await;
        if self.exiting.load(Ordering::SeqCst) {
            return Ok(());
        }
        {
            let mut store = self.store()?;
            let mut power = self
                .power
                .lock()
                .map_err(|_| AppError::new(ErrorCode::SystemFailed, "电源管理不可用。"))?;
            let previous = store.preferences.keep_display_awake;
            let prevent_sleep = store.preferences.prevent_sleep;
            power.apply(prevent_sleep, !previous)?;
            store.preferences.keep_display_awake = !previous;
            if let Err(error) = store.save_preferences() {
                store.preferences.keep_display_awake = previous;
                let _ = power.apply(prevent_sleep, previous);
                return Err(error);
            }
        }
        let snapshot = self.snapshot().await?;
        let _ = self.app.emit_to("main", "ll:snapshot", snapshot);
        Ok(())
    }

    pub async fn save_shortcuts(&self, shortcuts: Shortcuts) -> Result<AppSnapshot> {
        shortcuts.validate()?;
        let _update = self.session_update.lock().await;
        let _admin = self.administrative_lock.lock().await;
        let session = self.session.lock().await;
        let previous = self.store()?.preferences.shortcuts.clone();
        if shortcuts != previous {
            if session.snapshot.stage == "startingStream" && session.child.is_some() {
                return Err(AppError::new(
                    ErrorCode::Busy,
                    "远控正在启动，请稍候再保存快捷键。",
                ));
            }
            let pid = session.child.as_ref().and_then(Child::id).filter(|_| {
                session.snapshot.stage == "streaming" && !session.cancel.is_cancelled()
            });
            if let Some(pid) = pid {
                self.apply_stream_shortcuts(&session, pid, &shortcuts)
                    .await?;
            }
            let saved = (|| {
                let mut store = self.store()?;
                store.preferences.shortcuts = shortcuts;
                if let Err(error) = store.save_preferences() {
                    store.preferences.shortcuts = previous.clone();
                    return Err(error);
                }
                Ok(())
            })();
            if let Err(error) = saved {
                if let Some(pid) = pid {
                    let _ = self.apply_stream_shortcuts(&session, pid, &previous).await;
                }
                return Err(error);
            }
        }
        drop(session);
        self.snapshot().await
    }

    async fn apply_stream_shortcuts(
        &self,
        session: &SessionData,
        pid: u32,
        shortcuts: &Shortcuts,
    ) -> Result<()> {
        self.moonlight
            .ipc(
                &session.pipe_name,
                &session.token,
                "apply",
                json!({"patch":{"kind":"shortcuts"},"shortcuts":shortcuts}),
                pid,
            )
            .await?;
        Ok(())
    }
}
