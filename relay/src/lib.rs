use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use neloa_relay_protocol::{
    decode_tunnel_frame, ClientControl, RelayCloseReason, RelayDevice, RelayErrorCode,
    ServerControl, MAX_RELAY_PAYLOAD, RELAY_PROTOCOL_VERSION, TUNNEL_HEADER_LEN,
};
use serde::Serialize;
use subtle::ConstantTimeEq;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::timeout;
use tracing::info;
use uuid::Uuid;

const DEFAULT_BIND: &str = "0.0.0.0:8787";
const DEFAULT_MAX_DEVICES: usize = 64;
const MAX_CONFIGURED_DEVICES: usize = 4096;
const MINIMUM_TOKEN_LEN: usize = 32;
const MAXIMUM_TOKEN_LEN: usize = 512;
const REGISTER_TIMEOUT: Duration = Duration::from_secs(10);
const OUTBOUND_QUEUE_CAPACITY: usize = 256;
const MAX_CONTROL_MESSAGE_SIZE: usize = 32 * 1024;

#[derive(Clone, Debug)]
pub struct RelayConfig {
    pub bind: SocketAddr,
    pub token: String,
    pub max_devices: usize,
}

impl RelayConfig {
    pub fn from_env() -> Result<Self, String> {
        let bind = env::var("NELOA_RELAY_BIND")
            .unwrap_or_else(|_| DEFAULT_BIND.into())
            .parse::<SocketAddr>()
            .map_err(|error| format!("NELOA_RELAY_BIND is invalid: {error}"))?;
        let token = env::var("NELOA_RELAY_TOKEN")
            .map_err(|_| "NELOA_RELAY_TOKEN is required".to_string())?;
        validate_token(&token)?;
        let max_devices = env::var("NELOA_RELAY_MAX_DEVICES")
            .unwrap_or_else(|_| DEFAULT_MAX_DEVICES.to_string())
            .parse::<usize>()
            .map_err(|error| format!("NELOA_RELAY_MAX_DEVICES is invalid: {error}"))?;
        if !(1..=MAX_CONFIGURED_DEVICES).contains(&max_devices) {
            return Err(format!(
                "NELOA_RELAY_MAX_DEVICES must be between 1 and {MAX_CONFIGURED_DEVICES}"
            ));
        }
        Ok(Self {
            bind,
            token,
            max_devices,
        })
    }
}

#[derive(Clone)]
pub struct RelayState {
    inner: Arc<RelayStateInner>,
}

struct RelayStateInner {
    token: String,
    max_devices: usize,
    next_connection_id: AtomicU64,
    connections: RwLock<HashMap<String, Connection>>,
    tunnels: Mutex<HashMap<Uuid, Tunnel>>,
}

#[derive(Clone)]
struct Connection {
    connection_id: u64,
    device: RelayDevice,
    outbound: mpsc::Sender<Message>,
}

#[derive(Clone)]
struct Tunnel {
    source_id: String,
    target_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: &'static str,
    relay_protocol_version: u16,
}

#[derive(Debug)]
struct RelayFailure {
    code: RelayErrorCode,
    message: String,
    tunnel_id: Option<Uuid>,
}

impl RelayFailure {
    fn new(code: RelayErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            tunnel_id: None,
        }
    }

    fn for_tunnel(code: RelayErrorCode, tunnel_id: Uuid, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            tunnel_id: Some(tunnel_id),
        }
    }

    fn into_control(self) -> ServerControl {
        ServerControl::Error {
            code: self.code,
            message: self.message,
            tunnel_id: self.tunnel_id,
        }
    }
}

impl RelayState {
    pub fn new(token: impl Into<String>) -> Result<Self, String> {
        Self::with_max_devices(token, DEFAULT_MAX_DEVICES)
    }

    pub fn with_max_devices(token: impl Into<String>, max_devices: usize) -> Result<Self, String> {
        let token = token.into();
        validate_token(&token)?;
        if !(1..=MAX_CONFIGURED_DEVICES).contains(&max_devices) {
            return Err(format!(
                "max devices must be between 1 and {MAX_CONFIGURED_DEVICES}"
            ));
        }
        Ok(Self {
            inner: Arc::new(RelayStateInner {
                token,
                max_devices,
                next_connection_id: AtomicU64::new(1),
                connections: RwLock::new(HashMap::new()),
                tunnels: Mutex::new(HashMap::new()),
            }),
        })
    }

