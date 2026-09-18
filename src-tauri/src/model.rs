use serde::{Deserialize, Serialize};

pub(crate) const PROTOCOL_VERSION: u16 = 1;
pub(crate) const MIN_PROTOCOL_VERSION: u16 = 1;
pub(crate) const CAPABILITIES: &[&str] = &[
    "discovery",
    "pairing",
    "noise-xx",
    "test-message",
    "file-transfer",
    "streaming-file-hash",
    "clipboard-text",
];

pub(crate) fn protocol_compatible(peer_version: u16, peer_min_version: u16) -> bool {
    peer_min_version > 0
        && peer_min_version <= peer_version
        && peer_version >= MIN_PROTOCOL_VERSION
        && PROTOCOL_VERSION >= peer_min_version
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PeerDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub version: String,
    #[serde(default)]
    pub protocol_version: u16,
    #[serde(default)]
    pub min_protocol_version: u16,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub addresses: Vec<String>,
    pub port: u16,
    pub last_seen_ms: u128,
    #[serde(default)]
    pub relay_available: bool,
    #[serde(skip)]
    pub service_fullname: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TransportPreference {
    #[default]
    Auto,
    Lan,
    Relay,
}

impl PeerDevice {
    pub(crate) fn is_protocol_compatible(&self) -> bool {
        protocol_compatible(self.protocol_version, self.min_protocol_version)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoverySnapshot {
    pub active: bool,
    pub error: Option<String>,
    pub peers: Vec<PeerDevice>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectedFile {
    pub path: String,
    pub name: String,
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrustedDevice {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub platform: String,
    pub public_key: String,
    pub fingerprint: String,
    #[serde(default)]
    pub transport_preference: TransportPreference,
    pub paired_at_ms: u128,
    pub last_verified_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetworkStatus {
    pub active: bool,
    pub error: Option<String>,
    pub port: u16,
    pub identity_fingerprint: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecuritySnapshot {
    pub network: NetworkStatus,
    pub trusted_devices: Vec<TrustedDevice>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelaySnapshot {
    pub enabled: bool,
    pub url: String,
    pub has_token: bool,
    pub connected: bool,
    pub online_devices: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairingPeer {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairingRequest {
    pub session_id: String,
    pub peer: PairingPeer,
    pub code: String,
    pub direction: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairingResult {
    pub session_id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub accepted: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestMessageEvent {
    pub id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub text: String,
    pub direction: String,
    pub at_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileOfferEvent {
    pub transfer_id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub name: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileTransferProgress {
    pub transfer_id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub name: String,
    pub direction: String,
    pub stage: String,
    pub transferred: u64,
    pub size: u64,
    pub bytes_per_second: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileTransferResult {
    pub transfer_id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub name: String,
    pub direction: String,
    pub status: String,
    pub message: String,
    pub path: Option<String>,
    pub sha256: Option<String>,
    pub size: u64,
    pub at_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClipboardSnapshot {
    pub enabled: bool,
    pub max_bytes: usize,
    pub protect_sensitive: bool,
    pub poll_interval_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClipboardSyncEvent {
    pub id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub direction: String,
    pub status: String,
    pub bytes: usize,
    pub message: String,
    pub at_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticCheck {
    pub id: String,
    pub label: String,
    pub state: String,
    pub detail: String,
    pub guidance: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticPeer {
    pub id_prefix: String,
    pub name: String,
    pub platform: String,
    pub app_version: String,
    pub protocol_version: u16,
    pub min_protocol_version: u16,
    pub compatible: bool,
    pub capabilities: Vec<String>,
    pub last_seen_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticsSnapshot {
    pub generated_at_ms: u128,
    pub app_version: String,
    pub platform: String,
    pub protocol_version: u16,
    pub min_protocol_version: u16,
    pub device_id_prefix: String,
    pub checks: Vec<DiagnosticCheck>,
    pub peers: Vec<DiagnosticPeer>,
    pub firewall_guidance: String,
}

#[cfg(test)]
mod tests {
    use super::{protocol_compatible, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION};

    #[test]
    fn protocol_ranges_must_overlap_and_be_well_formed() {
        assert!(protocol_compatible(PROTOCOL_VERSION, MIN_PROTOCOL_VERSION));
        assert!(!protocol_compatible(0, 0));
        assert!(!protocol_compatible(2, 2));
        assert!(!protocol_compatible(1, 2));
    }
}
