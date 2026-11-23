#[cfg(windows)]
mod windows_audio {
    use crate::{
        error::{AppError, ErrorCode, Result},
        storage::atomic_write,
    };
    use std::{fs, io::ErrorKind, path::PathBuf};
    use windows::{
        core::PCWSTR,
        Win32::{
            Media::Audio::{
                eCommunications, eConsole, eMultimedia, eRender, Endpoints::IAudioEndpointVolume,
                IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
            },
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
                COINIT_MULTITHREADED,
            },
        },
    };

    pub struct HostAudio {
        journal: PathBuf,
        muted_endpoints: Vec<String>,
    }

    impl HostAudio {
        pub fn new(data_root: PathBuf) -> Self {
            let journal = data_root.join("host-audio-mute.json");
            let muted_endpoints = fs::read(&journal)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            Self {
                journal,
                muted_endpoints,
            }
        }

        pub fn update(&mut self, controlled: bool) -> Result<()> {
            if !controlled && self.muted_endpoints.is_empty() {
                return Ok(());
            }
            let _com = ComApartment::open().map_err(audio_error)?;
            let devices: IMMDeviceEnumerator =
                unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                    .map_err(audio_error)?;
            if controlled {
                self.mute_default_devices(&devices)
            } else {
                self.restore(&devices)
            }
        }

        fn save(&self) -> Result<()> {
            if self.muted_endpoints.is_empty() {
                match fs::remove_file(&self.journal) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(error.into()),
                }
            } else {
                atomic_write(&self.journal, &serde_json::to_vec(&self.muted_endpoints)?)
            }
        }

        fn mute_default_devices(&mut self, devices: &IMMDeviceEnumerator) -> Result<()> {
            for role in [eMultimedia, eConsole, eCommunications] {
                let Ok(device) = (unsafe { devices.GetDefaultAudioEndpoint(eRender, role) }) else {
                    continue;
                };
                let id = endpoint_id(&device).map_err(audio_error)?;
                if self.muted_endpoints.contains(&id) {
                    continue;
                }
                let volume = endpoint_volume(&device).map_err(audio_error)?;
                if unsafe { volume.GetMute() }.map_err(audio_error)?.as_bool() {
                    continue;
                }
                // Save the original unmuted state before changing the endpoint.
                self.muted_endpoints.push(id);
                if let Err(error) = self.save() {
                    self.muted_endpoints.pop();
                    return Err(error);
                }
                if let Err(error) = unsafe { volume.SetMute(true, std::ptr::null()) } {
                    self.muted_endpoints.pop();
                    self.save()?;
                    return Err(audio_error(error));
                }
            }
            Ok(())
        }

        fn restore(&mut self, devices: &IMMDeviceEnumerator) -> Result<()> {
            let remaining = self
                .muted_endpoints
                .iter()
                .filter(|id| {
                    let id: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
                    let result = unsafe { devices.GetDevice(PCWSTR(id.as_ptr())) }
                        .and_then(|device| endpoint_volume(&device))
                        .and_then(|volume| unsafe { volume.SetMute(false, std::ptr::null()) });
                    result.is_err()
                })
                .cloned()
                .collect::<Vec<_>>();
            let previous = std::mem::replace(&mut self.muted_endpoints, remaining);
            if self.muted_endpoints != previous {
                if let Err(error) = self.save() {
                    self.muted_endpoints = previous;
                    return Err(error);
                }
            }
            Ok(())
        }
    }

    struct ComApartment;

    impl ComApartment {
        fn open() -> windows::core::Result<Self> {
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
            Ok(Self)
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    fn endpoint_id(device: &IMMDevice) -> windows::core::Result<String> {
        unsafe {
            let id = device.GetId()?;
            let text = id.to_string().unwrap_or_default();
            CoTaskMemFree(Some(id.as_ptr().cast()));
            Ok(text)
        }
    }

    fn endpoint_volume(device: &IMMDevice) -> windows::core::Result<IAudioEndpointVolume> {
        unsafe { device.Activate(CLSCTX_ALL, None) }
    }

    fn audio_error(error: windows::core::Error) -> AppError {
        tracing::debug!(code = ?error.code(), "Windows audio endpoint operation failed");
        AppError::new(ErrorCode::SystemFailed, "无法调整被控电脑的声音。")
    }
}

#[cfg(windows)]
pub use windows_audio::HostAudio;

#[cfg(not(windows))]
pub struct HostAudio;

#[cfg(not(windows))]
impl HostAudio {
    pub fn new(_: std::path::PathBuf) -> Self {
        Self
    }

    pub fn update(&mut self, _: bool) -> crate::error::Result<()> {
        Ok(())
    }
}
