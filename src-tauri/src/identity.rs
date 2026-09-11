use std::sync::Arc;

#[cfg(not(mobile))]
use keyring::{Entry, Error as KeyringError};
#[cfg(mobile)]
use keyring_core::{Entry, Error as KeyringError};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

const KEYRING_SERVICE: &str = "app.neloa.desktop";
const KEYRING_ACCOUNT: &str = "noise-static-key-v1";

#[derive(Clone)]
pub(crate) struct NoiseIdentity {
    secret: Arc<Zeroizing<[u8; 32]>>,
    public: [u8; 32],
}

impl NoiseIdentity {
    pub(crate) fn load_or_create() -> Result<Self, String> {
        #[cfg(mobile)]
        initialize_mobile_keyring()?;

        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
            .map_err(|error| format!("无法连接系统凭据库：{error}"))?;

        let secret = match entry.get_secret() {
            Ok(bytes) => bytes
                .try_into()
                .map_err(|_| "系统凭据库中的设备密钥长度无效".to_string())?,
            Err(KeyringError::NoEntry) => {
                let generated = StaticSecret::random().to_bytes();
                entry
                    .set_secret(&generated)
                    .map_err(|error| format!("无法将设备密钥写入系统凭据库：{error}"))?;
                generated
            }
            Err(error) => return Err(format!("无法读取系统凭据库中的设备密钥：{error}")),
        };

        Ok(Self::from_secret(secret))
    }

    #[cfg(test)]
    pub(crate) fn generate_for_test() -> Self {
        Self::from_secret(StaticSecret::random().to_bytes())
    }

    fn from_secret(secret: [u8; 32]) -> Self {
        let public = PublicKey::from(&StaticSecret::from(secret)).to_bytes();
        Self {
            secret: Arc::new(Zeroizing::new(secret)),
            public,
        }
    }

    pub(crate) fn secret(&self) -> &[u8] {
        self.secret.as_ref().as_ref()
    }

    #[cfg(test)]
    pub(crate) fn public(&self) -> [u8; 32] {
        self.public
    }

    pub(crate) fn fingerprint(&self) -> String {
        fingerprint(&self.public)
    }
}

#[cfg(target_os = "android")]
fn initialize_mobile_keyring() -> Result<(), String> {
    use std::sync::OnceLock;

    static INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();
    INITIALIZED
        .get_or_init(|| {
            let store = android_native_keyring_store::Store::new()
                .map_err(|error| format!("无法连接 Android Keystore：{error}"))?;
            keyring_core::set_default_store(store);
            Ok(())
        })
        .clone()
}

#[cfg(target_os = "ios")]
fn initialize_mobile_keyring() -> Result<(), String> {
    use std::sync::OnceLock;

    static INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();
    INITIALIZED
        .get_or_init(|| {
            let store = apple_native_keyring_store::keychain::Store::new()
                .map_err(|error| format!("无法连接 iOS Keychain：{error}"))?;
            keyring_core::set_default_store(store);
            Ok(())
        })
        .clone()
}

pub(crate) fn fingerprint(public_key: &[u8]) -> String {
    public_key
        .chunks(2)
        .take(4)
        .map(hex::encode_upper)
        .collect::<Vec<_>>()
        .join(":")
}
