use crate::{
    error::{AppError, ErrorCode, Result},
    model::{AccountData, Device, DeviceEdit, Preferences},
};
use sha2::{Digest, Sha256};
use std::{
    cmp::Reverse,
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use zeroize::{Zeroize, Zeroizing};

pub fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new(ErrorCode::StorageFailed, "数据路径无效。"))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if path.exists() {
            fs::copy(path, path.with_extension("bak"))?;
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            };
            let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        #[cfg(not(windows))]
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<(T, bool)> {
    if !path.exists() {
        return Ok((T::default(), false));
    }
    let bytes = fs::read(path)?;
    match serde_json::from_slice(&bytes) {
        Ok(value) => Ok((value, false)),
        Err(_) => {
            let backup = fs::read(path.with_extension("bak"))?;
            Ok((serde_json::from_slice(&backup)?, true))
        }
    }
}

fn device_priority(device: &Device) -> (bool, u32) {
    let confirmed = device
        .uuid
        .as_ref()
        .is_some_and(|uuid| !uuid.trim().is_empty());
    let priority = u32::from(device.credential_ref.is_some()) * 10
        + u32::from(device.favorite) * 3
        + u32::from(device.display_name != device.host_name) * 2
        + u32::from(device.wallpaper.is_some())
        + u32::from(!device.note.is_empty())
        + u32::from(device.preferences != crate::model::StreamPreferences::default())
        + u32::from(device.quick_apps != ["Desktop"]);
    (confirmed, priority)
}

