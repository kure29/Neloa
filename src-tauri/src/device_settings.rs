use std::{fs, path::PathBuf, sync::Arc};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

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
        let settings = if path.exists() {
            let bytes = fs::read(&path).map_err(|error| format!("无法读取设备设置：{error}"))?;
            let stored: StoredDeviceSettings = serde_json::from_slice(&bytes)
                .map_err(|error| format!("设备设置已损坏：{error}"))?;
            StoredDeviceSettings {
                name: normalize_device_name(&stored.name)?,
            }
        } else {
            StoredDeviceSettings {
                name: safe_default_name(&default_name),
            }
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
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("无法创建设备设置目录：{error}"))?;
        }
        let bytes = serde_json::to_vec_pretty(settings)
            .map_err(|error| format!("无法编码设备设置：{error}"))?;
        fs::write(&self.path, bytes).map_err(|error| format!("无法保存设备设置：{error}"))
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
