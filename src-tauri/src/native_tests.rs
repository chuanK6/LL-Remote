use crate::{
    engines::{sunshine::SunshineAdapter, EnginePaths},
    model::SunshineSettings,
    storage::Vault,
};
use std::path::PathBuf;

// Windows may reserve the ports around a randomly allocated socket. Sunshine
// needs a group of TCP/UDP ports, including RTSP; validate the entire group.
fn isolated_sunshine_web_port() -> u16 {
    for base in (20000u16..62000).step_by(37) {
        let tcp: Option<Vec<_>> = [-5i32, 0, 1, 21]
            .into_iter()
            .map(|offset| {
                std::net::TcpListener::bind(("0.0.0.0", (base as i32 + offset) as u16)).ok()
            })
            .collect();
        let udp: Option<Vec<_>> = [9u16, 10, 11, 13]
            .into_iter()
            .map(|offset| std::net::UdpSocket::bind(("0.0.0.0", base + offset)).ok())
            .collect();
        if tcp.is_some() && udp.is_some() {
            return base + 1;
        }
    }
    panic!("No isolated Sunshine port group is available");
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "Read-only verification of an existing LocalSystem Sunshine service from an ordinary user"]
async fn existing_sunshine_service_listener_and_account_are_recognized_without_credentials() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let data = tempfile::tempdir().unwrap();
    let adapter =
        SunshineAdapter::new(EnginePaths::open(root).unwrap(), data.path().into(), true).unwrap();
    assert!(adapter.external && adapter.running().await);
    let service = crate::platform::sunshine_service().unwrap();
    assert!(crate::platform::verify_listener(
        adapter.port().unwrap(),
        &service.sunshine_root.join("sunshine.exe")
    ));
    assert!(!crate::platform::verify_listener(
        adapter.port().unwrap(),
        &service.sunshine_root.join("tools/sunshinesvc.exe")
    ));
    let status = adapter.status(&Vault::new(data.path())).await;
    assert!(status.ready && status.account_initialized, "The live account in the existing service must be recognized without importing its password");
    assert!(!status.credential_valid);
}

#[tokio::test]
#[ignore = "Creates a browser-style account only in an isolated bundled Sunshine process"]
async fn account_created_outside_ll_and_unreadable_state_are_detected_from_the_live_api() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let data = tempfile::tempdir().unwrap();
    let adapter =
        SunshineAdapter::new(EnginePaths::open(root).unwrap(), data.path().into(), false).unwrap();
    let port = isolated_sunshine_web_port();
    let config_root = adapter.config_path.parent().unwrap();
    std::fs::create_dir_all(config_root.join("credentials")).unwrap();
    let certificate = config_root.join("credentials/cacert.pem");
    let config = format!("locale = zh\nport = {}\ncredentials_file = {}\nfile_state = {}\nfile_apps = {}\ncert = {}\npkey = {}\nlog_path = {}\n", port - 1, adapter.state_path.display(), adapter.state_path.display(), config_root.join("apps.json").display(), certificate.display(), config_root.join("credentials/cakey.pem").display(), config_root.join("sunshine.log").display());
    std::fs::write(&adapter.config_path, config).unwrap();
    std::fs::write(
        config_root.join("apps.json"),
        r#"{"env":{},"apps":[{"name":"Desktop"}]}"#,
    )
    .unwrap();
    let password = format!("Fixture-{}", uuid::Uuid::new_v4());
    let vault = Vault::new(data.path());
    let result = async {
        adapter.start().await?;
        let client = reqwest::Client::builder().no_proxy().danger_accept_invalid_hostnames(true)
            .add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(&certificate)?).unwrap()).build().unwrap();
        let response: serde_json::Value = client.post(format!("https://127.0.0.1:{port}/api/password"))
            .json(&serde_json::json!({"currentUsername":"", "currentPassword":"", "newUsername":"external-fixture", "newPassword":password, "confirmNewPassword":password}))
            .send().await.unwrap().json().await.unwrap();
        assert_eq!(response.get("status"), Some(&serde_json::Value::Bool(true)));
        // Sunshine keeps its live state and TLS certificate in memory. Hide only
        // this fixture's files to exercise the imported service/ACL fallback.
        std::fs::rename(&adapter.state_path, data.path().join("state-fixture.saved"))?;
        std::fs::rename(&certificate, data.path().join("certificate-fixture.saved"))?;
        assert!(!adapter.initialized());
        let status = adapter.status(&vault).await;
        assert!(status.ready && status.account_initialized);
        assert!(!status.credential_valid);
        adapter.setup("external-fixture", &password, "", "existing", &vault).await?;
        let verified = adapter.status(&vault).await;
        assert!(verified.credential_valid && verified.account_initialized);
        assert_eq!(verified.account_username.as_deref(), Some("external-fixture"));
        Ok::<_, crate::error::AppError>(())
    }.await;
    let _ = adapter.stop().await;
    result.unwrap();
}

