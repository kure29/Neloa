use std::{cmp::Reverse, collections::HashMap, fs, io::Write, path::PathBuf, sync::Arc};

use parking_lot::{Mutex, RwLock};

use crate::model::TrustedDevice;

const TOUCH_PERSIST_INTERVAL_MS: u128 = 30_000;
pub(crate) const MAX_DEVICE_ALIAS_CHARS: usize = 32;

#[derive(Clone)]
pub(crate) struct TrustStore {
    path: PathBuf,
    devices: Arc<RwLock<HashMap<String, TrustedDevice>>>,
    persist_lock: Arc<Mutex<()>>,
    last_touch_persisted_ms: Arc<Mutex<u128>>,
}

impl TrustStore {
    pub(crate) fn load(path: PathBuf) -> Result<Self, String> {
        let devices: HashMap<String, TrustedDevice> = if path.exists() {
            let bytes =
                fs::read(&path).map_err(|error| format!("无法读取可信设备列表：{error}"))?;
            let stored = match serde_json::from_slice::<Vec<TrustedDevice>>(&bytes) {
                Ok(devices) => devices,
                Err(error) => {
                    // Never restore an older trust snapshot: it could resurrect a revoked key.
                    let backup =
                        path.with_extension(format!("corrupt-{}.json", uuid::Uuid::new_v4()));
                    fs::rename(&path, &backup)
                        .map_err(|error| format!("无法保留损坏的可信设备列表：{error}"))?;
                    eprintln!(
                        "可信设备列表已损坏（{error}），已保留至 {}，请重新配对",
                        backup.display()
                    );
                    Vec::new()
                }
            };
            stored
                .into_iter()
                .map(|device| (device.id.clone(), device))
                .collect()
        } else {
            HashMap::new()
        };
        let last_touch_persisted_ms = devices
            .values()
            .map(|device| device.last_verified_ms)
            .max()
            .unwrap_or_default();

        Ok(Self {
            path,
            devices: Arc::new(RwLock::new(devices)),
            persist_lock: Arc::new(Mutex::new(())),
            last_touch_persisted_ms: Arc::new(Mutex::new(last_touch_persisted_ms)),
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

    pub(crate) fn upsert(&self, mut device: TrustedDevice) -> Result<(), String> {
        self.update_devices(|devices| {
            if device.alias.is_none() {
                device.alias = devices
                    .get(&device.id)
                    .and_then(|existing| existing.alias.clone());
            }
            devices.insert(device.id.clone(), device);
            Ok(())
        })
    }

    pub(crate) fn set_alias(&self, id: &str, value: &str) -> Result<TrustedDevice, String> {
        let value = value.trim();
        if value.chars().count() > MAX_DEVICE_ALIAS_CHARS {
            return Err(format!("设备备注最多 {MAX_DEVICE_ALIAS_CHARS} 个字符"));
        }
        if value.chars().any(char::is_control) {
            return Err("设备备注不能包含换行或控制字符".into());
        }
        let alias = (!value.is_empty()).then(|| value.to_string());
        self.update_devices(|devices| {
            let device = devices
                .get_mut(id)
                .ok_or_else(|| "只能为已配对设备设置备注名".to_string())?;
            device.alias = alias;
            Ok(device.clone())
        })
    }

    pub(crate) fn remove(&self, id: &str) -> Result<bool, String> {
        if self.find(id).is_none() {
            return Ok(false);
        }
        self.update_devices(|devices| Ok(devices.remove(id).is_some()))
    }

    fn update_devices<T>(
        &self,
        update: impl FnOnce(&mut HashMap<String, TrustedDevice>) -> Result<T, String>,
    ) -> Result<T, String> {
        let _persist_guard = self.persist_lock.lock();
        let mut devices = self.devices.write();
        let mut next = devices.clone();
        let result = update(&mut next)?;
        self.persist_devices(next.values().cloned().collect())?;
        *devices = next;
        Ok(result)
    }

    pub(crate) async fn touch(&self, id: &str, at_ms: u128) -> Result<(), String> {
        let changed = {
            let mut devices = self.devices.write();
            if let Some(device) = devices.get_mut(id) {
                device.last_verified_ms = at_ms;
                true
            } else {
                false
            }
        };
        if !changed {
            return Ok(());
        }

        // Verification timestamps are useful metadata, not trust decisions. Keep the in-memory
        // value exact while coalescing frequent clipboard and transfer updates into one disk write.
        let should_persist = {
            let mut last_persisted = self.last_touch_persisted_ms.lock();
            if at_ms.saturating_sub(*last_persisted) < TOUCH_PERSIST_INTERVAL_MS {
                false
            } else {
                *last_persisted = at_ms;
                true
            }
        };
        if !should_persist {
            return Ok(());
        }

        let store = self.clone();
        let outcome = tokio::task::spawn_blocking(move || store.persist())
            .await
            .map_err(|error| format!("等待可信设备列表保存任务失败：{error}"))
            .and_then(|result| result);
        if outcome.is_err() {
            let mut last_persisted = self.last_touch_persisted_ms.lock();
            if *last_persisted == at_ms {
                *last_persisted = 0;
            }
        }
        outcome
    }

    fn persist(&self) -> Result<(), String> {
        let _persist_guard = self.persist_lock.lock();
        self.persist_devices(self.list())
    }

    fn persist_devices(&self, mut devices: Vec<TrustedDevice>) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        fs::create_dir_all(parent).map_err(|error| format!("无法创建应用数据目录：{error}"))?;
        devices.sort_by_key(|device| Reverse(device.last_verified_ms));
        let data = serde_json::to_vec_pretty(&devices)
            .map_err(|error| format!("无法序列化可信设备列表：{error}"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| format!("无法创建可信设备列表临时文件：{error}"))?;
        temporary
            .write_all(&data)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|error| format!("无法保存可信设备列表：{error}"))?;
        temporary
            .persist(&self.path)
            .map_err(|error| format!("无法替换可信设备列表：{error}"))?;
        *self.last_touch_persisted_ms.lock() = devices
            .iter()
            .map(|device| device.last_verified_ms)
            .max()
            .unwrap_or_default();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trusted_device(last_verified_ms: u128) -> TrustedDevice {
        TrustedDevice {
            id: "device-1".into(),
            name: "Test device".into(),
            alias: None,
            platform: "test".into(),
            public_key: "public-key".into(),
            fingerprint: "fingerprint".into(),
            paired_at_ms: 0,
            last_verified_ms,
        }
    }

    fn persisted_timestamp(path: &std::path::Path) -> u128 {
        let bytes = fs::read(path).unwrap();
        serde_json::from_slice::<Vec<TrustedDevice>>(&bytes).unwrap()[0].last_verified_ms
    }

    #[test]
    fn corrupt_store_is_preserved_without_restoring_trust() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trusted-devices.json");
        fs::write(&path, b"[{truncated").unwrap();
        let store = TrustStore::load(path.clone()).unwrap();
        assert!(store.list().is_empty());
        let backup = fs::read_dir(directory.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(backup
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("corrupt-"));
        assert_eq!(fs::read(backup).unwrap(), b"[{truncated");
        store.upsert(trusted_device(1)).unwrap();
        assert_eq!(TrustStore::load(path).unwrap().list().len(), 1);
    }

    #[test]
    fn failed_atomic_replace_does_not_commit_in_memory_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trusted-devices.json");
        let store = TrustStore::load(path.clone()).unwrap();
        store.upsert(trusted_device(1)).unwrap();
        let saved = directory.path().join("saved.json");
        fs::rename(&path, &saved).unwrap();
        fs::create_dir(&path).unwrap(); // Force replacement to fail, even under privileged tests.
        assert!(store.set_alias("device-1", "not committed").is_err());
        assert_eq!(store.find("device-1").unwrap().alias, None);
        assert!(store.remove("device-1").is_err());
        assert!(store.find("device-1").is_some());
        assert_eq!(persisted_timestamp(&saved), 1);
        fs::remove_dir(&path).unwrap();
        fs::rename(saved, &path).unwrap();
        store.remove("device-1").unwrap();
        assert!(TrustStore::load(path).unwrap().list().is_empty());
    }

    #[test]
    fn unfinished_temporary_write_leaves_previous_store_readable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trusted-devices.json");
        let store = TrustStore::load(path.clone()).unwrap();
        store.upsert(trusted_device(1)).unwrap();
        let mut interrupted = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        interrupted.write_all(b"[{unfinished").unwrap();
        assert_eq!(TrustStore::load(path.clone()).unwrap().list().len(), 1);
        store.upsert(trusted_device(2)).unwrap();
        assert_eq!(persisted_timestamp(&path), 2);
    }

    #[tokio::test]
    async fn coalesces_frequent_touch_writes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trusted-devices.json");
        let store = TrustStore::load(path.clone()).unwrap();
        store.upsert(trusted_device(0)).unwrap();

        store.touch("device-1", 30_000).await.unwrap();
        assert_eq!(persisted_timestamp(&path), 30_000);

        store.touch("device-1", 30_001).await.unwrap();
        assert_eq!(store.find("device-1").unwrap().last_verified_ms, 30_001);
        assert_eq!(persisted_timestamp(&path), 30_000);

        store.touch("device-1", 60_000).await.unwrap();
        assert_eq!(persisted_timestamp(&path), 60_000);
    }

