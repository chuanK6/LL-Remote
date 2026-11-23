use crate::{
    error::{AppError, ErrorCode},
    model::{Device, HistoryEntry, SessionPatch, StreamPreferences},
    storage::{SecretRecord, Store, Vault},
};

#[test]
fn native_errors_stop_retries_for_password_and_keep_unknown_network_distinct() {
    let password = AppError::native_info("连接密码错误", false);
    assert_eq!(password.code, ErrorCode::PasswordRejected);
    assert!(!password.retryable);
    assert!(password.reauthenticate);
    let network = AppError::native_info("连接失败", false);
    assert_eq!(network.code, ErrorCode::NetworkFailed);
    assert!(network.retryable);
    assert_ne!(network.code, ErrorCode::QuotaInsufficient);
    assert_eq!(
        AppError::native_info("服务已到期", false).code,
        ErrorCode::ServiceExpired
    );
}

#[test]
fn confirming_both_native_ids_merges_cached_and_online_rows_without_losing_edits() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    let existing = Device {
        id: "existing".into(),
        uuid: Some("host-uuid".into()),
        host_name: "Original".into(),
        display_name: "我的电脑".into(),
        favorite: true,
        wallpaper: Some("wallpapers/saved.png".into()),
        preferences: StreamPreferences {
            bitrate_kbps: 42000,
            ..StreamPreferences::default()
        },
        ..Device::default()
    };
    let account = Device {
        id: "account".into(),
        nat_id: Some("nat-host-id".into()),
        host_name: "Original".into(),
        display_name: "Original".into(),
        source: "account".into(),
        favorite: true,
        credential_ref: Some("directory-credential".into()),
        credential_state: "valid".into(),
        ..Device::default()
    };
    store.merge_device(existing);
    store.merge_device(account);
    store.account_data.history.push(HistoryEntry {
        id: "old".into(),
        device_id: Some("account".into()),
        peer_code: None,
        connected_at: "2026-11-20T12:00:00Z".into(),
        method: "同账号".into(),
        result: "success".into(),
        error_code: None,
    });
    let incoming = Device {
        uuid: Some("host-uuid".into()),
        nat_id: Some("nat-host-id".into()),
        host_name: "Original".into(),
        display_name: "Original".into(),
        ..Device::default()
    };
    let id = store.merge_device(incoming);
    assert_eq!(id, "existing");
    assert_eq!(store.account_data.devices.len(), 1);
    let merged = &store.account_data.devices[0];
    assert_eq!(merged.display_name, "我的电脑");
    assert!(merged.favorite);
    assert_eq!(merged.wallpaper.as_deref(), Some("wallpapers/saved.png"));
    assert_eq!(merged.preferences.bitrate_kbps, 42000);
    assert_eq!(
        store.account_data.history[0].device_id.as_deref(),
        Some(id.as_str())
    );
}

#[test]
fn repeated_connections_with_new_native_ids_update_one_confirmed_device() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    let first = Device {
        id: "saved-device".into(),
        uuid: Some("host-uuid".into()),
        nat_id: Some("native-session-one".into()),
        peer_code: Some("100200".into()),
        host_name: "Desktop".into(),
        display_name: "工作电脑".into(),
        ..Device::default()
    };
    let original_id = store.merge_device(first);
    for index in 2..=4 {
        let id = store.merge_device(Device {
            uuid: Some(" HOST-UUID ".into()),
            nat_id: Some(format!("native-session-{index}")),
            peer_code: Some("100200".into()),
            host_name: "Desktop".into(),
            display_name: "Desktop".into(),
            last_connected_at: Some(format!("2026-11-20T12:00:0{index}Z")),
            ..Device::default()
        });
        assert_eq!(id, original_id);
    }
    assert_eq!(store.account_data.devices.len(), 1);
    let device = &store.account_data.devices[0];
    assert_eq!(device.display_name, "工作电脑");
    assert_eq!(device.nat_id.as_deref(), Some("native-session-4"));
    assert_eq!(
        device.last_connected_at.as_deref(),
        Some("2026-11-20T12:00:04Z")
    );
}

