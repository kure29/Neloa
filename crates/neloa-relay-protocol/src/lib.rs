use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RELAY_PROTOCOL_VERSION: u16 = 1;
pub const TUNNEL_HEADER_LEN: usize = 16;
pub const MAX_RELAY_PAYLOAD: usize = 256 * 1024;
pub const MAX_DEVICE_ID_LEN: usize = 128;
pub const MAX_DEVICE_NAME_LEN: usize = 128;
pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_CAPABILITY_LEN: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub app_version: String,
    pub protocol_version: u16,
    pub min_protocol_version: u16,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl RelayDevice {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_text("device id", &self.id, MAX_DEVICE_ID_LEN)?;
        validate_text("device name", &self.name, MAX_DEVICE_NAME_LEN)?;
        validate_text("platform", &self.platform, MAX_CAPABILITY_LEN)?;
        validate_text("app version", &self.app_version, MAX_CAPABILITY_LEN)?;
        if self.protocol_version == 0
            || self.min_protocol_version == 0
            || self.min_protocol_version > self.protocol_version
        {
            return Err(ProtocolError::InvalidProtocolRange);
        }
        if self.capabilities.len() > MAX_CAPABILITIES {
            return Err(ProtocolError::TooManyCapabilities);
        }
        for capability in &self.capabilities {
            validate_text("capability", capability, MAX_CAPABILITY_LEN)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ClientControl {
    Register {
        relay_protocol_version: u16,
        device: RelayDevice,
    },
    OpenTunnel {
        tunnel_id: Uuid,
        target_id: String,
    },
    CloseTunnel {
        tunnel_id: Uuid,
    },
    Ping {
        nonce: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerControl {
    Registered {
        relay_protocol_version: u16,
    },
    Presence {
        devices: Vec<RelayDevice>,
    },
    IncomingTunnel {
        tunnel_id: Uuid,
        source: RelayDevice,
    },
    TunnelOpened {
        tunnel_id: Uuid,
        target: RelayDevice,
    },
    TunnelClosed {
        tunnel_id: Uuid,
        reason: RelayCloseReason,
    },
    Pong {
        nonce: u64,
    },
    Error {
        code: RelayErrorCode,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tunnel_id: Option<Uuid>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelayCloseReason {
    Requested,
    PeerDisconnected,
    Backpressure,
    ProtocolError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelayErrorCode {
    AuthenticationRequired,
    RegistrationRequired,
    AlreadyRegistered,
    DuplicateDevice,
    IncompatibleProtocol,
    InvalidMessage,
    InvalidDevice,
    ServerFull,
    InvalidTarget,
    TargetOffline,
    TunnelAlreadyExists,
    TunnelNotFound,
    TunnelAccessDenied,
    Backpressure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    MissingField(&'static str),
    FieldTooLong(&'static str),
    InvalidProtocolRange,
    TooManyCapabilities,
    FrameTooShort,
    FrameTooLarge,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(field) => write!(formatter, "{field} is empty"),
            Self::FieldTooLong(field) => write!(formatter, "{field} is too long"),
            Self::InvalidProtocolRange => formatter.write_str("protocol range is invalid"),
            Self::TooManyCapabilities => formatter.write_str("too many capabilities"),
            Self::FrameTooShort => formatter.write_str("relay frame is shorter than its header"),
            Self::FrameTooLarge => formatter.write_str("relay frame payload exceeds the limit"),
        }
    }
}

impl std::error::Error for ProtocolError {}

pub fn encode_tunnel_frame(tunnel_id: Uuid, payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if payload.len() > MAX_RELAY_PAYLOAD {
        return Err(ProtocolError::FrameTooLarge);
    }
    let mut frame = Vec::with_capacity(TUNNEL_HEADER_LEN + payload.len());
    frame.extend_from_slice(tunnel_id.as_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

pub fn decode_tunnel_frame(frame: &[u8]) -> Result<(Uuid, &[u8]), ProtocolError> {
    if frame.len() < TUNNEL_HEADER_LEN {
        return Err(ProtocolError::FrameTooShort);
    }
    let payload = &frame[TUNNEL_HEADER_LEN..];
    if payload.len() > MAX_RELAY_PAYLOAD {
        return Err(ProtocolError::FrameTooLarge);
    }
    let tunnel_id = Uuid::from_slice(&frame[..TUNNEL_HEADER_LEN])
        .expect("a 16-byte UUID header is always valid");
    Ok((tunnel_id, payload))
}

fn validate_text(
    field: &'static str,
    value: &str,
    maximum_len: usize,
) -> Result<(), ProtocolError> {
    if value.trim().is_empty() {
        return Err(ProtocolError::MissingField(field));
    }
    if value.len() > maximum_len {
        return Err(ProtocolError::FieldTooLong(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> RelayDevice {
        RelayDevice {
            id: "device-a".into(),
            name: "Phone".into(),
            platform: "ios".into(),
            app_version: "0.1.9".into(),
            protocol_version: 1,
            min_protocol_version: 1,
            capabilities: vec!["file-transfer".into()],
        }
    }

    #[test]
    fn control_messages_use_stable_tagged_json() {
        let message = ClientControl::Register {
            relay_protocol_version: RELAY_PROTOCOL_VERSION,
            device: device(),
        };
        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains(r#""type":"register""#));
        assert!(json.contains(r#""relayProtocolVersion":1"#));
        assert_eq!(
            serde_json::from_str::<ClientControl>(&json).unwrap(),
            message
        );
    }

    #[test]
    fn tunnel_frames_round_trip_without_inspecting_payload() {
        let tunnel_id = Uuid::new_v4();
        let payload = b"opaque noise ciphertext";
        let encoded = encode_tunnel_frame(tunnel_id, payload).unwrap();
        let (decoded_id, decoded_payload) = decode_tunnel_frame(&encoded).unwrap();
        assert_eq!(decoded_id, tunnel_id);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn validation_rejects_malformed_device_metadata() {
        let mut malformed = device();
        malformed.min_protocol_version = 2;
        assert_eq!(
            malformed.validate(),
            Err(ProtocolError::InvalidProtocolRange)
        );
    }

    #[test]
    fn oversized_tunnel_frames_are_rejected() {
        let payload = vec![0_u8; MAX_RELAY_PAYLOAD + 1];
        assert_eq!(
            encode_tunnel_frame(Uuid::new_v4(), &payload),
            Err(ProtocolError::FrameTooLarge)
        );
    }
}