    #[test]
    fn loads_legacy_devices_without_alias() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trusted-devices.json");
        fs::write(
            &path,
            r#"[{"id":"legacy","name":"Old Mac","platform":"macos","publicKey":"key","fingerprint":"fp","pairedAtMs":1,"lastVerifiedMs":2}]"#,
        )
        .unwrap();

        let store = TrustStore::load(path).unwrap();
        assert_eq!(store.find("legacy").unwrap().alias, None);
    }

    #[test]
    fn persists_alias_and_preserves_it_when_device_refreshes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trusted-devices.json");
        let store = TrustStore::load(path.clone()).unwrap();
        store.upsert(trusted_device(1)).unwrap();

        let renamed = store.set_alias("device-1", "  客厅 Mac  ").unwrap();
        assert_eq!(renamed.alias.as_deref(), Some("客厅 Mac"));

        let mut refreshed = trusted_device(2);
        refreshed.name = "Updated system name".into();
        store.upsert(refreshed).unwrap();
        assert_eq!(
            store.find("device-1").unwrap().alias.as_deref(),
            Some("客厅 Mac")
        );
        assert_eq!(
            TrustStore::load(path.clone())
                .unwrap()
                .find("device-1")
                .unwrap()
                .alias
                .as_deref(),
            Some("客厅 Mac")
        );

        assert_eq!(store.set_alias("device-1", "  ").unwrap().alias, None);
        assert!(store
            .set_alias("device-1", &"a".repeat(MAX_DEVICE_ALIAS_CHARS + 1))
            .is_err());
    }
}
