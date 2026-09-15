use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use neloa_relay_protocol::{
    decode_tunnel_frame, encode_tunnel_frame, ClientControl, RelayDevice, ServerControl,
    MAX_RELAY_PAYLOAD, RELAY_PROTOCOL_VERSION, TUNNEL_HEADER_LEN,
};
use parking_lot::RwLock;
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{duplex, split, AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf},
    net::TcpStream,
    sync::{mpsc, oneshot, watch},
    time::{interval, sleep, timeout, MissedTickBehavior},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        client::IntoClientRequest,
        http::{header::AUTHORIZATION, HeaderValue},
        protocol::WebSocketConfig,
        Message,
    },
    MaybeTlsStream, WebSocketStream,
};
use uuid::Uuid;

use crate::{
    model::{LocalDevice, PeerDevice, RelaySnapshot, CAPABILITIES},
    relay_settings::{RelayConnectionConfig, RelayDirective},
    trust::TrustStore,
    unix_millis,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const OPEN_TUNNEL_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);
const RECONFIGURE_DELAY: Duration = Duration::from_millis(200);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(25);
const COMMAND_CAPACITY: usize = 32;
const INCOMING_TUNNEL_CAPACITY: usize = 16;
const TUNNEL_EVENT_CAPACITY: usize = 256;
const TUNNEL_INBOUND_CAPACITY: usize = 32;
const TUNNEL_STREAM_BUFFER: usize = 256 * 1024;
const TUNNEL_READ_BUFFER: usize = 64 * 1024;
const MAX_WEBSOCKET_MESSAGE: usize = MAX_RELAY_PAYLOAD + TUNNEL_HEADER_LEN;

type RelaySocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type RelayWriter = SplitSink<RelaySocket, Message>;
type RelayReader = SplitStream<RelaySocket>;

pub(crate) struct RelayTunnel {
    pub send: WriteHalf<DuplexStream>,
    pub receive: ReadHalf<DuplexStream>,
}

pub(crate) struct IncomingRelayTunnel {
    pub source_id: String,
    pub tunnel: RelayTunnel,
}

struct OpenTunnelRequest {
    target_id: String,
    response: oneshot::Sender<Result<RelayTunnel, String>>,
}

struct PendingOpen {
    target_id: String,
    response: oneshot::Sender<Result<RelayTunnel, String>>,
}

enum TunnelEvent {
    Payload { tunnel_id: Uuid, bytes: Vec<u8> },
    Closed { tunnel_id: Uuid },
}

enum ConnectionExit {
    Reconfigure,
    Failed(String),
}

#[derive(Clone)]
pub(crate) struct RelayClientHandle {
    directive: watch::Sender<RelayDirective>,
    device: watch::Sender<LocalDevice>,
    commands: mpsc::Sender<OpenTunnelRequest>,
    status: Arc<RwLock<RelaySnapshot>>,
}

pub(crate) struct RelayClientWorker {
    directive: watch::Receiver<RelayDirective>,
    device: watch::Receiver<LocalDevice>,
    commands: mpsc::Receiver<OpenTunnelRequest>,
    incoming: mpsc::Sender<IncomingRelayTunnel>,
    status: Arc<RwLock<RelaySnapshot>>,
}

pub(crate) fn relay_client_channel(
    directive: RelayDirective,
    device: LocalDevice,
) -> (
    RelayClientHandle,
    RelayClientWorker,
    mpsc::Receiver<IncomingRelayTunnel>,
) {
    let initial = directive.initial_snapshot();
    let status = Arc::new(RwLock::new(initial));
    let (directive_sender, directive_receiver) = watch::channel(directive);
    let (device_sender, device_receiver) = watch::channel(device);
    let (command_sender, command_receiver) = mpsc::channel(COMMAND_CAPACITY);
    let (incoming_sender, incoming_receiver) = mpsc::channel(INCOMING_TUNNEL_CAPACITY);
    (
        RelayClientHandle {
            directive: directive_sender,
            device: device_sender,
            commands: command_sender,
            status: Arc::clone(&status),
        },
        RelayClientWorker {
            directive: directive_receiver,
            device: device_receiver,
            commands: command_receiver,
            incoming: incoming_sender,
            status,
        },
        incoming_receiver,
    )
}