#[test]
fn same_second_directory_updates_refresh_status_and_keep_connection_time() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    store.merge_device(Device {
        uuid: Some("host-one".into()),
        nat_id: Some("native-host".into()),
        host_name: "Desktop".into(),
        display_name: "Desktop".into(),
        status: "offline".into(),
        observed_at: Some("2026-11-20T12:00:01Z".into()),
        last_connected_at: Some("2026-11-20T12:00:01Z".into()),
        ..Device::default()
    });
    store.merge_device(Device {
        uuid: Some("host-one".into()),
        nat_id: Some("native-host".into()),
        host_name: "Renamed Desktop".into(),
        display_name: "Renamed Desktop".into(),
        status: "online".into(),
        service_ready: Some(true),
        observed_at: Some("2026-11-20T12:00:01Z".into()),
        ..Device::default()
    });
    let device = &store.account_data.devices[0];
    assert_eq!(store.account_data.devices.len(), 1);
    assert_eq!(device.status, "online");
    assert_eq!(device.service_ready, Some(true));
    assert_eq!(device.host_name, "Renamed Desktop");
    assert_eq!(device.display_name, "Renamed Desktop");
    assert_eq!(
        device.last_connected_at.as_deref(),
        Some("2026-11-20T12:00:01Z")
    );
}

#[test]
fn verified_uuid_conflicts_keep_same_account_devices_separate() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    for uuid in ["host-one", "host-two"] {
        store.merge_device(Device {
            uuid: Some(uuid.into()),
            nat_id: Some("reused-native-id".into()),
            peer_code: Some("100200".into()),
            host_name: "Desktop".into(),
            display_name: "Desktop".into(),
            address: Some("10.0.0.1".into()),
            ..Device::default()
        });
    }
    assert_eq!(store.account_data.devices.len(), 2);
    assert!(store
        .account_data
        .devices
        .iter()
        .any(|device| device.uuid.as_deref() == Some("host-one")));
    assert!(store
        .account_data
        .devices
        .iter()
        .any(|device| device.uuid.as_deref() == Some("host-two")));
    for _ in 0..2 {
        store.merge_device(Device {
            nat_id: Some("reused-native-id".into()),
            host_name: "Desktop".into(),
            display_name: "Desktop".into(),
            source: "account".into(),
            ..Device::default()
        });
    }
    assert_eq!(store.account_data.devices.len(), 3);
    assert!(store
        .account_data
        .devices
        .iter()
        .any(|device| device.uuid.as_deref() == Some("host-one")));
    assert!(store
        .account_data
        .devices
        .iter()
        .any(|device| device.uuid.as_deref() == Some("host-two")));
    let mut legacy_rows = store.account_data.devices.clone();
    legacy_rows.sort_by_key(|device| device.uuid.is_some());
    store.account_data.devices = legacy_rows;
    store.save_devices().unwrap();
    let mut reopened = Store::open(root.path().into()).unwrap();
    reopened.switch_account(Some("account-a")).unwrap();
    assert_eq!(reopened.account_data.devices.len(), 3);
    assert_eq!(
        reopened
            .account_data
            .devices
            .iter()
            .filter(|device| device.uuid.is_some())
            .count(),
        2
    );
}

#[test]
fn code_connection_confirms_a_saved_device_despite_a_changed_native_id() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    let saved_id = store.merge_device(Device {
        peer_code: Some("100200".into()),
        nat_id: Some("old-native-id".into()),
        display_name: "我的电脑".into(),
        ..Device::default()
    });
    let confirmed_id = store.merge_device(Device {
        peer_code: Some("100200".into()),
        nat_id: Some("new-native-id".into()),
        uuid: Some("confirmed-host".into()),
        host_name: "Desktop".into(),
        display_name: "Desktop".into(),
        ..Device::default()
    });
    assert_eq!(confirmed_id, saved_id);
    assert_eq!(store.account_data.devices.len(), 1);
    assert_eq!(
        store.account_data.devices[0].uuid.as_deref(),
        Some("confirmed-host")
    );
    assert_eq!(store.account_data.devices[0].display_name, "我的电脑");
    store.save_devices().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    for (nat_id, status) in [("old-native-id", "offline"), ("new-native-id", "online")] {
        assert_eq!(
            store.merge_device(Device {
                nat_id: Some(nat_id.into()),
                status: status.into(),
                source: "account".into(),
                ..Device::default()
            }),
            saved_id
        );
    }
    assert_eq!(store.account_data.devices.len(), 1);
    assert_eq!(
        store.account_data.devices[0].nat_id.as_deref(),
        Some("new-native-id")
    );
    assert_eq!(store.account_data.devices[0].status, "online");
}

#[test]
fn deletion_removes_the_local_device_record() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    let id = store.merge_device(Device {
        uuid: Some("verified".into()),
        nat_id: Some("current".into()),
        known_nat_ids: vec!["previous".into()],
        ..Device::default()
    });
    store.remove_device(&id).unwrap();

    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    assert!(store.account_data.devices.is_empty());
}