    fn next_connection_id(&self) -> u64 {
        self.inner
            .next_connection_id
            .fetch_add(1, Ordering::Relaxed)
    }

    fn authorized(&self, headers: &HeaderMap) -> bool {
        let Some(actual) = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
        else {
            return false;
        };
        let expected = format!("Bearer {}", self.inner.token);
        actual.as_bytes().ct_eq(expected.as_bytes()).into()
    }

    async fn register(
        &self,
        connection_id: u64,
        device: RelayDevice,
        outbound: mpsc::Sender<Message>,
    ) -> Result<(), RelayFailure> {
        let mut connections = self.inner.connections.write().await;
        if connections.contains_key(&device.id) {
            return Err(RelayFailure::new(
                RelayErrorCode::DuplicateDevice,
                "this device is already connected",
            ));
        }
        if connections.len() >= self.inner.max_devices {
            return Err(RelayFailure::new(
                RelayErrorCode::ServerFull,
                "relay has reached its configured device limit",
            ));
        }
        connections.insert(
            device.id.clone(),
            Connection {
                connection_id,
                device,
                outbound,
            },
        );
        Ok(())
    }

    async fn open_tunnel(
        &self,
        source_id: &str,
        tunnel_id: Uuid,
        target_id: String,
    ) -> Result<(Connection, Connection), RelayFailure> {
        if target_id == source_id {
            return Err(RelayFailure::for_tunnel(
                RelayErrorCode::InvalidTarget,
                tunnel_id,
                "a device cannot open a tunnel to itself",
            ));
        }
        let (source, target) = {
            let connections = self.inner.connections.read().await;
            let source = connections.get(source_id).cloned().ok_or_else(|| {
                RelayFailure::for_tunnel(
                    RelayErrorCode::RegistrationRequired,
                    tunnel_id,
                    "source device is no longer registered",
                )
            })?;
            let target = connections.get(&target_id).cloned().ok_or_else(|| {
                RelayFailure::for_tunnel(
                    RelayErrorCode::TargetOffline,
                    tunnel_id,
                    "target device is offline",
                )
            })?;
            (source, target)
        };

        let mut tunnels = self.inner.tunnels.lock().await;
        if tunnels.contains_key(&tunnel_id) {
            return Err(RelayFailure::for_tunnel(
                RelayErrorCode::TunnelAlreadyExists,
                tunnel_id,
                "tunnel id is already in use",
            ));
        }
        tunnels.insert(
            tunnel_id,
            Tunnel {
                source_id: source_id.into(),
                target_id,
            },
        );
        Ok((source, target))
    }

    async fn remove_tunnel(&self, tunnel_id: Uuid) -> Option<Tunnel> {
        self.inner.tunnels.lock().await.remove(&tunnel_id)
    }

    async fn route_for_tunnel(
        &self,
        sender_id: &str,
        tunnel_id: Uuid,
    ) -> Result<Connection, RelayFailure> {
        let recipient_id = {
            let tunnels = self.inner.tunnels.lock().await;
            let tunnel = tunnels.get(&tunnel_id).ok_or_else(|| {
                RelayFailure::for_tunnel(
                    RelayErrorCode::TunnelNotFound,
                    tunnel_id,
                    "tunnel does not exist",
                )
            })?;
            if tunnel.source_id == sender_id {
                tunnel.target_id.clone()
            } else if tunnel.target_id == sender_id {
                tunnel.source_id.clone()
            } else {
                return Err(RelayFailure::for_tunnel(
                    RelayErrorCode::TunnelAccessDenied,
                    tunnel_id,
                    "device is not a participant in this tunnel",
                ));
            }
        };

        self.inner
            .connections
            .read()
            .await
            .get(&recipient_id)
            .cloned()
            .ok_or_else(|| {
                RelayFailure::for_tunnel(
                    RelayErrorCode::TargetOffline,
                    tunnel_id,
                    "tunnel peer is offline",
                )
            })
    }

