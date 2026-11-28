use crate::{
    error::{AppError, ErrorCode, Result},
    runtime::Runtime,
    signin_status::{self, WebsiteDates},
    storage::{SecretRecord, Vault},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{
    webview::{NewWindowResponse, PageLoadEvent},
    Emitter, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const WINDOW: &str = "jiaoyue-signin";
const HOST: &str = "www.natpierce.cn";
const LOGIN_PATH: &str = "/pc/login/login.html";
const SIGNIN_PATH: &str = "/pc/sign/index.html";

pub async fn open(owner: &WebviewWindow, runtime: &Runtime) -> Result<()> {
    let account = runtime.nat.cached_account().await;
    let credential = match account.username.as_deref() {
        Some(username) if account.authenticated => current_credential(runtime, username).await,
        _ => None,
    };
    let app = owner.app_handle().clone();
    let window = owner.clone();
    let runtime = owner.state::<Arc<Runtime>>().inner().clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    owner
        .with_webview(move |_main_webview| {
            let result = (|| {
                if let Some(signin) = app.get_webview_window(WINDOW) {
                    signin.unminimize()?;
                    return signin.set_focus();
                }
                let url = Url::parse(&format!("https://{HOST}{SIGNIN_PATH}"))
                    .expect("fixed Jiaoyue sign-in URL");
                let credential = Mutex::new(credential);
                let submitted = AtomicBool::new(false);
                let builder = WebviewWindowBuilder::new(&app, WINDOW, WebviewUrl::External(url))
                    .title("LL远程 · 皎月连签到")
                    .inner_size(1140.0, 720.0)
                    .center()
                    .parent(&window)?
                    .incognito(true)
                    .devtools(false)
                    .on_navigation(official_origin)
                    .on_new_window(|_, _| NewWindowResponse::Deny)
                    .on_download(|_, _| false)
                    .on_page_load(move |webview, page| {
                        if page.event() != PageLoadEvent::Finished || !official_origin(page.url()) {
                            return;
                        }
                        if page.url().path() == LOGIN_PATH {
                            let secret = credential.lock().ok().and_then(|mut value| value.take());
                            let json = Zeroizing::new(
                                serde_json::to_string(&secret).expect("string fields serialize"),
                            );
                            let program = include_str!("signin-login.js");
                            if webview
                                .eval(format!("({program})({});", json.as_str()))
                                .is_err()
                            {
                                tracing::warn!("Could not run Jiaoyue website login");
                            }
                        } else if page.url().path() == SIGNIN_PATH
                            && !submitted.swap(true, Ordering::Relaxed)
                        {
                            let _ = webview.eval(include_str!("signin-dates.js"));
                            if webview.eval(include_str!("signin-submit.js")).is_err() {
                                tracing::warn!("Could not activate the Jiaoyue sign-in button");
                            }
                        }
                    });
                #[cfg(windows)]
                let builder = builder.with_environment(_main_webview.environment());
                let signin = builder.build()?;
                let cancel = CancellationToken::new();
                let on_close = cancel.clone();
                signin.on_window_event(move |event| {
                    if matches!(event, WindowEvent::Destroyed) {
                        on_close.cancel();
                    }
                });
                tauri::async_runtime::spawn(observe_dates(runtime, signin, cancel));
                Ok(())
            })();
            let _ = send.send(result);
        })
        .map_err(window_error)?;
    receive.await.map_err(window_error)?.map_err(window_error)
}

async fn observe_dates(runtime: Arc<Runtime>, window: WebviewWindow, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut previous = None;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = interval.tick() => {}
        }
        let (send, receive) = tokio::sync::oneshot::channel();
        let send = Mutex::new(Some(send));
        if window
            .eval_with_callback(include_str!("signin-dates.js"), move |raw| {
                if let Some(sender) = send.lock().ok().and_then(|mut value| value.take()) {
                    let _ = sender.send(serde_json::from_str::<WebsiteDates>(&raw).ok());
                }
            })
            .is_err()
        {
            return;
        }
        let Ok(Ok(Some(dates))) = tokio::time::timeout(Duration::from_secs(2), receive).await
        else {
            continue;
        };
        if previous.as_ref() == Some(&dates) {
            continue;
        }
        let key = signin_status::account_key(&dates.username);
        if key.is_empty() || key.len() > 128 || cancel.is_cancelled() {
            continue;
        }
        let account = runtime.nat.cached_account().await;
        if account.authenticated
            && account
                .username
                .as_deref()
                .is_some_and(|name| signin_status::account_key(name) != key)
        {
            continue;
        }
        if cancel.is_cancelled() {
            return;
        }
        match runtime
            .store()
            .and_then(|store| signin_status::save(&store, dates.clone()))
        {
            Ok(changed) => {
                previous = Some(dates);
                if changed {
                    let _ = runtime.app.emit_to("main", "ll:signin-updated", ());
                }
            }
            Err(error) => {
                tracing::warn!(code = ?error.code, "Could not save Jiaoyue service dates")
            }
        }
    }
}

pub fn close(owner: &WebviewWindow) {
    if let Some(signin) = owner.get_webview_window(WINDOW) {
        let _ = signin.close();
    }
}

pub async fn current_credential(runtime: &Runtime, username: &str) -> Option<SecretRecord> {
    runtime
        .nat
        .restart_login_secret()
        .await
        .or_else(|| {
            runtime
                .vault
                .get(&Vault::reference("local", "jiaoyue-login"))
                .ok()
        })
        .filter(|secret| {
            !secret.password.is_empty()
                && secret
                    .username
                    .trim_start_matches("+86")
                    .eq_ignore_ascii_case(username.trim_start_matches("+86"))
        })
}

fn official_origin(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some(HOST)
        && url.port_or_known_default() == Some(443)
}

fn window_error(error: impl std::fmt::Display) -> AppError {
    tracing::warn!(%error, "Failed to open Jiaoyue sign-in window");
    AppError::new(ErrorCode::SystemFailed, "无法打开皎月连签到窗口，请重试。")
}
