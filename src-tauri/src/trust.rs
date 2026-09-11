use std::{cmp::Reverse, collections::HashMap, fs, path::PathBuf, sync::Arc};

use parking_lot::RwLock;

use crate::model::TrustedDevice;

#[derive(Clone)]
pub(crate) struct TrustStore {
    path: PathBuf,
    devices: Arc<RwLock<HashMap<String, TrustedDevice>>>,
}

impl TrustStore {
    pub(crate) fn load(path: PathBuf) -> Result<Self, String> {
        let devices = if path.exists() {
            let bytes =
                fs::read(&path).map_err(|error| format!("无法读取可信设备列表：{error}"))?;
            serde_json::from_slice::<Vec<TrustedDevice>>(&bytes)
                .map_err(|error| format!("可信设备列表已损坏：{error}"))?
                .into_iter()
                .map(|device| (device.id.clone(), device))
                .collect()
        } else {
            HashMap::new()
        };

        Ok(Self {
            path,
            devices: Arc::new(RwLock::new(devices)),
        })
    }

    pub(crate) fn list(&self) -> Vec<TrustedDevice> {
        let mut devices: Vec<_> = self.devices.read().values().cloned().collect();
        devices.sort_by_key(|device| Reverse(device.last_verified_ms));
        devices
    }

    pub(crate) fn find(&self, id: &str) -> Option<TrustedDevice> {
        self.devices.read().get(id).cloned()
    }

    pub(crate) fn upsert(&self, device: TrustedDevice) -> Result<(), String> {
        self.devices.write().insert(device.id.clone(), device);
        self.persist()
    }

    pub(crate) fn remove(&self, id: &str) -> Result<bool, String> {
        let removed = self.devices.write().remove(id).is_some();
        if removed {
            self.persist()?;
        }
        Ok(removed)
    }

    pub(crate) fn touch(&self, id: &str, at_ms: u128) -> Result<(), String> {
        let changed = {
            let mut devices = self.devices.write();
            if let Some(device) = devices.get_mut(id) {
                device.last_verified_ms = at_ms;
                true
            } else {
                false
            }
        };
        if changed {
            self.persist()?;
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("无法创建应用数据目录：{error}"))?;
        }
        let data = serde_json::to_vec_pretty(&self.list())
            .map_err(|error| format!("无法序列化可信设备列表：{error}"))?;
        fs::write(&self.path, data).map_err(|error| format!("无法保存可信设备列表：{error}"))
    }
}
