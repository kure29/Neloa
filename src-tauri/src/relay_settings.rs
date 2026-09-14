use std::{fs, net::IpAddr, path::PathBuf, sync::Arc};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::Zeroizing;

use crate::{
    identity::{load_relay_token, store_relay_token},
    model::RelaySnapshot,
};

const MINIMUM_TOKEN_LEN: usize = 32;
const MAXIMUM_TOKEN_LEN: usize = 512;
const MAXIMUM_URL_LEN: usize = 2048;

#[derive(Clone)]
pub(crate) struct RelayConnectionConfig {
    pub url: String,
    pub token: Zeroizing<String>,
}

#[derive(Clone)]
pub(crate) enum RelayDirective {
    Disabled {
        url: String,
        has_token: bool,
    },
    Connect(RelayConnectionConfig),
    Invalid {
        url: String,
        has_token: bool,
        error: String,
    },
}

impl RelayDirective {
    pub(crate) fn initial_snapshot(&self) -> RelaySnapshot {
        match self {
            Self::Disabled { url, has_token } => RelaySnapshot {
                enabled: false,
                url: url.clone(),
                has_token: *has_token,
                connected: false,
                online_devices: 0,
                error: None,
            },
            Self::Connect(config) => RelaySnapshot {
                enabled: true,
                url: config.url.clone(),
                has_token: true,
                connected: false,
                online_devices: 0,
                error: None,
            },
            Self::Invalid {
                url,
                has_token,
                error,
            } => RelaySnapshot {
                enabled: true,
                url: url.clone(),
                has_token: *has_token,
                connected: false,
                online_devices: 0,
                error: Some(error.clone()),
            },
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredRelaySettings {
    enabled: bool,
    url: String,
}

#[derive(Clone)]
pub(crate) struct RelaySettingsStore {
    path: PathBuf,
    settings: Arc<RwLock<StoredRelaySettings>>,
}

impl RelaySettingsStore {
    pub(crate) fn load(path: PathBuf) -> Result<Self, String> {
        let settings = if path.exists() {
            let bytes = fs::read(&path).map_err(|error| format!("无法读取中继设置：{error}"))?;
            serde_json::from_slice(&bytes).map_err(|error| format!("中继设置已损坏：{error}"))?
        } else {
            StoredRelaySettings::default()
        };
        Ok(Self {
            path,
            settings: Arc::new(RwLock::new(settings)),
        })
    }

    pub(crate) fn directive(&self) -> RelayDirective {
        let settings = self.settings.read().clone();
        if !settings.enabled {
            return RelayDirective::Disabled {
                url: settings.url,
                has_token: load_relay_token().ok().flatten().is_some(),
            };
        }
        let token = match load_relay_token() {
            Ok(token) => token,
            Err(error) => {
                return RelayDirective::Invalid {
                    url: settings.url,
                    has_token: false,
                    error,
                };
            }
        };
        directive_from(settings, token)
    }

    pub(crate) fn update(
        &self,
        enabled: bool,
        url: String,
        token: Option<String>,
    ) -> Result<RelayDirective, String> {
        if !enabled {
            let settings = StoredRelaySettings {
                enabled: false,
                url: self.settings.read().url.clone(),
            };
            self.persist(&settings)?;
            *self.settings.write() = settings.clone();
            return Ok(RelayDirective::Disabled {
                url: settings.url,
                has_token: load_relay_token().ok().flatten().is_some(),
            });
        }
        let normalized_url = if url.trim().is_empty() {
            String::new()
        } else {
            normalize_relay_url(&url)?
        };
        if normalized_url.is_empty() {
            return Err("请输入中继 WebSocket 地址".into());
        }
        let new_token = token.filter(|value| !value.is_empty());
        if let Some(token) = &new_token {
            validate_relay_token(token)?;
        }
        let available_token = match new_token {
            Some(token) => {
                store_relay_token(&token)?;
                Some(Zeroizing::new(token))
            }
            None => load_relay_token()?,
        };
        if available_token.is_none() {
            return Err("请输入中继访问令牌".into());
        }

        let settings = StoredRelaySettings {
            enabled: true,
            url: normalized_url,
        };
        self.persist(&settings)?;
        *self.settings.write() = settings.clone();
        Ok(directive_from(settings, available_token))
    }

    fn persist(&self, settings: &StoredRelaySettings) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("无法创建设置目录：{error}"))?;
        }
        let bytes = serde_json::to_vec_pretty(settings)
            .map_err(|error| format!("无法编码中继设置：{error}"))?;
        fs::write(&self.path, bytes).map_err(|error| format!("无法保存中继设置：{error}"))
    }
}

fn directive_from(
    settings: StoredRelaySettings,
    token: Option<Zeroizing<String>>,
) -> RelayDirective {
    if !settings.enabled {
        return RelayDirective::Disabled {
            url: settings.url,
            has_token: token.is_some(),
        };
    }
    let Some(token) = token else {
        return RelayDirective::Invalid {
            url: settings.url,
            has_token: false,
            error: "中继访问令牌缺失，请重新保存中继设置".into(),
        };
    };
    if let Err(error) = validate_relay_token(&token) {
        return RelayDirective::Invalid {
            url: settings.url,
            has_token: true,
            error,
        };
    }
    match normalize_relay_url(&settings.url) {
        Ok(url) => RelayDirective::Connect(RelayConnectionConfig { url, token }),
        Err(error) => RelayDirective::Invalid {
            url: settings.url,
            has_token: true,
            error,
        },
    }
}

fn validate_relay_token(token: &str) -> Result<(), String> {
    if token.len() < MINIMUM_TOKEN_LEN || token.len() > MAXIMUM_TOKEN_LEN {
        return Err(format!(
            "中继令牌长度必须为 {MINIMUM_TOKEN_LEN}–{MAXIMUM_TOKEN_LEN} 个字符"
        ));
    }
    if token.chars().any(char::is_whitespace) {
        return Err("中继令牌不能包含空格或换行".into());
    }
    Ok(())
}

fn normalize_relay_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.len() > MAXIMUM_URL_LEN {
        return Err("中继地址过长".into());
    }
    let mut url = Url::parse(value).map_err(|_| "中继地址格式无效".to_string())?;
    let host = url
        .host_str()
        .ok_or_else(|| "中继地址缺少主机名".to_string())?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false);
    match url.scheme() {
        "wss" => {}
        "ws" if loopback => {}
        "ws" => return Err("远程中继必须使用加密的 wss:// 地址".into()),
        _ => return Err("中继地址必须以 wss:// 开头".into()),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("中继地址不能包含用户名或密码".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("中继地址不能包含查询参数或片段".into());
    }
    match url.path() {
        "" | "/" => url.set_path("/v1/ws"),
        "/v1/ws" => {}
        _ => return Err("中继地址路径必须为 /v1/ws".into()),
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::{normalize_relay_url, validate_relay_token};

    #[test]
    fn relay_url_requires_tls_except_for_loopback() {
        assert_eq!(
            normalize_relay_url("wss://relay.example.com").unwrap(),
            "wss://relay.example.com/v1/ws"
        );
        assert!(normalize_relay_url("ws://relay.example.com/v1/ws").is_err());
        assert!(normalize_relay_url("ws://127.0.0.1:8787").is_ok());
        assert!(normalize_relay_url("wss://relay.example.com/other").is_err());
    }

    #[test]
    fn relay_token_uses_the_server_policy() {
        assert!(validate_relay_token("short").is_err());
        assert!(validate_relay_token(&"x".repeat(32)).is_ok());
        assert!(validate_relay_token(&format!("{} ", "x".repeat(32))).is_err());
    }
}