impl RelayClientHandle {
    pub(crate) fn unavailable(
        directive: RelayDirective,
        device: LocalDevice,
        error: String,
    ) -> Self {
        let (directive_sender, _) = watch::channel(directive.clone());
        let (device_sender, _) = watch::channel(device);
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_CAPACITY);
        drop(command_receiver);
        let mut status = directive.initial_snapshot();
        status.connected = false;
        status.error = Some(error);
        Self {
            directive: directive_sender,
            device: device_sender,
            commands: command_sender,
            status: Arc::new(RwLock::new(status)),
        }
    }

    pub(crate) fn configure(&self, directive: RelayDirective) -> Result<(), String> {
        *self.status.write() = directive.initial_snapshot();
        self.directive
            .send(directive)
            .map_err(|_| "中继连接服务未运行".to_string())?;
        Ok(())
    }

    pub(crate) fn status(&self) -> RelaySnapshot {
        self.status.read().clone()
    }

    pub(crate) fn update_device(&self, device: LocalDevice) -> Result<(), String> {
        self.device
            .send(device)
            .map_err(|_| "中继连接服务未运行".to_string())
    }

    pub(crate) async fn open_tunnel(&self, target_id: String) -> Result<RelayTunnel, String> {
        let status = self.status.read().clone();
        if !status.connected {
            return Err(status.error.unwrap_or_else(|| "中继尚未连接".into()));
        }
        let (response, result) = oneshot::channel();
        self.commands
            .send(OpenTunnelRequest {
                target_id,
                response,
            })
            .await
            .map_err(|_| "中继连接服务未运行".to_string())?;
        timeout(OPEN_TUNNEL_TIMEOUT, result)
            .await
            .map_err(|_| "打开中继隧道超时".to_string())?
            .map_err(|_| "中继连接在隧道建立前中断".to_string())?
    }
}

