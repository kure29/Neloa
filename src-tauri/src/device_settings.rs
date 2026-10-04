use std::{fs, path::PathBuf, sync::Arc};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::storage::{set_aside_corrupt_settings, write_atomically};

pub(crate) const MAX_DEVICE_NAME_CHARS: usize = 32;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredDeviceSettings {
    name: String,
}

#[derive(Clone)]
pub(crate) struct DeviceSettingsStore {
    path: PathBuf,
    settings: Arc<RwLock<StoredDeviceSettings>>,
}

impl DeviceSettingsStore {
    pub(crate) fn load(path: PathBuf, default_name: String) -> Result<Self, String> {
        let stored_name = if path.exists() {
            let bytes = fs::read(&path).map_err(|error| format!("无法读取设备设置：{error}"))?;
            match serde_json::from_slice::<StoredDeviceSettings>(&bytes)
                .map_err(|error| error.to_string())
                .and_then(|stored| normalize_device_name(&stored.name))
            {
                Ok(name) => Some(name),
                Err(error) => {
                    set_aside_corrupt_settings(&path, "设备设置", &error);
                    None
                }
            }
        } else {
            None
        };
        let settings = StoredDeviceSettings {
            name: stored_name.unwrap_or_else(|| safe_default_name(&default_name)),
        };
        Ok(Self {
            path,
            settings: Arc::new(RwLock::new(settings)),
        })
    }

    pub(crate) fn name(&self) -> String {
        self.settings.read().name.clone()
    }

    pub(crate) fn update(&self, name: &str) -> Result<String, String> {
        let name = normalize_device_name(name)?;
        let settings = StoredDeviceSettings { name: name.clone() };
        self.persist(&settings)?;
        *self.settings.write() = settings;
        Ok(name)
    }

    fn persist(&self, settings: &StoredDeviceSettings) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(settings)
            .map_err(|error| format!("无法编码设备设置：{error}"))?;
        write_atomically(&self.path, &bytes).map_err(|error| format!("无法保存设备设置：{error}"))
    }
}

fn normalize_device_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("设备名称不能为空".into());
    }
    if value.chars().count() > MAX_DEVICE_NAME_CHARS {
        return Err(format!("设备名称最多 {MAX_DEVICE_NAME_CHARS} 个字符"));
    }
    if value.chars().any(char::is_control) {
        return Err("设备名称不能包含换行或控制字符".into());
    }
    Ok(value.to_string())
}

fn safe_default_name(value: &str) -> String {
    let sanitized = value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_DEVICE_NAME_CHARS)
        .collect::<String>();
    if sanitized.is_empty() {
        "Neloa Device".into()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_trims_custom_names() {
        assert_eq!(
            normalize_device_name("  我的 iPhone  ").unwrap(),
            "我的 iPhone"
        );
        assert!(normalize_device_name("   ").is_err());
        assert!(normalize_device_name("line\nbreak").is_err());
        assert!(normalize_device_name(&"a".repeat(MAX_DEVICE_NAME_CHARS + 1)).is_err());
        assert_eq!(
            normalize_device_name(&"机".repeat(MAX_DEVICE_NAME_CHARS))
                .unwrap()
                .chars()
                .count(),
            MAX_DEVICE_NAME_CHARS
        );
    }

    #[test]
    fn unreadable_settings_fall_back_to_the_default_name() {
        for contents in [&b""[..], b"{\"name\":", b"{\"name\":\"line\\nbreak\"}"] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("device-settings.json");
            fs::write(&path, contents).unwrap();

            let store = DeviceSettingsStore::load(path.clone(), "MacBook".into()).unwrap();
            assert_eq!(store.name(), "MacBook");
            let backup = fs::read_dir(root.path()).unwrap().next().unwrap().unwrap();
            assert!(backup.file_name().to_string_lossy().contains("corrupt-"));
            assert_eq!(fs::read(backup.path()).unwrap(), contents);

            store.update("Renamed").unwrap();
            assert_eq!(
                DeviceSettingsStore::load(path, "ignored".into())
                    .unwrap()
                    .name(),
                "Renamed"
            );
        }
    }

    #[test]
    fn persists_the_selected_name() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("device-settings.json");
        let store = DeviceSettingsStore::load(path.clone(), "iPhone".into()).unwrap();
        assert_eq!(store.name(), "iPhone");
        assert_eq!(store.update("Neloa Phone").unwrap(), "Neloa Phone");
        assert_eq!(
            DeviceSettingsStore::load(path, "ignored".into())
                .unwrap()
                .name(),
            "Neloa Phone"
        );
    }
}