    async fn disconnect(&self, connection_id: u64, device_id: &str) {
        let removed = {
            let mut connections = self.inner.connections.write().await;
            if connections
                .get(device_id)
                .is_some_and(|connection| connection.connection_id == connection_id)
            {
                connections.remove(device_id);
                true
            } else {
                false
            }
        };
        if !removed {
            return;
        }

        let closed_tunnels = {
            let mut tunnels = self.inner.tunnels.lock().await;
            let ids = tunnels
                .iter()
                .filter_map(|(tunnel_id, tunnel)| {
                    (tunnel.source_id == device_id || tunnel.target_id == device_id)
                        .then_some(*tunnel_id)
                })
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|tunnel_id| {
                    tunnels.remove(&tunnel_id).map(|tunnel| (tunnel_id, tunnel))
                })
                .collect::<Vec<_>>()
        };

        for (tunnel_id, tunnel) in closed_tunnels {
            let other_id = if tunnel.source_id == device_id {
                tunnel.target_id
            } else {
                tunnel.source_id
            };
            if let Some(other) = self.inner.connections.read().await.get(&other_id).cloned() {
                let _ = send_control(
                    &other.outbound,
                    &ServerControl::TunnelClosed {
                        tunnel_id,
                        reason: RelayCloseReason::PeerDisconnected,
                    },
                );
            }
        }
        self.broadcast_presence().await;
        info!(connection_id, "relay device disconnected");
    }

    async fn broadcast_presence(&self) {
        let (mut devices, outbound) = {
            let connections = self.inner.connections.read().await;
            (
                connections
                    .values()
                    .map(|connection| connection.device.clone())
                    .collect::<Vec<_>>(),
                connections
                    .values()
                    .map(|connection| connection.outbound.clone())
                    .collect::<Vec<_>>(),
            )
        };
        devices.sort_by(|left, right| left.id.cmp(&right.id));
        let presence = ServerControl::Presence { devices };
        for sender in outbound {
            let _ = send_control(&sender, &presence);
        }
    }
}

pub fn router(state: RelayState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/ws", any(websocket))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        relay_protocol_version: RELAY_PROTOCOL_VERSION,
    })
}