impl RelayClientWorker {
    pub(crate) async fn run(
        mut self,
        app: AppHandle,
        trust: TrustStore,
        peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    ) {
        loop {
            let directive = self.directive.borrow().clone();
            let initial_snapshot = directive.initial_snapshot();
            match directive {
                RelayDirective::Disabled { .. } => {
                    clear_relay_presence(&peers);
                    publish_status(&app, &self.status, directive.initial_snapshot());
                    if !wait_for_reconfiguration(&mut self.directive, &mut self.commands, None)
                        .await
                    {
                        return;
                    }
                }
                RelayDirective::Invalid { ref error, .. } => {
                    clear_relay_presence(&peers);
                    publish_status(&app, &self.status, directive.initial_snapshot());
                    if !wait_for_reconfiguration(
                        &mut self.directive,
                        &mut self.commands,
                        Some(error.clone()),
                    )
                    .await
                    {
                        return;
                    }
                }
                RelayDirective::Connect(config) => {
                    let local = self.device.borrow_and_update().clone();
                    let mut connecting = initial_snapshot.clone();
                    connecting.error = None;
                    publish_status(&app, &self.status, connecting);
                    match run_connection(
                        &app,
                        &local,
                        &trust,
                        &peers,
                        &self.status,
                        &mut self.directive,
                        &mut self.device,
                        &mut self.commands,
                        &self.incoming,
                        config,
                    )
                    .await
                    {
                        ConnectionExit::Reconfigure => {
                            clear_relay_presence(&peers);
                            sleep(RECONFIGURE_DELAY).await;
                        }
                        ConnectionExit::Failed(error) => {
                            clear_relay_presence(&peers);
                            let mut failed = initial_snapshot;
                            failed.error = Some(error.clone());
                            publish_status(&app, &self.status, failed);
                            if !wait_for_reconfiguration(
                                &mut self.directive,
                                &mut self.commands,
                                Some(error),
                            )
                            .await
                            {
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn wait_for_reconfiguration(
    directive: &mut watch::Receiver<RelayDirective>,
    commands: &mut mpsc::Receiver<OpenTunnelRequest>,
    unavailable_message: Option<String>,
) -> bool {
    loop {
        tokio::select! {
            changed = directive.changed() => return changed.is_ok(),
            request = commands.recv() => {
                let Some(request) = request else { return false; };
                let message = unavailable_message.clone().unwrap_or_else(|| "中继未启用".into());
                let _ = request.response.send(Err(message));
            }
            _ = sleep(RECONNECT_DELAY), if unavailable_message.is_some() => return true,
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_connection(
    app: &AppHandle,
    local: &LocalDevice,
    trust: &TrustStore,
    peers: &Arc<RwLock<HashMap<String, PeerDevice>>>,
    status: &Arc<RwLock<RelaySnapshot>>,
    directive: &mut watch::Receiver<RelayDirective>,
    device: &mut watch::Receiver<LocalDevice>,
    commands: &mut mpsc::Receiver<OpenTunnelRequest>,
    incoming: &mpsc::Sender<IncomingRelayTunnel>,
    config: RelayConnectionConfig,
) -> ConnectionExit {
    let socket = match connect_socket(&config).await {
        Ok(socket) => socket,
        Err(error) => return ConnectionExit::Failed(error),
    };
    let (mut writer, mut reader) = socket.split();
    let registration = ClientControl::Register {
        relay_protocol_version: RELAY_PROTOCOL_VERSION,
        device: RelayDevice {
            id: local.id.clone(),
            name: local.name.clone(),
            platform: local.platform.clone(),
            app_version: local.version.clone(),
            protocol_version: crate::model::PROTOCOL_VERSION,
            min_protocol_version: crate::model::MIN_PROTOCOL_VERSION,
            capabilities: CAPABILITIES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        },
    };
    if let Err(error) = send_control(&mut writer, &registration).await {
        return ConnectionExit::Failed(error);
    }
    match wait_for_registration(&mut reader).await {
        Ok(()) => {}
        Err(error) => return ConnectionExit::Failed(error),
    }

    let mut connected = config_snapshot(&config);
    connected.connected = true;
    publish_status(app, status, connected);

    let (tunnel_events, mut tunnel_event_receiver) = mpsc::channel(TUNNEL_EVENT_CAPACITY);
    let mut tunnels: HashMap<Uuid, mpsc::Sender<Vec<u8>>> = HashMap::new();
    let mut pending: HashMap<Uuid, PendingOpen> = HashMap::new();
    let mut keepalive = interval(KEEPALIVE_INTERVAL);
    keepalive.set_missed_tick_behavior(MissedTickBehavior::Delay);
    keepalive.tick().await;
    let mut nonce = 0_u64;

    loop {
        tokio::select! {
            changed = directive.changed() => {
                let exit = if changed.is_ok() {
                    ConnectionExit::Reconfigure
                } else {
                    ConnectionExit::Failed("中继设置服务已停止".into())
                };
                let _ = writer.send(Message::Close(None)).await;
                return exit;
            }
            changed = device.changed() => {
                let exit = if changed.is_ok() {
                    ConnectionExit::Reconfigure
                } else {
                    ConnectionExit::Failed("设备设置服务已停止".into())
                };
                let _ = writer.send(Message::Close(None)).await;
                return exit;
            }
            request = commands.recv() => {
                let Some(request) = request else {
                    return ConnectionExit::Failed("中继连接服务已停止".into());
                };
                if trust.find(&request.target_id).is_none() {
                    let _ = request.response.send(Err("只允许通过中继连接已配对设备".into()));
                    continue;
                }
                let tunnel_id = Uuid::new_v4();
                let control = ClientControl::OpenTunnel {
                    tunnel_id,
                    target_id: request.target_id.clone(),
                };
                if let Err(error) = send_control(&mut writer, &control).await {
                    let _ = request.response.send(Err(error.clone()));
                    return ConnectionExit::Failed(error);
                }
                pending.insert(tunnel_id, PendingOpen {
                    target_id: request.target_id,
                    response: request.response,
                });
            }
            event = tunnel_event_receiver.recv() => {
                let Some(event) = event else {
                    return ConnectionExit::Failed("中继隧道队列意外关闭".into());
                };
                match event {
                    TunnelEvent::Payload { tunnel_id, bytes } => {
                        let frame = match encode_tunnel_frame(tunnel_id, &bytes) {
                            Ok(frame) => frame,
                            Err(error) => {
                                tunnels.remove(&tunnel_id);
                                let _ = send_control(&mut writer, &ClientControl::CloseTunnel { tunnel_id }).await;
                                if let Some(open) = pending.remove(&tunnel_id) {
                                    let _ = open.response.send(Err(format!("中继数据帧无效：{error}")));
                                }
                                continue;
                            }
                        };
                        if let Err(error) = writer.send(Message::Binary(frame.into())).await {
                            return ConnectionExit::Failed(format!("无法发送中继数据：{error}"));
                        }
                    }
                    TunnelEvent::Closed { tunnel_id } => {
                        if tunnels.remove(&tunnel_id).is_some() {
                            let _ = send_control(&mut writer, &ClientControl::CloseTunnel { tunnel_id }).await;
                        }
                    }
                }
            }
            message = reader.next() => {
                let message = match message {
                    Some(Ok(message)) => message,
                    Some(Err(error)) => return ConnectionExit::Failed(format!("中继连接异常：{error}")),
                    None => return ConnectionExit::Failed("中继连接已关闭".into()),
                };
                match message {
                    Message::Text(text) => {
                        let control = match serde_json::from_str::<ServerControl>(&text) {
                            Ok(control) => control,
                            Err(error) => return ConnectionExit::Failed(format!("中继返回了无效控制消息：{error}")),
                        };
                        match control {
                            ServerControl::Presence { devices } => {
                                let online_devices = apply_relay_presence(peers, trust, &local.id, devices);
                                let mut next = config_snapshot(&config);
                                next.connected = true;
                                next.online_devices = online_devices;
                                publish_status(app, status, next);
                            }
                            ServerControl::IncomingTunnel { tunnel_id, source } => {
                                if source.id == local.id || trust.find(&source.id).is_none() || tunnels.contains_key(&tunnel_id) {
                                    let _ = send_control(&mut writer, &ClientControl::CloseTunnel { tunnel_id }).await;
                                    continue;
                                }
                                let (tunnel, inbound) = spawn_tunnel(tunnel_id, tunnel_events.clone());
                                tunnels.insert(tunnel_id, inbound);
                                if incoming.try_send(IncomingRelayTunnel {
                                    source_id: source.id,
                                    tunnel,
                                }).is_err() {
                                    tunnels.remove(&tunnel_id);
                                    let _ = send_control(&mut writer, &ClientControl::CloseTunnel { tunnel_id }).await;
                                }
                            }
                            ServerControl::TunnelOpened { tunnel_id, target } => {
                                let Some(open) = pending.remove(&tunnel_id) else {
                                    let _ = send_control(&mut writer, &ClientControl::CloseTunnel { tunnel_id }).await;
                                    continue;
                                };
                                if target.id != open.target_id {
                                    let _ = open.response.send(Err("中继返回的目标设备不匹配".into()));
                                    let _ = send_control(&mut writer, &ClientControl::CloseTunnel { tunnel_id }).await;
                                    continue;
                                }
                                let (tunnel, inbound) = spawn_tunnel(tunnel_id, tunnel_events.clone());
                                if open.response.send(Ok(tunnel)).is_ok() {
                                    tunnels.insert(tunnel_id, inbound);
                                } else {
                                    let _ = send_control(&mut writer, &ClientControl::CloseTunnel { tunnel_id }).await;
                                }
                            }
                            ServerControl::TunnelClosed { tunnel_id, reason } => {
                                tunnels.remove(&tunnel_id);
                                if let Some(open) = pending.remove(&tunnel_id) {
                                    let _ = open.response.send(Err(format!("中继隧道已关闭：{reason:?}")));
                                }
                            }
                            ServerControl::Error { code, message, tunnel_id } => {
                                let detail = format!("中继拒绝请求（{code:?}）：{message}");
                                if let Some(tunnel_id) = tunnel_id {
                                    tunnels.remove(&tunnel_id);
                                    if let Some(open) = pending.remove(&tunnel_id) {
                                        let _ = open.response.send(Err(detail));
                                    }
                                } else {
                                    return ConnectionExit::Failed(detail);
                                }
                            }
                            ServerControl::Pong { .. } => {}
                            ServerControl::Registered { .. } => {
                                return ConnectionExit::Failed("中继重复发送了注册确认".into());
                            }
                        }
                    }
                    Message::Binary(frame) => {
                        let (tunnel_id, payload) = match decode_tunnel_frame(&frame) {
                            Ok(decoded) => decoded,
                            Err(error) => return ConnectionExit::Failed(format!("中继返回了无效数据帧：{error}")),
                        };
                        let Some(inbound) = tunnels.get(&tunnel_id).cloned() else {
                            let _ = send_control(&mut writer, &ClientControl::CloseTunnel { tunnel_id }).await;
                            continue;
                        };
                        if inbound.try_send(payload.to_vec()).is_err() {
                            tunnels.remove(&tunnel_id);
                            let _ = send_control(&mut writer, &ClientControl::CloseTunnel { tunnel_id }).await;
                        }
                    }
                    Message::Ping(payload) => {
                        if let Err(error) = writer.send(Message::Pong(payload)).await {
                            return ConnectionExit::Failed(format!("无法回应中继保活消息：{error}"));
                        }
                    }
                    Message::Pong(_) => {}
                    Message::Close(_) => return ConnectionExit::Failed("中继连接已关闭".into()),
                    Message::Frame(_) => {}
                }
            }
            _ = keepalive.tick() => {
                nonce = nonce.wrapping_add(1);
                if let Err(error) = send_control(&mut writer, &ClientControl::Ping { nonce }).await {
                    return ConnectionExit::Failed(error);
                }
            }
        }
    }
}

async fn connect_socket(config: &RelayConnectionConfig) -> Result<RelaySocket, String> {
    let mut request = config
        .url
        .as_str()
        .into_client_request()
        .map_err(|error| format!("无法创建中继连接请求：{error}"))?;
    let mut value = HeaderValue::from_str(&format!("Bearer {}", config.token.as_str()))
        .map_err(|_| "中继令牌无法写入请求头".to_string())?;
    value.set_sensitive(true);
    request.headers_mut().insert(AUTHORIZATION, value);
    let mut websocket_config = WebSocketConfig::default();
    websocket_config.max_message_size = Some(MAX_WEBSOCKET_MESSAGE);
    websocket_config.max_frame_size = Some(MAX_WEBSOCKET_MESSAGE);
    timeout(
        CONNECT_TIMEOUT,
        connect_async_with_config(request, Some(websocket_config), false),
    )
    .await
    .map_err(|_| "连接中继超时".to_string())?
    .map(|(socket, _)| socket)
    .map_err(|error| format!("无法连接中继：{error}"))
}

async fn wait_for_registration(reader: &mut RelayReader) -> Result<(), String> {
    let message = timeout(CONNECT_TIMEOUT, reader.next())
        .await
        .map_err(|_| "等待中继注册确认超时".to_string())?
        .ok_or_else(|| "中继在注册完成前关闭了连接".to_string())?
        .map_err(|error| format!("无法读取中继注册确认：{error}"))?;
    let Message::Text(text) = message else {
        return Err("中继注册确认格式无效".into());
    };
    match serde_json::from_str::<ServerControl>(&text)
        .map_err(|error| format!("中继注册确认不是有效 JSON：{error}"))?
    {
        ServerControl::Registered {
            relay_protocol_version,
        } if relay_protocol_version == RELAY_PROTOCOL_VERSION => Ok(()),
        ServerControl::Registered { .. } => Err("中继协议版本不兼容".into()),
        ServerControl::Error { code, message, .. } => {
            Err(format!("中继注册失败（{code:?}）：{message}"))
        }
        _ => Err("中继未返回注册确认".into()),
    }
}

async fn send_control(writer: &mut RelayWriter, control: &ClientControl) -> Result<(), String> {
    let json =
        serde_json::to_string(control).map_err(|error| format!("无法编码中继控制消息：{error}"))?;
    writer
        .send(Message::Text(json.into()))
        .await
        .map_err(|error| format!("无法发送中继控制消息：{error}"))
}

fn spawn_tunnel(
    tunnel_id: Uuid,
    events: mpsc::Sender<TunnelEvent>,
) -> (RelayTunnel, mpsc::Sender<Vec<u8>>) {
    let (session, pump) = duplex(TUNNEL_STREAM_BUFFER);
    let (receive, send) = split(session);
    let (inbound, inbound_receiver) = mpsc::channel(TUNNEL_INBOUND_CAPACITY);
    tokio::spawn(run_tunnel_pump(tunnel_id, pump, inbound_receiver, events));
    (RelayTunnel { send, receive }, inbound)
}

async fn run_tunnel_pump(
    tunnel_id: Uuid,
    stream: DuplexStream,
    mut inbound: mpsc::Receiver<Vec<u8>>,
    events: mpsc::Sender<TunnelEvent>,
) {
    let (mut reader, mut writer) = split(stream);
    let mut buffer = vec![0_u8; TUNNEL_READ_BUFFER];
    loop {
        tokio::select! {
            read = reader.read(&mut buffer) => {
                let Ok(size) = read else { break; };
                if size == 0 {
                    break;
                }
                if events.send(TunnelEvent::Payload {
                    tunnel_id,
                    bytes: buffer[..size].to_vec(),
                }).await.is_err() {
                    return;
                }
            }
            bytes = inbound.recv() => {
                let Some(bytes) = bytes else { break; };
                if writer.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = events.send(TunnelEvent::Closed { tunnel_id }).await;
}

fn apply_relay_presence(
    peers: &Arc<RwLock<HashMap<String, PeerDevice>>>,
    trust: &TrustStore,
    local_id: &str,
    devices: Vec<RelayDevice>,
) -> usize {
    let devices = devices
        .into_iter()
        .filter(|device| device.id != local_id && trust.find(&device.id).is_some())
        .map(|device| (device.id.clone(), device))
        .collect::<HashMap<_, _>>();
    let online_ids = devices.keys().cloned().collect::<HashSet<_>>();
    let mut peers = peers.write();
    peers.retain(|id, peer| {
        if peer.relay_available && !online_ids.contains(id) {
            peer.relay_available = false;
            return !peer.addresses.is_empty();
        }
        true
    });
    let now = unix_millis();
    for (id, device) in &devices {
        if let Some(peer) = peers.get_mut(id) {
            peer.relay_available = true;
            peer.last_seen_ms = now;
            if peer.addresses.is_empty() {
                peer.name.clone_from(&device.name);
                peer.platform.clone_from(&device.platform);
                peer.version.clone_from(&device.app_version);
                peer.protocol_version = device.protocol_version;
                peer.min_protocol_version = device.min_protocol_version;
                peer.capabilities.clone_from(&device.capabilities);
            }
        } else {
            peers.insert(
                id.clone(),
                PeerDevice {
                    id: id.clone(),
                    name: device.name.clone(),
                    platform: device.platform.clone(),
                    version: device.app_version.clone(),
                    protocol_version: device.protocol_version,
                    min_protocol_version: device.min_protocol_version,
                    capabilities: device.capabilities.clone(),
                    addresses: Vec::new(),
                    port: 0,
                    last_seen_ms: now,
                    relay_available: true,
                    service_fullname: String::new(),
                },
            );
        }
    }
    devices.len()
}

fn clear_relay_presence(peers: &Arc<RwLock<HashMap<String, PeerDevice>>>) {
    peers.write().retain(|_, peer| {
        peer.relay_available = false;
        !peer.addresses.is_empty()
    });
}

fn config_snapshot(config: &RelayConnectionConfig) -> RelaySnapshot {
    RelaySnapshot {
        enabled: true,
        url: config.url.clone(),
        has_token: true,
        connected: false,
        online_devices: 0,
        error: None,
    }
}

fn publish_status(app: &AppHandle, status: &Arc<RwLock<RelaySnapshot>>, snapshot: RelaySnapshot) {
    *status.write() = snapshot.clone();
    let _ = app.emit("relay-status-changed", snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrustedDevice;

    fn relay_device(id: &str) -> RelayDevice {
        RelayDevice {
            id: id.into(),
            name: format!("Device {id}"),
            platform: "test".into(),
            app_version: "test".into(),
            protocol_version: crate::model::PROTOCOL_VERSION,
            min_protocol_version: crate::model::MIN_PROTOCOL_VERSION,
            capabilities: CAPABILITIES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        }
    }

    #[tokio::test]
    async fn relay_tunnel_pump_moves_bytes_in_both_directions() {
        let tunnel_id = Uuid::new_v4();
        let (events, mut event_receiver) = mpsc::channel(TUNNEL_EVENT_CAPACITY);
        let (mut tunnel, inbound) = spawn_tunnel(tunnel_id, events);

        tunnel.send.write_all(b"outbound").await.unwrap();
        match event_receiver.recv().await.unwrap() {
            TunnelEvent::Payload {
                tunnel_id: actual,
                bytes,
            } => {
                assert_eq!(actual, tunnel_id);
                assert_eq!(bytes, b"outbound");
            }
            TunnelEvent::Closed { .. } => panic!("tunnel closed before outbound data arrived"),
        }

        inbound.send(b"inbound".to_vec()).await.unwrap();
        let mut received = [0_u8; 7];
        tunnel.receive.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, b"inbound");

        tunnel.send.shutdown().await.unwrap();
        let closed = timeout(Duration::from_secs(1), event_receiver.recv())
            .await
            .expect("tunnel pump did not close")
            .unwrap();
        match closed {
            TunnelEvent::Closed { tunnel_id: actual } => assert_eq!(actual, tunnel_id),
            TunnelEvent::Payload { .. } => panic!("unexpected payload after closing tunnel"),
        }
    }

    #[test]
    fn relay_presence_exposes_only_previously_trusted_devices() {
        let directory = tempfile::tempdir().unwrap();
        let trust = TrustStore::load(directory.path().join("trusted-devices.json")).unwrap();
        trust
            .upsert(TrustedDevice {
                id: "trusted".into(),
                name: "Trusted".into(),
                alias: None,
                platform: "test".into(),
                public_key: "00".repeat(32),
                fingerprint: "00:00:00:00".into(),
                paired_at_ms: 1,
                last_verified_ms: 1,
            })
            .unwrap();
        let peers = Arc::new(RwLock::new(HashMap::new()));

        let online = apply_relay_presence(
            &peers,
            &trust,
            "local",
            vec![
                relay_device("local"),
                relay_device("trusted"),
                relay_device("untrusted"),
            ],
        );

        assert_eq!(online, 1);
        assert_eq!(peers.read().len(), 1);
        assert!(peers.read().get("trusted").unwrap().relay_available);
        assert!(!peers.read().contains_key("untrusted"));

        apply_relay_presence(&peers, &trust, "local", Vec::new());
        assert!(peers.read().is_empty());
    }
}
