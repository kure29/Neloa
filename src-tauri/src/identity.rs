use std::sync::Arc;

#[cfg(not(mobile))]
use keyring::{Entry, Error as KeyringError};
#[cfg(mobile)]
use keyring_core::{Entry, Error as KeyringError};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

// Keep this internal keyring namespace stable when the public bundle ID changes,
// so the existing Noise identity can still be read. Filesystem-backed device,
// trust, and settings data are migrated separately during desktop startup.
const KEYRING_SERVICE: &str = "app.neloa.desktop";
const KEYRING_ACCOUNT: &str = "noise-static-key-v1";
const RELAY_TOKEN_ACCOUNT: &str = "relay-access-token-v1";

#[derive(Clone)]
pub(crate) struct NoiseIdentity {
    secret: Arc<Zeroizing<[u8; 32]>>,
    public: [u8; 32],
}

impl NoiseIdentity {
    pub(crate) fn load_or_create() -> Result<Self, String> {
        initialize_keyring()?;

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

pub(crate) fn load_relay_token() -> Result<Option<Zeroizing<String>>, String> {
    initialize_keyring()?;
    let entry = Entry::new(KEYRING_SERVICE, RELAY_TOKEN_ACCOUNT)
        .map_err(|error| format!("无法连接系统凭据库：{error}"))?;
    match entry.get_secret() {
        Ok(bytes) => {
            let bytes = Zeroizing::new(bytes);
            String::from_utf8(bytes.to_vec())
                .map(Zeroizing::new)
                .map(Some)
                .map_err(|_| "系统凭据库中的中继令牌不是有效文本".to_string())
        }
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(format!("无法读取系统凭据库中的中继令牌：{error}")),
    }
}

pub(crate) fn store_relay_token(token: &str) -> Result<(), String> {
    initialize_keyring()?;
    Entry::new(KEYRING_SERVICE, RELAY_TOKEN_ACCOUNT)
        .map_err(|error| format!("无法连接系统凭据库：{error}"))?
        .set_secret(token.as_bytes())
        .map_err(|error| format!("无法将中继令牌写入系统凭据库：{error}"))
}

#[cfg(not(mobile))]
fn initialize_keyring() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "android")]
fn initialize_keyring() -> Result<(), String> {
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
fn initialize_keyring() -> Result<(), String> {
    use std::sync::OnceLock;

    static INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();
    INITIALIZED
        .get_or_init(|| {
            let store = apple_native_keyring_store::protected::Store::new()
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