#[test]
fn associating_a_selected_device_keeps_its_id_and_merges_directory_history() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    store.merge_device(Device {
        id: "selected-device".into(),
        uuid: Some("host-one".into()),
        host_name: "Desktop".into(),
        display_name: "Desktop".into(),
        ..Device::default()
    });
    store.merge_device(Device {
        id: "directory-device".into(),
        nat_id: Some("native-host".into()),
        host_name: "Desktop".into(),
        display_name: "Desktop".into(),
        favorite: true,
        credential_ref: Some("saved-credential".into()),
        credential_state: "valid".into(),
        ..Device::default()
    });
    store.account_data.history.push(HistoryEntry {
        id: "directory-history".into(),
        device_id: Some("directory-device".into()),
        peer_code: None,
        connected_at: "2026-11-20T12:00:01Z".into(),
        method: "同账号".into(),
        result: "success".into(),
        error_code: None,
    });
    let mut selected = store.device("selected-device").unwrap();
    selected.nat_id = Some("native-host".into());
    let merged_id = store.merge_device(selected);
    assert_eq!(merged_id, "selected-device");
    assert_eq!(store.account_data.devices.len(), 1);
    assert!(store.account_data.devices[0].favorite);
    assert_eq!(
        store.account_data.devices[0].credential_ref.as_deref(),
        Some("saved-credential")
    );
    assert_eq!(store.account_data.devices[0].credential_state, "valid");
    assert_eq!(
        store.account_data.history[0].device_id.as_deref(),
        Some("selected-device")
    );
}

#[test]
fn restarting_consolidates_legacy_duplicates_and_keeps_edits_and_history() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    let edited = Device {
        id: "edited-device".into(),
        uuid: Some("host-one".into()),
        host_name: "Desktop".into(),
        display_name: "家里的电脑".into(),
        favorite: true,
        wallpaper: Some("wallpapers/saved.png".into()),
        note: "保存的备注".into(),
        preferences: StreamPreferences {
            bitrate_kbps: 42000,
            ..StreamPreferences::default()
        },
        quick_apps: vec!["Steam".into()],
        credential_ref: Some("old-invalid-credential".into()),
        credential_state: "invalid".into(),
        observed_at: Some("2026-11-20T12:00:01Z".into()),
        last_connected_at: Some("2026-11-20T12:00:01Z".into()),
        ..Device::default()
    };
    let duplicate = Device {
        id: "duplicate-device".into(),
        uuid: Some("HOST-ONE".into()),
        nat_id: Some("latest-native-id".into()),
        peer_code: Some("100200".into()),
        host_name: "Desktop".into(),
        display_name: "Desktop".into(),
        status: "online".into(),
        network_connected: true,
        service_ready: Some(true),
        address: Some("10.0.0.2".into()),
        credential_ref: Some("saved-credential".into()),
        credential_state: "valid".into(),
        observed_at: Some("2026-11-20T12:00:02Z".into()),
        last_connected_at: Some("2026-11-20T12:00:02Z".into()),
        ..Device::default()
    };
    let stale = Device {
        id: "stale-device".into(),
        uuid: Some("host-one".into()),
        nat_id: Some("stale-native-id".into()),
        address: Some("10.0.0.3".into()),
        host_name: "Desktop".into(),
        display_name: "Desktop".into(),
        credential_ref: Some("stale-invalid-credential".into()),
        credential_state: "invalid".into(),
        observed_at: Some("2026-11-20T12:00:00Z".into()),
        last_connected_at: Some("2026-11-20T12:00:00Z".into()),
        ..Device::default()
    };
    let other = Device {
        id: "other-device".into(),
        uuid: Some("host-two".into()),
        host_name: "Desktop".into(),
        display_name: "Desktop".into(),
        ..Device::default()
    };
    store.account_data.devices = vec![edited, duplicate, stale, other];
    for id in ["edited-device", "duplicate-device"] {
        store.account_data.history.push(HistoryEntry {
            id: format!("history-{id}"),
            device_id: Some(id.into()),
            peer_code: Some("100200".into()),
            connected_at: "2026-11-20T12:00:01Z".into(),
            method: "识别码".into(),
            result: "success".into(),
            error_code: None,
        });
    }
    store.save_devices().unwrap();
    let mut reopened = Store::open(root.path().into()).unwrap();
    reopened.switch_account(Some("account-a")).unwrap();
    assert_eq!(reopened.account_data.devices.len(), 2);
    let merged = reopened
        .account_data
        .devices
        .iter()
        .find(|device| device.peer_code.is_some())
        .unwrap();
    assert_eq!(merged.display_name, "家里的电脑");
    assert!(merged.favorite);
    assert_eq!(merged.note, "保存的备注");
    assert_eq!(merged.wallpaper.as_deref(), Some("wallpapers/saved.png"));
    assert_eq!(merged.preferences.bitrate_kbps, 42000);
    assert_eq!(merged.quick_apps, ["Steam"]);
    assert_eq!(merged.nat_id.as_deref(), Some("latest-native-id"));
    assert_eq!(merged.address.as_deref(), Some("10.0.0.2"));
    assert_eq!(merged.credential_ref.as_deref(), Some("saved-credential"));
    assert_eq!(merged.credential_state, "valid");
    assert_eq!(
        merged.last_connected_at.as_deref(),
        Some("2026-11-20T12:00:02Z")
    );
    assert_eq!(merged.status, "unknown");
    assert!(!merged.network_connected);
    assert_eq!(merged.service_ready, None);
    assert!(reopened
        .account_data
        .history
        .iter()
        .all(|entry| entry.device_id.as_deref() == Some(merged.id.as_str())));
    let mut persisted = Store::open(root.path().into()).unwrap();
    persisted.switch_account(Some("account-a")).unwrap();
    assert_eq!(persisted.account_data.devices.len(), 2);
    assert_eq!(persisted.account_data.history.len(), 2);
}