#[tokio::test]
#[ignore = "Runs isolated loopback pairing, with optional native streaming via LLREMOTE_TEST_STREAM_CONTROLS"]
async fn isolated_loopback_pairing_uses_real_pending_pairing_api() {
    use crate::engines::{moonlight::MoonlightAdapter, sunshine::probe_host};
    use futures_util::FutureExt;
    use std::{panic::AssertUnwindSafe, time::Duration};
    use tokio_util::sync::CancellationToken;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture_root = root.join(".work");
    std::fs::create_dir_all(&fixture_root).unwrap();
    let data = tempfile::Builder::new()
        .prefix("pairing-fixture-")
        .tempdir_in(&fixture_root)
        .unwrap();
    let paths = EnginePaths::open(root).unwrap();
    let sunshine = SunshineAdapter::new(paths.clone(), data.path().into(), false).unwrap();
    let port = isolated_sunshine_web_port();
    let config_root = sunshine.config_path.parent().unwrap();
    std::fs::create_dir_all(config_root.join("credentials")).unwrap();
    let config = format!("locale = zh\nport = {}\ncredentials_file = {}\nfile_state = {}\nfile_apps = {}\ncert = {}\npkey = {}\nlog_path = {}\n", port - 1, sunshine.state_path.display(), sunshine.state_path.display(), config_root.join("apps.json").display(), config_root.join("credentials/cacert.pem").display(), config_root.join("credentials/cakey.pem").display(), config_root.join("sunshine.log").display());
    std::fs::write(&sunshine.config_path, config).unwrap();
    std::fs::write(
        config_root.join("apps.json"),
        r#"{"env":{},"apps":[{"name":"Desktop"}]}"#,
    )
    .unwrap();
    let vault = Vault::new(data.path());
    let password = format!("Fixture-{}", uuid::Uuid::new_v4());
    let moonlight = MoonlightAdapter::new(paths, Some(data.path().join("Moonlight")));
    let cancel = CancellationToken::new();
    let result = AssertUnwindSafe(async {
        sunshine
            .setup("ll-pair-test", &password, &password, "create", &vault)
            .await?;
        sunshine.start().await?;
        let address = format!("127.0.0.1:{}", port - 1);
        probe_host(&address, None, &cancel).await?;
        let pairing = {
            let moonlight = moonlight.clone();
            let address = address.clone();
            let token = cancel.clone();
            tokio::spawn(async move { moonlight.pair(&address, None, "4827", &token).await })
        };
        let mut pairing_id = None;
        for _ in 0..60 {
            let status = sunshine.status(&vault).await;
            if let Some(request) = status.pending_pairings.first() {
                pairing_id = Some(request.id.clone());
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let id = pairing_id.expect("The native pairing request must appear before confirming PIN");
        sunshine.confirm_pin("4827", "", &id, &vault).await?;
        tokio::time::timeout(Duration::from_secs(45), pairing)
            .await
            .unwrap()
            .unwrap()?;
        let restarted_moonlight =
            MoonlightAdapter::new(moonlight.paths.clone(), Some(data.path().join("Moonlight")));
        assert!(matches!(
            restarted_moonlight.list_apps(&address, None, &cancel).await?,
            crate::engines::moonlight::AppList::Paired(_)
        ));
        #[cfg(windows)]
        if std::env::var_os("LLREMOTE_TEST_STREAM_CONTROLS").is_some() {
            check_stream_controls(&restarted_moonlight, &address).await?;
            sunshine.stop_on_exit(false).await?;
            assert!(!sunshine.running().await);
        }
        Ok::<_, crate::error::AppError>(())
    })
    .catch_unwind()
    .await;
    cancel.cancel();
    let _ = sunshine.stop().await;
    if !matches!(&result, Ok(Ok(()))) {
        eprintln!(
            "Isolated pairing fixture retained: {}",
            data.keep().display()
        );
    }
    match result {
        Ok(result) => result.unwrap(),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(windows)]
async fn check_stream_controls(
    moonlight: &crate::engines::moonlight::MoonlightAdapter,
    address: &str,
) -> crate::error::Result<()> {
    use crate::model::StreamPreferences;
    use serde_json::json;
    use std::{sync::atomic::Ordering, time::Duration};
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowLongW, GetWindowThreadProcessId, GWL_STYLE,
            WS_CAPTION,
        },
    };
    unsafe extern "system" fn inspect(window: HWND, state: LPARAM) -> i32 {
        let state = &mut *(state as *mut (u32, Option<bool>));
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        let mut class = [0u16; 32];
        let len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        if pid == state.0 && len == 7 && class[..7].iter().copied().eq("SDL_app".encode_utf16()) {
            state.1 = Some(GetWindowLongW(window, GWL_STYLE) as u32 & WS_CAPTION == WS_CAPTION);
            return 0;
        }
        1
    }
    let preferences = StreamPreferences {
        width: 1280,
        height: 720,
        fps: 30,
        bitrate_kbps: 5000,
        window_mode: "windowed".into(),
        ..Default::default()
    };
    let pipe = format!("LLRemote-{}", uuid::Uuid::new_v4());
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let mut stream = moonlight.spawn_stream(address, None, "Desktop", &preferences, &pipe, &token)?;
    let pid = stream.child.id().unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        while !stream.verified.load(Ordering::SeqCst) {
            assert!(
                stream.child.try_wait().unwrap().is_none(),
                "Moonlight exited before displaying video"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("A frame must render before testing controls");
    moonlight
        .ipc(&pipe, &token, "input", json!({ "suspended": true }), pid)
        .await?;
    for (mode, caption) in [
        ("borderless", false),
        ("windowed", true),
        ("fullscreen", false),
        ("windowed", true),
    ] {
        moonlight
            .ipc(
                &pipe,
                &token,
                "apply",
                json!({ "patch": { "kind": "window" }, "preferences": { "windowMode": mode } }),
                pid,
            )
            .await?;
        let mut state = (pid, None::<bool>);
        unsafe {
            EnumWindows(Some(inspect), &mut state as *mut _ as isize);
        }
        assert_eq!(state.1, Some(caption), "Native window style for {mode}");
    }
    for mode in ["game", "office"] {
        moonlight
            .ipc(
                &pipe,
                &token,
                "apply",
                json!({ "patch": { "kind": "mouse" }, "preferences": { "mouseMode": mode } }),
                pid,
            )
            .await?;
    }
    moonlight
        .ipc(&pipe, &token, "disconnect", json!({}), pid)
        .await?;
    let exit = tokio::time::timeout(Duration::from_secs(10), stream.child.wait())
        .await
        .expect("Moonlight must exit after disconnect")?;
    assert!(exit.success());
    Ok(())
}

#[tokio::test]
#[ignore = "Runs only against a fresh isolated data directory and bundled local Sunshine"]
async fn isolated_sunshine_account_create_verify_and_config() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let data = tempfile::Builder::new()
        .prefix("Sunshine 中文 路径-")
        .tempdir()
        .unwrap();
    let adapter =
        SunshineAdapter::new(EnginePaths::open(root).unwrap(), data.path().into(), false).unwrap();
    let port = isolated_sunshine_web_port();
    std::fs::create_dir_all(adapter.config_path.parent().unwrap().join("credentials")).unwrap();
    let config = format!("locale = zh\nport = {}\ncredentials_file = {}\nfile_state = {}\nfile_apps = {}\ncert = {}\npkey = {}\nlog_path = {}\n", port - 1, adapter.state_path.display(), adapter.state_path.display(), adapter.config_path.parent().unwrap().join("apps.json").display(), adapter.config_path.parent().unwrap().join("credentials/cacert.pem").display(), adapter.config_path.parent().unwrap().join("credentials/cakey.pem").display(), adapter.config_path.parent().unwrap().join("sunshine.log").display());
    std::fs::write(&adapter.config_path, config).unwrap();
    std::fs::write(
        adapter.config_path.parent().unwrap().join("apps.json"),
        r#"{"env":{},"apps":[{"name":"Desktop"}]}"#,
    )
    .unwrap();
    let vault = Vault::new(data.path());
    let password = format!("Fixture-{}", uuid::Uuid::new_v4());
    let result = async {
        adapter
            .setup("ll-native-test", &password, &password, "create", &vault)
            .await?;
        assert!(adapter.initialized());
        assert!(
            !adapter.running().await,
            "Controller-only account initialization must stop its temporary process"
        );
        let started = std::time::Instant::now();
        adapter
            .setup("ll-native-test", &password, "", "existing", &vault)
            .await?;
        eprintln!(
            "Stopped Sunshine account verification: {:?}",
            started.elapsed()
        );
        assert!(!adapter.running().await);
        assert_eq!(
            adapter
                .setup("ll-native-test", "incorrect", "", "existing", &vault)
                .await
                .unwrap_err()
                .code,
            crate::error::ErrorCode::SunshineAccountRequired
        );
        assert!(!adapter.running().await);
        adapter.start().await?;
        let status = adapter.status(&vault).await;
        assert!(status.ready && status.credential_valid);
        assert_eq!(status.locale, "zh");
        adapter
            .save_settings(&SunshineSettings::default(), &vault)
            .await?;
        Ok::<_, crate::error::AppError>(())
    }
    .await;
    let _ = adapter.stop().await;
    result.unwrap();
}
