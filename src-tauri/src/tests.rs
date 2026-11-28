use crate::{
    error::{AppError, ErrorCode},
    model::{Device, DeviceEdit, HistoryEntry, SessionPatch, StreamPreferences},
    storage::{SecretRecord, Store, Vault},
};

fn wallpaper_fixture() -> (tempfile::TempDir, Store, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.png");
    std::fs::write(&source, b"\x89PNG\r\n\x1a\nimage fixture").unwrap();
    let mut store = Store::open(root.path().join("Data")).unwrap();
    store.switch_account(Some("wallpaper-account")).unwrap();
    store.account_data.devices.push(Device {
        id: "device".into(),
        ..Device::default()
    });
    store
        .edit_device(&wallpaper_edit(Some(&source), false))
        .unwrap();
    (root, store, source)
}

fn wallpaper_edit(path: Option<&std::path::Path>, reset: bool) -> DeviceEdit {
    DeviceEdit {
        device_id: "device".into(),
        display_name: "电脑".into(),
        favorite: false,
        note: String::new(),
        wallpaper_path: path.map(|path| path.to_string_lossy().into()),
        reset_wallpaper: reset,
    }
}

#[test]
fn wallpaper_reset_deletes_managed_copy_and_keeps_source_and_default() {
    let (_root, mut store, source) = wallpaper_fixture();
    let previous = store
        .root
        .join(store.device("device").unwrap().wallpaper.unwrap());
    let default = store.root.join("wallpapers/default.svg");
    std::fs::write(&default, b"<svg>default</svg>").unwrap();
    store.edit_device(&wallpaper_edit(None, true)).unwrap();
    assert!(!previous.exists());
    assert!(source.exists());
    assert_eq!(std::fs::read(&default).unwrap(), b"<svg>default</svg>");
    assert!(store.device("device").unwrap().wallpaper.is_none());
    let mut reopened = Store::open(store.root.clone()).unwrap();
    reopened.switch_account(Some("wallpaper-account")).unwrap();
    assert!(reopened.device("device").unwrap().wallpaper.is_none());
}

#[test]
fn wallpaper_replacement_deletes_previous_and_persists_new_copy() {
    let (root, mut store, source) = wallpaper_fixture();
    let previous = store
        .root
        .join(store.device("device").unwrap().wallpaper.unwrap());
    let replacement = root.path().join("replacement.webp");
    let bytes = b"RIFF1234WEBPnew fixture";
    std::fs::write(&replacement, bytes).unwrap();
    store
        .edit_device(&wallpaper_edit(Some(&replacement), false))
        .unwrap();
    let relative = store.device("device").unwrap().wallpaper.unwrap();
    assert!(!previous.exists());
    assert_eq!(std::fs::read(store.root.join(&relative)).unwrap(), bytes);
    assert!(source.exists() && replacement.exists());
    assert_eq!(
        std::fs::read_dir(store.root.join("wallpapers"))
            .unwrap()
            .count(),
        1
    );
    let mut reopened = Store::open(store.root.clone()).unwrap();
    reopened.switch_account(Some("wallpaper-account")).unwrap();
    assert_eq!(
        reopened.device("device").unwrap().wallpaper.as_deref(),
        Some(relative.as_str())
    );
}

#[test]
fn wallpaper_reselecting_managed_copy_reads_before_deleting_previous() {
    let (_root, mut store, _source) = wallpaper_fixture();
    let previous = store
        .root
        .join(store.device("device").unwrap().wallpaper.unwrap());
    let bytes = std::fs::read(&previous).unwrap();
    store
        .edit_device(&wallpaper_edit(Some(&previous), false))
        .unwrap();
    let current = store
        .root
        .join(store.device("device").unwrap().wallpaper.unwrap());
    assert_ne!(current, previous);
    assert!(!previous.exists());
    assert_eq!(std::fs::read(current).unwrap(), bytes);
}