#[test]
fn account_switch_and_restart_do_not_reuse_another_accounts_devices_or_cached_online_status() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.switch_account(Some("account-a")).unwrap();
    store.merge_device(Device {
        uuid: Some("host-a".into()),
        status: "online".into(),
        service_ready: Some(true),
        ..Device::default()
    });
    store.save_devices().unwrap();
    store.switch_account(Some("account-b")).unwrap();
    assert!(store.account_data.devices.is_empty());
    store.switch_account(Some("account-a")).unwrap();
    assert_eq!(store.account_data.devices[0].status, "unknown");
    assert_eq!(store.account_data.devices[0].service_ready, None);
    assert_ne!(
        Vault::reference("account-a", "host-a"),
        Vault::reference("account-b", "host-a")
    );
}

#[test]
fn atomic_configuration_keeps_a_recoverable_previous_version() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path().into()).unwrap();
    store.save_preferences().unwrap();
    store.preferences.auto_retry = false;
    store.save_preferences().unwrap();
    std::fs::write(root.path().join("preferences.json"), b"corrupted").unwrap();
    let recovered = Store::open(root.path().into()).unwrap();
    assert!(recovered.preferences.auto_retry);
    assert!(recovered.warning.is_some());
}

#[test]
fn mouse_and_audio_patches_keep_the_complete_quality_and_window_snapshot() {
    let original = StreamPreferences {
        width: 2560,
        height: 1440,
        fps: 120,
        bitrate_kbps: 42000,
        window_mode: "borderless".into(),
        ..StreamPreferences::default()
    };
    let mouse = original
        .patched(&SessionPatch::Mouse {
            mode: "game".into(),
        })
        .unwrap();
    assert_eq!(
        (mouse.width, mouse.height, mouse.fps, mouse.bitrate_kbps),
        (2560, 1440, 120, 42000)
    );
    assert_eq!(mouse.window_mode, original.window_mode);
    let audio = mouse
        .patched(&SessionPatch::Audio {
            volume: 30,
            muted: true,
            device: String::new(),
            host_audio: false,
        })
        .unwrap();
    assert_eq!(
        (audio.width, audio.height, audio.fps, audio.bitrate_kbps),
        (2560, 1440, 120, 42000)
    );
    assert_eq!(audio.mouse_mode, "game");
    assert_eq!(audio.window_mode, "borderless");
}

#[cfg(windows)]
#[test]
fn dpapi_roundtrip_is_persistent_and_does_not_store_the_password_as_plaintext() {
    let root = tempfile::tempdir().unwrap();
    let reference = Vault::reference("account-a", "host-a");
    let vault = Vault::new(root.path());
    vault
        .put(
            &reference,
            &SecretRecord {
                username: "test".into(),
                password: "test-secret-never-in-json".into(),
            },
        )
        .unwrap();
    let blob = std::fs::read(
        root.path()
            .join("credentials")
            .join(format!("{reference}.bin")),
    )
    .unwrap();
    assert!(!String::from_utf8_lossy(&blob).contains("test-secret-never-in-json"));
    let reopened = Vault::new(root.path());
    assert_eq!(
        reopened.get(&reference).unwrap().password,
        "test-secret-never-in-json"
    );
    assert!(reopened.get("../../credentials.bin").is_err());
}