pub struct Store {
    pub root: PathBuf,
    pub preferences: Preferences,
    pub account_data: AccountData,
    pub account_scope: String,
    pub warning: Option<String>,
}
impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let (preferences, recovered) = read_json(&root.join("preferences.json"))?;
        Ok(Self {
            root,
            preferences,
            account_data: AccountData::default(),
            account_scope: "unassigned".into(),
            warning: recovered.then(|| "偏好文件损坏，已读取上一份备份。".into()),
        })
    }
    pub fn switch_account(&mut self, username: Option<&str>) -> Result<()> {
        let scope = username
            .map(|u| digest(&u.trim().to_lowercase()))
            .unwrap_or_else(|| "unassigned".into());
        if scope == self.account_scope {
            return Ok(());
        }
        let path = self.root.join("accounts").join(&scope).join("devices.json");
        let (mut data, recovered): (AccountData, bool) = read_json(&path)?;
        for device in &mut data.devices {
            device.status = "unknown".into();
            device.service_ready = None;
        }
        self.account_data = data;
        self.account_scope = scope;
        let mut devices = std::mem::take(&mut self.account_data.devices);
        let original_count = devices.len();
        devices.sort_by_key(|device| Reverse(device_priority(device)));
        for device in devices {
            self.merge_device(device);
        }
        if self.account_data.devices.len() < original_count {
            self.save_devices()?;
        }
        if recovered {
            self.warning = Some("设备文件损坏，已读取上一份备份。".into());
        }
        Ok(())
    }
    pub fn save_preferences(&self) -> Result<()> {
        atomic_write(
            &self.root.join("preferences.json"),
            &serde_json::to_vec_pretty(&self.preferences)?,
        )
    }
    pub fn save_devices(&self) -> Result<()> {
        atomic_write(
            &self
                .root
                .join("accounts")
                .join(&self.account_scope)
                .join("devices.json"),
            &serde_json::to_vec_pretty(&self.account_data)?,
        )
    }
    pub fn device(&self, id: &str) -> Result<Device> {
        self.account_data
            .devices
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Offline,
                    "设备已移除或不属于当前账号，请刷新设备列表。",
                )
            })
    }
    pub fn remove_device(&mut self, id: &str) -> Result<()> {
        self.device(id)?;
        let mut account = self.account_data.clone();
        account.devices.retain(|candidate| candidate.id != id);
        atomic_write(
            &self
                .root
                .join("accounts")
                .join(&self.account_scope)
                .join("devices.json"),
            &serde_json::to_vec_pretty(&account)?,
        )?;
        self.account_data = account;
        Ok(())
    }
    pub fn merge_device(&mut self, mut incoming: Device) -> String {
        for identifier in [
            &mut incoming.nat_id,
            &mut incoming.uuid,
            &mut incoming.peer_code,
        ] {
            *identifier = identifier
                .take()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
        }
        let mut matches: Vec<Device> = self
            .account_data
            .devices
            .iter()
            .filter(|d| d.same_host(&incoming))
            .cloned()
            .collect();
        let known_uuids: HashSet<String> = matches
            .iter()
            .filter_map(|device| device.uuid.as_deref())
            .map(|uuid| uuid.trim().to_ascii_lowercase())
            .filter(|uuid| !uuid.is_empty())
            .collect();
        if incoming.uuid.is_none() && known_uuids.len() > 1 {
            // The directory can reuse a peer ID across different Sunshine hosts.
            // Resolve an unverified directory row only when its current peer ID
            // and reported host name point to one of the verified records.
            let directory_match = if incoming.source == "account"
                && incoming.peer_code.is_none()
                && incoming.credential_ref.is_none()
                && !incoming.host_name.trim().is_empty()
            {
                let mut candidates = matches.iter().filter(|device| {
                    device.uuid.as_deref().is_some_and(|uuid| !uuid.trim().is_empty())
                        && device.nat_id.as_deref()
                            .zip(incoming.nat_id.as_deref())
                            .is_some_and(|(saved, reported)| saved.eq_ignore_ascii_case(reported))
                        && device.host_name.trim().eq_ignore_ascii_case(incoming.host_name.trim())
                });
                candidates
                    .next()
                    .filter(|_| candidates.next().is_none())
                    .map(|device| device.id.clone())
            } else {
                None
            };
            matches.retain(|device| {
                device.id == incoming.id
                    || directory_match.as_deref() == Some(device.id.as_str())
                    || (device.uuid.is_none()
                        && (directory_match.is_none()
                            || device.host_name.trim().eq_ignore_ascii_case(incoming.host_name.trim())))
            });
        }
        let primary = matches
            .iter()
            .find(|device| device.id == incoming.id)
            .or_else(|| matches.iter().max_by_key(|device| device_priority(device)));
        if let Some(primary) = primary {
            let mut current = primary.clone();
            let latest = matches
                .iter()
                .chain(std::iter::once(&incoming))
                .max_by(|left, right| {
                    left.observed_at
                        .as_ref()
                        .max(left.last_connected_at.as_ref())
                        .cmp(
                            &right
                                .observed_at
                                .as_ref()
                                .max(right.last_connected_at.as_ref()),
                        )
                })
                .unwrap()
                .clone();
            let credential = matches
                .iter()
                .chain(std::iter::once(&incoming))
                .filter(|device| device.credential_ref.is_some())
                .max_by_key(|device| {
                    (
                        device.credential_state == "valid",
                        device
                            .observed_at
                            .as_ref()
                            .max(device.last_connected_at.as_ref()),
                    )
                })
                .map(|device| {
                    (
                        device.credential_ref.clone(),
                        device.credential_state.clone(),
                    )
                });
            if current.display_name.is_empty() || current.display_name == current.host_name {
                if let Some(customized) =
                    matches
                        .iter()
                        .chain(std::iter::once(&incoming))
                        .find(|device| {
                            !device.display_name.is_empty()
                                && device.display_name != device.host_name
                        })
                {
                    current.display_name = customized.display_name.clone();
                }
            }
            current.favorite |= incoming.favorite || matches.iter().any(|d| d.favorite);
            if current.wallpaper.is_none() {
                current.wallpaper = matches
                    .iter()
                    .find_map(|d| d.wallpaper.clone())
                    .or_else(|| incoming.wallpaper.clone());
            }
            if current.note.is_empty() {
                current.note = matches
                    .iter()
                    .find(|d| !d.note.is_empty())
                    .map(|d| d.note.clone())
                    .unwrap_or_else(|| incoming.note.clone());
            }
            if current.peer_code.is_none() {
                current.peer_code = matches.iter().find_map(|d| d.peer_code.clone());
            }
            if current.preferences == crate::model::StreamPreferences::default() {
                if let Some(customized) = matches
                    .iter()
                    .chain(std::iter::once(&incoming))
                    .find(|device| device.preferences != crate::model::StreamPreferences::default())
                {
                    current.preferences = customized.preferences.clone();
                }
            }
            if current.quick_apps == ["Desktop"] {
                if let Some(customized) = matches
                    .iter()
                    .chain(std::iter::once(&incoming))
                    .find(|device| device.quick_apps != ["Desktop"])
                {
                    current.quick_apps = customized.quick_apps.clone();
                }
            }
            for history in &mut self.account_data.history {
                if history.device_id.as_deref() == Some(&incoming.id)
                    || matches
                        .iter()
                        .any(|d| history.device_id.as_deref() == Some(&d.id))
                {
                    history.device_id = Some(current.id.clone());
                }
            }
            self.account_data
                .devices
                .retain(|d| !matches.iter().any(|matched| matched.id == d.id));
            let unverified_account_name = incoming.source == "account"
                && incoming.uuid.is_none()
                && current.uuid.is_some();
            if !latest.host_name.is_empty() && !unverified_account_name {
                if current.display_name.is_empty() || current.display_name == current.host_name {
                    current.display_name = latest.host_name.clone();
                }
                current.host_name = latest.host_name;
            }
            if incoming.uuid.is_some() || current.uuid.is_some() {
                for id in matches
                    .iter()
                    .chain(std::iter::once(&incoming))
                    .flat_map(|device| device.nat_id.iter().chain(&device.known_nat_ids))
                {
                    if !current
                        .known_nat_ids
                        .iter()
                        .any(|known| known.eq_ignore_ascii_case(id))
                    {
                        current.known_nat_ids.push(id.clone());
                    }
                }
            }
            current.nat_id = latest.nat_id.or(incoming.nat_id).or(current.nat_id);
            current.uuid = incoming.uuid.or(current.uuid);
            current.address = latest.address.or(incoming.address).or(current.address);
            current.peer_code = latest
                .peer_code
                .or(incoming.peer_code)
                .or(current.peer_code);
            current.status = latest.status;
            current.service_ready = latest.service_ready;
            current.observed_at = latest.observed_at;
            if !latest.apps.is_empty() {
                current.apps = latest.apps;
            } else if !incoming.apps.is_empty() {
                current.apps = incoming.apps;
            }
            current.last_connected_at = matches
                .iter()
                .filter_map(|device| device.last_connected_at.clone())
                .max()
                .max(incoming.last_connected_at);
            if let Some((reference, state)) = credential {
                current.credential_ref = reference;
                current.credential_state = state;
            }
            if current.peer_code.is_some() {
                current.source = "code".into();
            }
            let id = current.id.clone();
            self.account_data.devices.push(current);
            id
        } else {
            let id = incoming.id.clone();
            self.account_data.devices.push(incoming);
            id
        }
    }
    pub fn edit_device(&mut self, edit: &DeviceEdit) -> Result<()> {
        let name = edit.display_name.trim();
        if name.is_empty() || name.chars().count() > 64 || edit.note.chars().count() > 256 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "设备名称为 1–64 个字符，备注不超过 256 个字符。",
            ));
        }
        let device = self.device(&edit.device_id)?;
        let wallpaper = if edit.reset_wallpaper {
            None
        } else if let Some(path) = &edit.wallpaper_path {
            Some(self.import_wallpaper(Path::new(path))?)
        } else {
            device.wallpaper
        };
        let current = self
            .account_data
            .devices
            .iter_mut()
            .find(|d| d.id == edit.device_id)
            .unwrap();
        current.display_name = name.into();
        current.favorite = edit.favorite;
        current.note = edit.note.trim().into();
        current.wallpaper = wallpaper;
        self.save_devices()
    }
    fn import_wallpaper(&self, path: &Path) -> Result<String> {
        let (bytes, extension) = read_wallpaper(path)?;
        let relative = format!("wallpapers/{}.{extension}", uuid::Uuid::new_v4());
        atomic_write(&self.root.join(&relative), &bytes)?;
        Ok(relative)
    }
    pub fn wallpaper_path(&self, relative: &str) -> Result<PathBuf> {
        let path = Path::new(relative);
        if path.is_absolute()
            || !relative.starts_with("wallpapers/")
            || path
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "壁纸路径不在应用数据目录内。",
            ));
        }
        let absolute = self.root.join(path).canonicalize()?;
        if !absolute.starts_with(self.root.canonicalize()?) {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "壁纸路径不在应用数据目录内。",
            ));
        }
        Ok(absolute)
    }
}

