use crate::{error::Result, model::StreamPreferences, storage::atomic_write};
use std::{collections::BTreeMap, fs, path::Path};

const REGISTRY_PATH: &str = "Software\\Moonlight Game Streaming Project\\Moonlight";

fn number(value: &str) -> Option<u32> {
    match value.trim().trim_matches('"') {
        "true" => Some(1),
        "false" => Some(0),
        value => value.parse().ok(),
    }
}

fn decode(
    base: &StreamPreferences,
    get: impl Fn(&str) -> Option<u32>,
) -> Option<StreamPreferences> {
    let mut preferences = base.clone();
    for (key, target) in [
        ("width", &mut preferences.width),
        ("height", &mut preferences.height),
        ("fps", &mut preferences.fps),
        ("bitrate", &mut preferences.bitrate_kbps),
    ] {
        if let Some(value) = get(key) {
            *target = value;
        }
    }
    if let Some(value) = get("mouseacceleration") {
        preferences.mouse_mode = if value != 0 { "office" } else { "game" }.into();
    }
    if let Some(value) = get("windowmode") {
        preferences.window_mode = match value {
            0 => "fullscreen",
            1 => "borderless",
            _ => "windowed",
        }
        .into();
    }
    if let Some(value) = get("videocfg") {
        preferences.codec = match value {
            1 => "h264",
            2 | 3 => "hevc",
            4 => "av1",
            _ => "auto",
        }
        .into();
    }
    if let Some(value) = get("audiocfg") {
        preferences.audio_config = match value {
            1 => "5.1",
            2 => "7.1",
            _ => "stereo",
        }
        .into();
    }
    if let Some(value) = get("capturesyskeys") {
        preferences.capture_keys = match value {
            0 => "never",
            2 => "always",
            _ => "fullscreen",
        }
        .into();
    }
    if let Some(value) = get("hostaudio") {
        preferences.host_audio = value != 0;
    }
    if let Some(value) = get("backgroundgamepad") {
        preferences.gamepad = value != 0;
    }
    preferences.validate().ok().map(|_| preferences)
}

fn encode(preferences: &StreamPreferences) -> BTreeMap<String, String> {
    let values = [
        ("width", preferences.width.to_string()),
        ("height", preferences.height.to_string()),
        ("fps", preferences.fps.to_string()),
        ("bitrate", preferences.bitrate_kbps.to_string()),
        (
            "mouseacceleration",
            (preferences.mouse_mode == "office").to_string(),
        ),
        (
            "windowmode",
            match preferences.window_mode.as_str() {
                "fullscreen" => "0",
                "borderless" => "1",
                _ => "2",
            }
            .into(),
        ),
        (
            "videocfg",
            match preferences.codec.as_str() {
                "h264" => "1",
                "hevc" => "2",
                "av1" => "4",
                _ => "0",
            }
            .into(),
        ),
        (
            "audiocfg",
            match preferences.audio_config.as_str() {
                "5.1" => "1",
                "7.1" => "2",
                _ => "0",
            }
            .into(),
        ),
        (
            "capturesyskeys",
            match preferences.capture_keys.as_str() {
                "never" => "0",
                "always" => "2",
                _ => "1",
            }
            .into(),
        ),
        ("hostaudio", preferences.host_audio.to_string()),
        ("backgroundgamepad", preferences.gamepad.to_string()),
    ];
    values
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}

fn general_values(text: &str) -> BTreeMap<String, String> {
    let mut general = false;
    let mut values = BTreeMap::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.starts_with('[') {
            general = line == "[General]";
            continue;
        }
        if general && !line.starts_with([';', '#']) {
            if let Some((key, value)) = line.split_once('=') {
                values.insert(key.trim().into(), value.trim().into());
            }
        }
    }
    values
}

pub fn read(ini: Option<&Path>, base: &StreamPreferences) -> Result<Option<StreamPreferences>> {
    if let Some(path) = ini {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let values = general_values(&text);
        return Ok(decode(base, |key| {
            values.get(key).and_then(|value| number(value))
        }));
    }
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey(REGISTRY_PATH) {
            Ok(key) => key,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        return Ok(decode(base, |name| {
            key.get_value::<u32, _>(name)
                .ok()
                .or_else(|| {
                    key.get_value::<u64, _>(name)
                        .ok()
                        .and_then(|value| value.try_into().ok())
                })
                .or_else(|| {
                    key.get_value::<String, _>(name)
                        .ok()
                        .and_then(|value| number(&value))
                })
        }));
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

pub fn write(ini: Option<&Path>, preferences: &StreamPreferences) -> Result<()> {
    preferences.validate()?;
    let values = encode(preferences);
    if let Some(path) = ini {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        let mut pending = values;
        let mut lines = vec![];
        let mut general = false;
        let mut found = false;
        for line in text.trim_start_matches('\u{feff}').lines() {
            if line.trim().starts_with('[') {
                if general {
                    lines.extend(
                        std::mem::take(&mut pending)
                            .into_iter()
                            .map(|(key, value)| format!("{key}={value}")),
                    );
                }
                general = line.trim() == "[General]";
                found |= general;
            }
            if general {
                if let Some((key, _)) = line.split_once('=') {
                    if let Some(value) = pending.remove(key.trim()) {
                        lines.push(format!("{}={value}", key.trim()));
                        continue;
                    }
                }
            }
            // Preserve all native hosts, pairing material and unsupported options.
            lines.push(line.to_owned());
        }
        if !found {
            lines.push("[General]".into());
        }
        lines.extend(
            pending
                .into_iter()
                .map(|(key, value)| format!("{key}={value}")),
        );
        return atomic_write(path, (lines.join("\n") + "\n").as_bytes());
    }
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(REGISTRY_PATH)?;
        for (name, value) in values {
            if let Ok(number) = value.parse::<u32>() {
                key.set_value(&name, &number)?;
            } else {
                key.set_value(&name, &value)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_qt_settings_update_supported_options_and_preserve_pairing_and_unrelated_options() {
        let root = tempfile::tempdir().unwrap();
        let ini = root.path().join("Moonlight.ini");
        fs::write(&ini, "[General]\nvsync=false\nmouseacceleration=true\nvideocfg=4\nwidth=2560\nheight=1440\nfps=120\nbitrate=42000\n[hosts]\n1\\uuid=existing-host\n1\\srvcert=@ByteArray(pairing-fixture)\n").unwrap();
        let base = StreamPreferences {
            volume: 33,
            audio_device: "LL-only device".into(),
            ..StreamPreferences::default()
        };
        let mut native = read(Some(&ini), &base).unwrap().unwrap();
        assert_eq!(
            (native.width, native.height, native.fps, native.bitrate_kbps),
            (2560, 1440, 120, 42000)
        );
        assert_eq!(native.codec, "av1");
        assert_eq!(native.mouse_mode, "office");
        assert_eq!(native.volume, 33);
        assert_eq!(native.audio_device, "LL-only device");
        native.window_mode = "borderless".into();
        native.mouse_mode = "game".into();
        write(Some(&ini), &native).unwrap();
        let text = fs::read_to_string(&ini).unwrap();
        assert!(
            text.contains("[hosts]\n1\\uuid=existing-host\n1\\srvcert=@ByteArray(pairing-fixture)")
        );
        assert!(text.contains("vsync=false"));
        assert_eq!(read(Some(&ini), &base).unwrap().unwrap(), native);
        assert_eq!(
            general_values(&text).get("videocfg").map(String::as_str),
            Some("4")
        );
    }
}