async fn websocket(
    State(state): State<RelayState>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Response {
    if !state.authorized(&headers) {
        return (StatusCode::UNAUTHORIZED, "relay authentication required").into_response();
    }
    websocket
        .max_message_size(MAX_RELAY_PAYLOAD + TUNNEL_HEADER_LEN)
        .max_frame_size(MAX_RELAY_PAYLOAD + TUNNEL_HEADER_LEN)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: RelayState) {
    let connection_id = state.next_connection_id();
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let (outbound, mut outgoing) = mpsc::channel::<Message>(OUTBOUND_QUEUE_CAPACITY);
    let writer = tokio::spawn(async move {
        while let Some(message) = outgoing.recv().await {
            if socket_sender.send(message).await.is_err() {
                break;
            }
        }
    });

    let registration = timeout(REGISTER_TIMEOUT, socket_receiver.next()).await;
    let device = match registration {
        Ok(Some(Ok(Message::Text(text)))) if text.len() <= MAX_CONTROL_MESSAGE_SIZE => {
            match parse_registration(&text) {
                Ok(device) => device,
                Err(failure) => {
                    let _ = send_control(&outbound, &failure.into_control());
                    drop(outbound);
                    let _ = writer.await;
                    return;
                }
            }
        }
        _ => {
            let failure = RelayFailure::new(
                RelayErrorCode::RegistrationRequired,
                "the first relay message must register the device",
            );
            let _ = send_control(&outbound, &failure.into_control());
            drop(outbound);
            let _ = writer.await;
            return;
        }
    };

    if let Err(failure) = state
        .register(connection_id, device.clone(), outbound.clone())
        .await
    {
        let _ = send_control(&outbound, &failure.into_control());
        drop(outbound);
        let _ = writer.await;
        return;
    }
    let _ = send_control(
        &outbound,
        &ServerControl::Registered {
            relay_protocol_version: RELAY_PROTOCOL_VERSION,
        },
    );
    state.broadcast_presence().await;
    info!(connection_id, "relay device registered");

    while let Some(message) = socket_receiver.next().await {
        match message {
            Ok(Message::Text(text)) if text.len() <= MAX_CONTROL_MESSAGE_SIZE => {
                match serde_json::from_str::<ClientControl>(&text) {
                    Ok(control) => {
                        if let Err(failure) =
                            handle_control(&state, &device, &outbound, control).await
                        {
                            let _ = send_control(&outbound, &failure.into_control());
                        }
                    }
                    Err(_) => {
                        let failure = RelayFailure::new(
                            RelayErrorCode::InvalidMessage,
                            "control message is not valid relay JSON",
                        );
                        let _ = send_control(&outbound, &failure.into_control());
                    }
                }
            }
            Ok(Message::Binary(frame)) => {
                if let Err(failure) = handle_binary(&state, &device.id, frame).await {
                    let _ = send_control(&outbound, &failure.into_control());
                }
            }
            Ok(Message::Ping(payload)) => {
                if outbound.send(Message::Pong(payload)).await.is_err() {
                    break;
                }
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(Message::Text(_)) => {
                let failure = RelayFailure::new(
                    RelayErrorCode::InvalidMessage,
                    "control message exceeds the size limit",
                );
                let _ = send_control(&outbound, &failure.into_control());
            }
        }
    }

    state.disconnect(connection_id, &device.id).await;
    drop(outbound);
    writer.abort();
}

fn parse_registration(text: &str) -> Result<RelayDevice, RelayFailure> {
    let control = serde_json::from_str::<ClientControl>(text).map_err(|_| {
        RelayFailure::new(
            RelayErrorCode::InvalidMessage,
            "registration is not valid relay JSON",
        )
    })?;
    let ClientControl::Register {
        relay_protocol_version,
        device,
    } = control
    else {
        return Err(RelayFailure::new(
            RelayErrorCode::RegistrationRequired,
            "the first relay message must register the device",
        ));
    };
    if relay_protocol_version != RELAY_PROTOCOL_VERSION {
        return Err(RelayFailure::new(
            RelayErrorCode::IncompatibleProtocol,
            "relay protocol version is not supported",
        ));
    }
    device.validate().map_err(|error| {
        RelayFailure::new(
            RelayErrorCode::InvalidDevice,
            format!("device metadata is invalid: {error}"),
        )
    })?;
    Ok(device)
}

async fn handle_control(
    state: &RelayState,
    device: &RelayDevice,
    outbound: &mpsc::Sender<Message>,
    control: ClientControl,
) -> Result<(), RelayFailure> {
    match control {
        ClientControl::Register { .. } => Err(RelayFailure::new(
            RelayErrorCode::AlreadyRegistered,
            "device has already registered",
        )),
        ClientControl::Ping { nonce } => {
            let _ = send_control(outbound, &ServerControl::Pong { nonce });
            Ok(())
        }
        ClientControl::OpenTunnel {
            tunnel_id,
            target_id,
        } => {
            let (source, target) = state.open_tunnel(&device.id, tunnel_id, target_id).await?;
            if !send_control(
                &target.outbound,
                &ServerControl::IncomingTunnel {
                    tunnel_id,
                    source: source.device,
                },
            ) {
                state.remove_tunnel(tunnel_id).await;
                return Err(RelayFailure::for_tunnel(
                    RelayErrorCode::TargetOffline,
                    tunnel_id,
                    "target device disconnected before the tunnel opened",
                ));
            }
            let _ = send_control(
                outbound,
                &ServerControl::TunnelOpened {
                    tunnel_id,
                    target: target.device,
                },
            );
            Ok(())
        }
        ClientControl::CloseTunnel { tunnel_id } => {
            let recipient = state.route_for_tunnel(&device.id, tunnel_id).await?;
            state.remove_tunnel(tunnel_id).await;
            let closed = ServerControl::TunnelClosed {
                tunnel_id,
                reason: RelayCloseReason::Requested,
            };
            let _ = send_control(&recipient.outbound, &closed);
            let _ = send_control(outbound, &closed);
            Ok(())
        }
    }
}

async fn handle_binary(
    state: &RelayState,
    sender_id: &str,
    frame: axum::body::Bytes,
) -> Result<(), RelayFailure> {
    let (tunnel_id, _) = decode_tunnel_frame(&frame).map_err(|error| {
        RelayFailure::new(
            RelayErrorCode::InvalidMessage,
            format!("binary relay frame is invalid: {error}"),
        )
    })?;
    let recipient = state.route_for_tunnel(sender_id, tunnel_id).await?;
    match recipient.outbound.try_send(Message::Binary(frame)) {
        Ok(()) => Ok(()),
        Err(mpsc::error::TrySendError::Full(_)) => {
            state.remove_tunnel(tunnel_id).await;
            let closed = ServerControl::TunnelClosed {
                tunnel_id,
                reason: RelayCloseReason::Backpressure,
            };
            let _ = send_control(&recipient.outbound, &closed);
            Err(RelayFailure::for_tunnel(
                RelayErrorCode::Backpressure,
                tunnel_id,
                "recipient cannot keep up with relay traffic",
            ))
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            state.remove_tunnel(tunnel_id).await;
            Err(RelayFailure::for_tunnel(
                RelayErrorCode::TargetOffline,
                tunnel_id,
                "tunnel peer disconnected",
            ))
        }
    }
}

fn send_control(sender: &mpsc::Sender<Message>, control: &ServerControl) -> bool {
    let Ok(json) = serde_json::to_string(control) else {
        return false;
    };
    sender.try_send(Message::Text(json.into())).is_ok()
}

fn validate_token(token: &str) -> Result<(), String> {
    if token.len() < MINIMUM_TOKEN_LEN {
        return Err(format!(
            "NELOA_RELAY_TOKEN must contain at least {MINIMUM_TOKEN_LEN} characters"
        ));
    }
    if token.len() > MAXIMUM_TOKEN_LEN {
        return Err(format!(
            "NELOA_RELAY_TOKEN must contain no more than {MAXIMUM_TOKEN_LEN} characters"
        ));
    }
    if token.chars().any(char::is_whitespace) {
        return Err("NELOA_RELAY_TOKEN must not contain whitespace".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_policy_rejects_short_or_ambiguous_values() {
        assert!(validate_token("short").is_err());
        assert!(validate_token(&"x".repeat(MINIMUM_TOKEN_LEN)).is_ok());
        assert!(validate_token(&format!("{} ", "x".repeat(MINIMUM_TOKEN_LEN))).is_err());
    }

    #[test]
    fn registration_requires_current_protocol_and_valid_device() {
        let valid = RelayDevice {
            id: "device-a".into(),
            name: "Phone".into(),
            platform: "ios".into(),
            app_version: "0.1.9".into(),
            protocol_version: 1,
            min_protocol_version: 1,
            capabilities: vec![],
        };
        let wrong_version = serde_json::to_string(&ClientControl::Register {
            relay_protocol_version: RELAY_PROTOCOL_VERSION + 1,
            device: valid.clone(),
        })
        .unwrap();
        assert_eq!(
            parse_registration(&wrong_version).unwrap_err().code,
            RelayErrorCode::IncompatibleProtocol
        );

        let valid = serde_json::to_string(&ClientControl::Register {
            relay_protocol_version: RELAY_PROTOCOL_VERSION,
            device: valid,
        })
        .unwrap();
        assert_eq!(parse_registration(&valid).unwrap().id, "device-a");
    }

    #[test]
    fn configured_device_limit_is_bounded() {
        assert!(RelayState::with_max_devices("x".repeat(MINIMUM_TOKEN_LEN), 0).is_err());
        assert!(RelayState::with_max_devices("x".repeat(MINIMUM_TOKEN_LEN), 1).is_ok());
        assert!(RelayState::with_max_devices(
            "x".repeat(MINIMUM_TOKEN_LEN),
            MAX_CONFIGURED_DEVICES + 1
        )
        .is_err());
    }
}