#[test]
fn wallpaper_metadata_edit_and_rejected_upload_keep_previous() {
    let (root, mut store, _source) = wallpaper_fixture();
    let relative = store.device("device").unwrap().wallpaper.unwrap();
    let previous = store.root.join(&relative);
    store.edit_device(&wallpaper_edit(None, false)).unwrap();
    let oversized = root.path().join("oversized.png");
    std::fs::File::create(&oversized)
        .unwrap()
        .set_len(5 * 1024 * 1024 + 1)
        .unwrap();
    assert!(store
        .edit_device(&wallpaper_edit(Some(&oversized), false))
        .is_err());
    assert_eq!(
        store.device("device").unwrap().wallpaper.as_deref(),
        Some(relative.as_str())
    );
    assert!(previous.exists());
    assert_eq!(
        std::fs::read_dir(store.root.join("wallpapers"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn wallpaper_save_failure_keeps_previous_and_removes_uncommitted_import() {
    let (_root, mut store, source) = wallpaper_fixture();
    let relative = store.device("device").unwrap().wallpaper.unwrap();
    let backup = store
        .root
        .join("accounts")
        .join(&store.account_scope)
        .join("devices.bak");
    std::fs::create_dir(&backup).unwrap();
    assert!(store
        .edit_device(&wallpaper_edit(Some(&source), false))
        .is_err());
    assert_eq!(
        store.device("device").unwrap().wallpaper.as_deref(),
        Some(relative.as_str())
    );
    assert!(store.root.join(relative).exists());
    assert_eq!(
        std::fs::read_dir(store.root.join("wallpapers"))
            .unwrap()
            .count(),
        1
    );
}

#[cfg(windows)]
#[test]
fn wallpaper_delete_failure_rolls_back_and_does_not_leave_new_import() {
    let (_root, mut store, source) = wallpaper_fixture();
    let relative = store.device("device").unwrap().wallpaper.unwrap();
    let previous = store.root.join(&relative);
    use std::os::windows::fs::OpenOptionsExt;
    // Deny FILE_SHARE_DELETE to model another application holding the image.
    // Modern Rust/Windows can delete readonly files, so readonly is not a lock.
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&previous)
        .unwrap();
    let result = store.edit_device(&wallpaper_edit(Some(&source), false));
    drop(held);
    assert!(result.unwrap_err().message.contains("旧壁纸无法删除"));
    assert_eq!(
        store.device("device").unwrap().wallpaper.as_deref(),
        Some(relative.as_str())
    );
    assert!(previous.exists());
    assert_eq!(
        std::fs::read_dir(store.root.join("wallpapers"))
            .unwrap()
            .count(),
        1
    );
    let mut reopened = Store::open(store.root.clone()).unwrap();
    reopened.switch_account(Some("wallpaper-account")).unwrap();
    assert_eq!(
        reopened.device("device").unwrap().wallpaper.as_deref(),
        Some(relative.as_str())
    );
}

#[test]
fn wallpaper_reset_never_deletes_svg_or_outside_assets() {
    let (root, mut store, _source) = wallpaper_fixture();
    for relative in ["wallpapers/default.svg", "assets/default.png"] {
        let path = store.root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"bundled default fixture").unwrap();
        store.account_data.devices[0].wallpaper = Some(relative.into());
        store.edit_device(&wallpaper_edit(None, true)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"bundled default fixture");
    }
    let outside = root.path().join("outside.png");
    std::fs::write(&outside, b"outside fixture").unwrap();
    store.account_data.devices[0].wallpaper = Some(outside.to_string_lossy().into());
    store.edit_device(&wallpaper_edit(None, true)).unwrap();
    assert!(outside.exists());
}

#[test]
fn wallpaper_shared_across_accounts_deleted_only_after_final_reference() {
    let (_root, mut store, _source) = wallpaper_fixture();
    let relative = store.device("device").unwrap().wallpaper.unwrap();
    let previous = store.root.join(&relative);
    let mut other = Store::open(store.root.clone()).unwrap();
    other.switch_account(Some("other-account")).unwrap();
    other.account_data.devices.push(Device {
        id: "device".into(),
        wallpaper: Some(relative.clone()),
        ..Device::default()
    });
    other.save_devices().unwrap();
    store.edit_device(&wallpaper_edit(None, true)).unwrap();
    assert!(previous.exists());
    other.edit_device(&wallpaper_edit(None, true)).unwrap();
    assert!(!previous.exists());
}

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

fn account_directory_device(nat_id: &str, name: &str, uuid: Option<&str>) -> Device {
    Device {
        nat_id: Some(nat_id.into()),
        account_host_name: Some(name.into()),
        host_name: name.into(),
        display_name: name.into(),
        uuid: uuid.map(str::to_owned),
        source: "account".into(),
        status: "online".into(),
        observed_at: Some("2026-11-25T12:00:00Z".into()),
        ..Device::default()
    }
}

fn saved_device_history(id: &str, device_id: &str) -> HistoryEntry {
    HistoryEntry {
        id: id.into(), device_id: Some(device_id.into()), peer_code: None,
        connected_at: "2026-11-25T11:00:00Z".into(), method: "同账号".into(),
        result: "success".into(), error_code: None,
    }
}

#[test]
fn account_reconnect_preserves_wallpaper_and_all_history_and_favorite_views_share_edits() {
    let (_root, mut store, source) = wallpaper_fixture();
    let saved = &mut store.account_data.devices[0];
    saved.source = "account".into();
    saved.nat_id = Some("old-directory-id".into());
    saved.host_name = "Desktop".into();
    saved.favorite = true;
    for index in 0..3 {
        store.account_data.history.push(saved_device_history(&format!("history-{index}"), "device"));
    }
    let original = store.device("device").unwrap().wallpaper.unwrap();
    store.merge_account_directory(Vec::new());
    assert!(!store.device("device").unwrap().account_online);
    store.merge_account_directory(vec![account_directory_device("new-directory-id", "Desktop", None)]);
    assert_eq!(store.account_data.devices.len(), 1);
    let saved = store.device("device").unwrap();
    assert!(saved.account_online && saved.favorite);
    assert_eq!(saved.wallpaper.as_deref(), Some(original.as_str()));
    assert!(store.root.join(&original).exists());
    store.save_devices().unwrap();
    let mut store = Store::open(store.root.clone()).unwrap();
    store.switch_account(Some("wallpaper-account")).unwrap();
    store.merge_account_directory(vec![account_directory_device("third-directory-id", "Desktop", Some("host-uuid"))]);
    let mut edit = wallpaper_edit(Some(&source), false);
    edit.favorite = true;
    store.edit_device(&edit).unwrap();
    let replacement = store.device("device").unwrap().wallpaper.unwrap();
    assert_ne!(original, replacement);
    assert!(!store.root.join(&original).exists());
    assert_eq!(store.account_data.devices.iter().filter(|device| device.favorite).count(), 1);
    for entry in &store.account_data.history {
        let device = store.device(entry.device_id.as_deref().unwrap()).unwrap();
        assert_eq!(device.id, "device");
        assert_eq!(device.wallpaper.as_deref(), Some(replacement.as_str()));
    }
    edit.wallpaper_path = None;
    edit.reset_wallpaper = true;
    store.edit_device(&edit).unwrap();
    for entry in &store.account_data.history {
        assert!(store.device(entry.device_id.as_deref().unwrap()).unwrap().wallpaper.is_none());
    }
    assert!(store.device("device").unwrap().favorite);
}

#[test]
fn account_directory_recovers_wallpaper_from_old_unverified_profile_and_rebinds_history() {
    let (_root, mut store, _source) = wallpaper_fixture();
    let saved = &mut store.account_data.devices[0];
    saved.source = "account".into();
    saved.nat_id = Some("old-id".into());
    saved.host_name = "Desktop".into();
    saved.favorite = true;
    let wallpaper = saved.wallpaper.clone();
    let mut confirmed = account_directory_device("current-id", "Desktop", Some("confirmed-host"));
    confirmed.id = "confirmed-device".into();
    confirmed.credential_ref = Some("saved-credential".into());
    confirmed.credential_state = "valid".into();
    store.account_data.devices.push(confirmed);
    for id in ["device", "confirmed-device"] {
        store.account_data.history.push(saved_device_history(&format!("history-{id}"), id));
    }
    store.merge_account_directory(vec![account_directory_device("current-id", "Desktop", None)]);
    assert_eq!(store.account_data.devices.len(), 1);
    let saved = store.device("confirmed-device").unwrap();
    assert_eq!(saved.wallpaper, wallpaper);
    assert!(saved.favorite && saved.account_online);
    assert_eq!(saved.uuid.as_deref(), Some("confirmed-host"));
    assert_eq!(saved.credential_ref.as_deref(), Some("saved-credential"));
    assert!(store.account_data.history.iter().all(|entry| entry.device_id.as_deref() == Some("confirmed-device")));
}

#[test]
fn account_directory_keeps_ambiguous_names_and_conflicting_uuids_separate() {
    let (_root, mut store, _source) = wallpaper_fixture();
    let saved = &mut store.account_data.devices[0];
    saved.source = "account".into();
    saved.nat_id = Some("old-id".into());
    saved.host_name = "Desktop".into();
    store.merge_account_directory(vec![
        account_directory_device("first-id", "Desktop", None),
        account_directory_device("second-id", "Desktop", None),
    ]);
    assert_eq!(store.account_data.devices.len(), 3);
    assert!(!store.device("device").unwrap().account_online);
    assert!(store.account_data.devices.iter().filter(|device| device.account_online).all(|device| device.wallpaper.is_none()));

    store.account_data.devices.retain(|device| device.id == "device");
    store.account_data.devices[0].uuid = Some("original-host".into());
    store.merge_account_directory(vec![account_directory_device("other-id", "Desktop", Some("other-host"))]);
    assert_eq!(store.account_data.devices.len(), 2);
    assert_eq!(store.device("device").unwrap().uuid.as_deref(), Some("original-host"));
    assert!(store.device("device").unwrap().wallpaper.is_some());
    assert!(store.account_data.devices.iter().find(|device| device.account_online).unwrap().wallpaper.is_none());
}

#[test]
fn account_directory_does_not_take_profile_from_another_live_native_id() {
    let (_root, mut store, _source) = wallpaper_fixture();
    let saved = &mut store.account_data.devices[0];
    saved.source = "account".into();
    saved.nat_id = Some("old-id".into());
    saved.host_name = "Desktop".into();
    store.merge_account_directory(vec![
        account_directory_device("new-id", "Desktop", None),
        account_directory_device("old-id", "Renamed desktop", None),
    ]);
    assert_eq!(store.account_data.devices.len(), 2);
    assert!(store.device("device").unwrap().wallpaper.is_some());
    assert!(store.account_data.devices.iter().find(|device| device.nat_id.as_deref() == Some("new-id")).unwrap().wallpaper.is_none());
}

#[test]
fn account_directory_name_continuity_survives_sunshine_name_and_connection_source_changes() {
    let (_root, mut store, _source) = wallpaper_fixture();
    let saved = &mut store.account_data.devices[0];
    saved.source = "code".into();
    saved.nat_id = Some("old-id".into());
    saved.account_host_name = Some("Directory name".into());
    saved.host_name = "Sunshine name".into();
    saved.uuid = Some("confirmed-host".into());
    saved.peer_code = Some("12345678".into());
    store.merge_account_directory(vec![account_directory_device("new-id", "Directory name", None)]);
    assert_eq!(store.account_data.devices.len(), 1);
    let saved = store.device("device").unwrap();
    assert_eq!(saved.host_name, "Sunshine name");
    assert_eq!(saved.uuid.as_deref(), Some("confirmed-host"));
    assert_eq!(saved.peer_code.as_deref(), Some("12345678"));
    assert!(saved.wallpaper.is_some() && saved.account_online);
}

#[test]
fn renamed_online_host_keeps_its_note_and_saved_identity() {
    let (_root, mut store, _source) = wallpaper_fixture();
    let saved = &mut store.account_data.devices[0];
    saved.source = "account".into();
    saved.nat_id = Some("native-id".into());
    saved.account_host_name = Some("Old name".into());
    saved.host_name = "Sunshine name".into();
    saved.display_name = "Saved display name".into();
    saved.note = "Office".into();
    saved.uuid = Some("confirmed-host".into());

    store.merge_account_directory(vec![account_directory_device("native-id", "New name", None)]);
    let renamed = store.device("device").unwrap();
    assert_eq!(renamed.account_host_name.as_deref(), Some("New name"));
    assert_eq!(renamed.note, "Office");
    assert_eq!(renamed.display_name, "Saved display name");

    store.merge_account_directory(vec![
        account_directory_device("reconnected-id", "Latest name", Some("confirmed-host")),
        account_directory_device("another-id", "Latest name", Some("different-host")),
    ]);
    let renamed = store.device("device").unwrap();
    assert_eq!(renamed.account_host_name.as_deref(), Some("Latest name"));
    assert_eq!(renamed.note, "Office");
    assert!(store.account_data.devices.iter().find(|device| device.nat_id.as_deref() == Some("another-id")).unwrap().note.is_empty());
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