pub fn read_wallpaper(path: &Path) -> Result<(Vec<u8>, &'static str)> {
    let meta = fs::metadata(path)?;
    if !meta.is_file() || meta.len() > 12 * 1024 * 1024 {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "请选择不超过 12 MB 的 PNG、JPEG 或 WebP 图片。",
        ));
    }
    let bytes = fs::read(path)?;
    let extension = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "jpg"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "webp"
    } else {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "图片格式不受支持，请选择 PNG、JPEG 或 WebP。",
        ));
    };
    Ok((bytes, extension))
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SecretRecord {
    pub username: String,
    pub password: String,
}
impl Drop for SecretRecord {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[derive(Clone)]
pub struct Vault {
    root: PathBuf,
}
impl Vault {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.join("credentials"),
        }
    }
    pub fn reference(scope: &str, identity: &str) -> String {
        digest(&format!("LLRemote-v1:{scope}:{identity}"))
    }
    pub fn contains(&self, reference: &str) -> bool {
        self.validate_reference(reference).is_ok()
            && self.root.join(format!("{reference}.bin")).is_file()
    }
    pub fn put(&self, reference: &str, secret: &SecretRecord) -> Result<()> {
        self.validate_reference(reference)?;
        let bytes = Zeroizing::new(serde_json::to_vec(secret)?);
        let encrypted = protect(&bytes, reference.as_bytes())?;
        atomic_write(&self.root.join(format!("{reference}.bin")), &encrypted)
    }
    pub fn get(&self, reference: &str) -> Result<SecretRecord> {
        self.validate_reference(reference)?;
        let bytes = fs::read(self.root.join(format!("{reference}.bin"))).map_err(|_| {
            AppError::new(
                ErrorCode::CredentialUnavailable,
                "本机保存的凭据不可用，请重新验证。",
            )
        })?;
        let decrypted = Zeroizing::new(unprotect(&bytes, reference.as_bytes())?);
        serde_json::from_slice(&decrypted).map_err(|_| {
            AppError::new(
                ErrorCode::CredentialUnavailable,
                "本机凭据无法解析，请重新验证。",
            )
        })
    }
    pub fn delete(&self, reference: &str) -> Result<()> {
        self.validate_reference(reference)?;
        let path = self.root.join(format!("{reference}.bin"));
        if path.exists() {
            fs::remove_file(&path)?;
        }
        let backup = path.with_extension("bak");
        if backup.exists() {
            fs::remove_file(backup)?;
        }
        Ok(())
    }
    fn validate_reference(&self, reference: &str) -> Result<()> {
        if reference.len() != 64 || !reference.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(AppError::new(
                ErrorCode::CredentialUnavailable,
                "凭据引用无效。",
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
fn crypt(bytes: &[u8], entropy: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let success = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if success == 0 {
        return Err(AppError::new(
            ErrorCode::CredentialUnavailable,
            "Windows 无法解密此凭据；更换电脑或用户后需要重新验证。",
        ));
    }
    let mut value =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    if !encrypt {
        unsafe {
            std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        }
    }
    unsafe {
        LocalFree(output.pbData as _);
    }
    if value.len() > 65536 {
        value.zeroize();
        return Err(AppError::new(
            ErrorCode::CredentialUnavailable,
            "凭据数据长度无效。",
        ));
    }
    Ok(value)
}
#[cfg(windows)]
fn protect(bytes: &[u8], entropy: &[u8]) -> Result<Vec<u8>> {
    crypt(bytes, entropy, true)
}
#[cfg(windows)]
fn unprotect(bytes: &[u8], entropy: &[u8]) -> Result<Vec<u8>> {
    crypt(bytes, entropy, false)
}
#[cfg(not(windows))]
fn protect(_: &[u8], _: &[u8]) -> Result<Vec<u8>> {
    Err(AppError::new(
        ErrorCode::SystemFailed,
        "凭据保存需要 Windows DPAPI。",
    ))
}
#[cfg(not(windows))]
fn unprotect(_: &[u8], _: &[u8]) -> Result<Vec<u8>> {
    Err(AppError::new(
        ErrorCode::CredentialUnavailable,
        "凭据解密需要 Windows DPAPI。",
    ))
}
