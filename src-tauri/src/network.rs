use std::{
    collections::HashMap,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    thread,
    time::{Duration, Instant},
};

use parking_lot::{Mutex, RwLock};
use quinn::{crypto::rustls::QuicClientConfig, ClientConfig, Endpoint, RecvStream, SendStream};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::CryptoProvider,
    pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snow::{params::NoiseParams, Builder, TransportState};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
    sync::{mpsc, oneshot, watch},
    time::timeout,
};
use uuid::Uuid;

use crate::{
    clipboard::{validate_clipboard_text, ClipboardService},
    identity::{fingerprint, NoiseIdentity},
    model::{
        ClipboardSyncEvent, FileOfferEvent, FileTransferProgress, FileTransferResult, LocalDevice,
        NetworkStatus, PairingPeer, PairingRequest, PairingResult, PeerDevice, TestMessageEvent,
        TrustedDevice, CAPABILITIES, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
    },
    relay_client::{
        relay_client_channel, IncomingRelayTunnel, RelayClientHandle, RelayClientWorker,
        RelayTunnel,
    },
    relay_settings::RelayDirective,
    trust::TrustStore,
    unix_millis,
};

const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
const FRAME_LIMIT: usize = 64 * 1024;
const TEXT_LIMIT: usize = 4096;
const FILE_CHUNK_SIZE: usize = 48 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const PAIRING_TIMEOUT: Duration = Duration::from_secs(120);
const FILE_OFFER_TIMEOUT: Duration = Duration::from_secs(120);
const FILE_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const REPAIR_REQUIRED_MESSAGE: &str = "两端信任状态不一致，请重新配对后再试";

type PendingConfirmations = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;
type PendingFileOffers = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;
type TransferCancellation = watch::Receiver<bool>;
type ActiveTransfers = Arc<Mutex<HashMap<String, watch::Sender<bool>>>>;

enum SessionSend {
    Quic(SendStream),
    Relay(tokio::io::WriteHalf<tokio::io::DuplexStream>),
}

enum SessionReceive {
    Quic(RecvStream),
    Relay(tokio::io::ReadHalf<tokio::io::DuplexStream>),
}

impl AsyncWrite for SessionSend {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Quic(stream) => {
                <SendStream as AsyncWrite>::poll_write(Pin::new(stream), context, buffer)
            }
            Self::Relay(stream) => Pin::new(stream).poll_write(context, buffer),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Quic(stream) => <SendStream as AsyncWrite>::poll_flush(Pin::new(stream), context),
            Self::Relay(stream) => Pin::new(stream).poll_flush(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Quic(stream) => {
                <SendStream as AsyncWrite>::poll_shutdown(Pin::new(stream), context)
            }
            Self::Relay(stream) => Pin::new(stream).poll_shutdown(context),
        }
    }
}

impl AsyncRead for SessionReceive {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Quic(stream) => Pin::new(stream).poll_read(context, buffer),
            Self::Relay(stream) => Pin::new(stream).poll_read(context, buffer),
        }
    }
}

#[derive(Clone)]
struct TransportConnector {
    quic: Endpoint,
    relay: RelayClientHandle,
}

impl TransportConnector {
    async fn connect(&self, peer: &PeerDevice) -> Result<(SessionSend, SessionReceive), String> {
        let lan_error = if peer.addresses.is_empty() {
            None
        } else {
            match connect_quic(&self.quic, peer).await {
                Ok((send, receive)) => {
                    return Ok((SessionSend::Quic(send), SessionReceive::Quic(receive)));
                }
                Err(error) => Some(error),
            }
        };
        if peer.relay_available {
            match self.relay.open_tunnel(peer.id.clone()).await {
                Ok(RelayTunnel { send, receive }) => {
                    return Ok((SessionSend::Relay(send), SessionReceive::Relay(receive)));
                }
                Err(relay_error) => {
                    if let Some(lan_error) = lan_error {
                        return Err(format!(
                            "局域网连接失败：{lan_error}；中继回退失败：{relay_error}"
                        ));
                    }
                    return Err(relay_error);
                }
            }
        }
        match lan_error {
            Some(error) => Err(error),
            _ => Err("设备当前没有可用的局域网或中继连接".into()),
        }
    }
}

#[derive(Clone)]
struct NetworkContext {
    app: AppHandle,
    local: LocalDevice,
    identity: NoiseIdentity,
    trust: TrustStore,
    clipboard: ClipboardService,
}

pub(crate) struct NetworkStartup {
    pub(crate) app: AppHandle,
    pub(crate) local: LocalDevice,
    pub(crate) identity: NoiseIdentity,
    pub(crate) trust: TrustStore,
    pub(crate) clipboard: ClipboardService,
    pub(crate) port: u16,
    pub(crate) peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    pub(crate) relay_directive: RelayDirective,
}

struct NetworkRuntime {
    context: NetworkContext,
    port: u16,
    commands: mpsc::UnboundedReceiver<NetworkCommand>,
    status: Arc<RwLock<NetworkStatus>>,
    peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    relay: RelayClientHandle,
    relay_worker: RelayClientWorker,
    relay_incoming: mpsc::Receiver<IncomingRelayTunnel>,
}

#[derive(Clone)]
struct PairingContext {
    app: AppHandle,
    local: LocalDevice,
    identity: NoiseIdentity,
    trust: TrustStore,
    pending: PendingConfirmations,
}

#[derive(Clone)]
struct TrustedSendContext {
    app: AppHandle,
    local: LocalDevice,
    identity: NoiseIdentity,
    trust: TrustStore,
}

#[derive(Clone)]
struct FileTransferContext {
    app: AppHandle,
    local: LocalDevice,
    identity: NoiseIdentity,
    trust: TrustStore,
    pending_offers: PendingFileOffers,
    active_transfers: ActiveTransfers,
}

#[derive(Clone)]
struct IncomingContext {
    file: FileTransferContext,
    clipboard: ClipboardService,
    pending_pairing: PendingConfirmations,
}

#[derive(Clone)]
struct TransferInfo {
    id: String,
    peer_id: String,
    peer_name: String,
    name: String,
    size: u64,
    direction: String,
    path: Option<PathBuf>,
    cleanup_source: bool,
}

#[derive(Debug)]
struct TransferFailure {
    status: &'static str,
    message: String,
}

impl TransferFailure {
    fn failed(message: impl Into<String>) -> Self {
        Self {
            status: "failed",
            message: message.into(),
        }
    }

    fn cancelled(message: impl Into<String>) -> Self {
        Self {
            status: "cancelled",
            message: message.into(),
        }
    }

    fn rejected(message: impl Into<String>) -> Self {
        Self {
            status: "rejected",
            message: message.into(),
        }
    }
}

enum NetworkCommand {
    UpdateLocalDevice(LocalDevice),
    BeginPairing {
        peer: PeerDevice,
        response: oneshot::Sender<Result<PairingRequest, String>>,
    },
    ConfirmPairing {
        session_id: String,
        accepted: bool,
        response: oneshot::Sender<Result<(), String>>,
    },
    SendTestMessage {
        peer: PeerDevice,
        text: String,
        response: oneshot::Sender<Result<TestMessageEvent, String>>,
    },
    SendFile {
        peer: PeerDevice,
        path: PathBuf,
        name: String,
        size: u64,
        cleanup_source: bool,
        transfer_id: String,
    },
    DecideFileOffer {
        transfer_id: String,
        accepted: bool,
        response: oneshot::Sender<Result<(), String>>,
    },
    CancelTransfer {
        transfer_id: String,
        response: oneshot::Sender<bool>,
    },
    BroadcastClipboard {
        peers: Vec<PeerDevice>,
        event_id: String,
        text: String,
    },
}

#[derive(Clone)]
pub(crate) struct NetworkHandle {
    sender: mpsc::UnboundedSender<NetworkCommand>,
    status: Arc<RwLock<NetworkStatus>>,
    relay: RelayClientHandle,
}

impl NetworkHandle {
    pub(crate) fn unavailable(
        error: String,
        port: u16,
        relay: RelayDirective,
        local: LocalDevice,
    ) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        drop(receiver);
        let relay = RelayClientHandle::unavailable(
            relay,
            local,
            "设备加密身份不可用，中继连接无法启动".into(),
        );
        Self {
            sender,
            status: Arc::new(RwLock::new(NetworkStatus {
                active: false,
                error: Some(error),
                port,
                identity_fingerprint: "不可用".to_string(),
            })),
            relay,
        }
    }

    pub(crate) fn start(startup: NetworkStartup) -> Self {
        let NetworkStartup {
            app,
            local,
            identity,
            trust,
            clipboard,
            port,
            peers,
            relay_directive,
        } = startup;
        let (sender, receiver) = mpsc::unbounded_channel();
        let (relay, relay_worker, relay_incoming) =
            relay_client_channel(relay_directive, local.clone());
        let thread_relay = relay.clone();
        let status = Arc::new(RwLock::new(NetworkStatus {
            active: false,
            error: None,
            port,
            identity_fingerprint: identity.fingerprint(),
        }));
        let thread_status = Arc::clone(&status);
        let context = NetworkContext {
            app,
            local,
            identity,
            trust,
            clipboard,
        };

        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("neloa-network")
                .build();
            match runtime {
                Ok(runtime) => {
                    let network = NetworkRuntime {
                        context,
                        port,
                        commands: receiver,
                        status: Arc::clone(&thread_status),
                        peers,
                        relay: thread_relay,
                        relay_worker,
                        relay_incoming,
                    };
                    if let Err(error) = runtime.block_on(run_network(network)) {
                        let mut current = thread_status.write();
                        current.active = false;
                        current.error = Some(error);
                    }
                }
                Err(error) => {
                    thread_status.write().error = Some(format!("无法启动网络运行时：{error}"));
                }
            }
        });

        Self {
            sender,
            status,
            relay,
        }
    }

    pub(crate) fn status(&self) -> NetworkStatus {
        self.status.read().clone()
    }

    pub(crate) fn relay_status(&self) -> crate::model::RelaySnapshot {
        self.relay.status()
    }

    pub(crate) fn configure_relay(&self, directive: RelayDirective) -> Result<(), String> {
        self.relay.configure(directive)
    }

    pub(crate) fn update_local_device(&self, device: LocalDevice) -> Result<(), String> {
        self.relay.update_device(device.clone())?;
        self.sender
            .send(NetworkCommand::UpdateLocalDevice(device))
            .map_err(|_| "加密网络服务未运行".to_string())
    }

    pub(crate) async fn begin_pairing(&self, peer: PeerDevice) -> Result<PairingRequest, String> {
        let (response, result) = oneshot::channel();
        self.sender
            .send(NetworkCommand::BeginPairing { peer, response })
            .map_err(|_| "加密网络服务未运行".to_string())?;
        result.await.map_err(|_| "配对连接意外中断".to_string())?
    }

    pub(crate) async fn confirm_pairing(
        &self,
        session_id: String,
        accepted: bool,
    ) -> Result<(), String> {
        let (response, result) = oneshot::channel();
        self.sender
            .send(NetworkCommand::ConfirmPairing {
                session_id,
                accepted,
                response,
            })
            .map_err(|_| "加密网络服务未运行".to_string())?;
        result.await.map_err(|_| "无法提交配对确认".to_string())?
    }

    pub(crate) async fn send_test_message(
        &self,
        peer: PeerDevice,
        text: String,
    ) -> Result<TestMessageEvent, String> {
        let (response, result) = oneshot::channel();
        self.sender
            .send(NetworkCommand::SendTestMessage {
                peer,
                text,
                response,
            })
            .map_err(|_| "加密网络服务未运行".to_string())?;
        result
            .await
            .map_err(|_| "加密测试消息连接意外中断".to_string())?
    }

    pub(crate) fn start_file_transfer(
        &self,
        peer: PeerDevice,
        path: PathBuf,
        name: String,
        size: u64,
        cleanup_source: bool,
    ) -> Result<String, String> {
        let transfer_id = Uuid::new_v4().to_string();
        self.sender
            .send(NetworkCommand::SendFile {
                peer,
                path,
                name,
                size,
                cleanup_source,
                transfer_id: transfer_id.clone(),
            })
            .map_err(|_| "加密网络服务未运行".to_string())?;
        Ok(transfer_id)
    }

    pub(crate) async fn decide_file_offer(
        &self,
        transfer_id: String,
        accepted: bool,
    ) -> Result<(), String> {
        let (response, result) = oneshot::channel();
        self.sender
            .send(NetworkCommand::DecideFileOffer {
                transfer_id,
                accepted,
                response,
            })
            .map_err(|_| "加密网络服务未运行".to_string())?;
        result
            .await
            .map_err(|_| "无法提交文件接收决定".to_string())?
    }

    pub(crate) async fn cancel_file_transfer(&self, transfer_id: String) -> Result<bool, String> {
        let (response, result) = oneshot::channel();
        self.sender
            .send(NetworkCommand::CancelTransfer {
                transfer_id,
                response,
            })
            .map_err(|_| "加密网络服务未运行".to_string())?;
        result.await.map_err(|_| "无法取消文件传输".to_string())
    }

    pub(crate) fn broadcast_clipboard(
        &self,
        peers: Vec<PeerDevice>,
        event_id: String,
        text: String,
    ) -> Result<(), String> {
        self.sender
            .send(NetworkCommand::BroadcastClipboard {
                peers,
                event_id,
                text,
            })
            .map_err(|_| "加密网络服务未运行".to_string())
    }
}

async fn run_network(runtime: NetworkRuntime) -> Result<(), String> {
    let NetworkRuntime {
        context,
        port,
        mut commands,
        status,
        peers,
        relay,
        relay_worker,
        mut relay_incoming,
    } = runtime;
    let NetworkContext {
        app,
        mut local,
        identity,
        trust,
        clipboard,
    } = context;
    let mut endpoint = make_endpoint(port)?;
    endpoint.set_default_client_config(insecure_quic_client_config()?);
    let connector = TransportConnector {
        quic: endpoint.clone(),
        relay: relay.clone(),
    };
    tokio::spawn(relay_worker.run(app.clone(), trust.clone(), peers));
    status.write().active = true;
    let pending: PendingConfirmations = Arc::new(Mutex::new(HashMap::new()));
    let pending_offers: PendingFileOffers = Arc::new(Mutex::new(HashMap::new()));
    let active_transfers: ActiveTransfers = Arc::new(Mutex::new(HashMap::new()));

    loop {
        tokio::select! {
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { return Ok(()); };
                let app = app.clone();
                let local = local.clone();
                let identity = identity.clone();
                let trust = trust.clone();
                let clipboard = clipboard.clone();
                let pending = Arc::clone(&pending);
                let pending_offers = Arc::clone(&pending_offers);
                let active_transfers = Arc::clone(&active_transfers);
                tokio::spawn(async move {
                    let result = async {
                        let connection = timeout(CONNECT_TIMEOUT, incoming)
                            .await
                            .map_err(|_| "接收连接超时".to_string())?
                            .map_err(|error| format!("无法接收 QUIC 连接：{error}"))?;
                        let (send, receive) = timeout(CONNECT_TIMEOUT, connection.accept_bi())
                            .await
                            .map_err(|_| "等待加密会话超时".to_string())?
                            .map_err(|error| format!("无法接收 QUIC 数据流：{error}"))?;
                        let context = IncomingContext {
                            file: FileTransferContext {
                                app: app.clone(),
                                local,
                                identity,
                                trust,
                                pending_offers,
                                active_transfers,
                            },
                            clipboard,
                            pending_pairing: pending,
                        };
                        handle_incoming(
                            context,
                            SessionSend::Quic(send),
                            SessionReceive::Quic(receive),
                        )
                        .await
                    }.await;
                    if let Err(error) = result {
                        let _ = app.emit("network-error", error);
                    }
                });
            }
            incoming = relay_incoming.recv() => {
                let Some(IncomingRelayTunnel { source_id, tunnel }) = incoming else {
                    return Err("中继连接服务意外停止".into());
                };
                if trust.find(&source_id).is_none() {
                    continue;
                }
                let app = app.clone();
                let local = local.clone();
                let identity = identity.clone();
                let trust = trust.clone();
                let clipboard = clipboard.clone();
                let pending = Arc::clone(&pending);
                let pending_offers = Arc::clone(&pending_offers);
                let active_transfers = Arc::clone(&active_transfers);
                tokio::spawn(async move {
                    let context = IncomingContext {
                        file: FileTransferContext {
                            app: app.clone(),
                            local,
                            identity,
                            trust,
                            pending_offers,
                            active_transfers,
                        },
                        clipboard,
                        pending_pairing: pending,
                    };
                    if let Err(error) = handle_incoming(
                        context,
                        SessionSend::Relay(tunnel.send),
                        SessionReceive::Relay(tunnel.receive),
                    ).await {
                        let _ = app.emit("network-error", error);
                    }
                });
            }
            command = commands.recv() => {
                let Some(command) = command else { return Ok(()); };
                match command {
                    NetworkCommand::UpdateLocalDevice(device) => {
                        local = device;
                    }
                    NetworkCommand::BeginPairing { peer, response } => {
                        let connector = connector.clone();
                        let context = PairingContext {
                            app: app.clone(),
                            local: local.clone(),
                            identity: identity.clone(),
                            trust: trust.clone(),
                            pending: Arc::clone(&pending),
                        };
                        tokio::spawn(async move {
                            begin_outgoing_pairing(connector, context, peer, response).await;
                        });
                    }
                    NetworkCommand::ConfirmPairing { session_id, accepted, response } => {
                        let result = pending
                            .lock()
                            .remove(&session_id)
                            .ok_or_else(|| "该配对请求已过期".to_string())
                            .and_then(|sender| sender.send(accepted).map_err(|_| "配对会话已关闭".to_string()));
                        let _ = response.send(result);
                    }
                    NetworkCommand::SendTestMessage { peer, text, response } => {
                        let connector = connector.clone();
                        let context = TrustedSendContext {
                            app: app.clone(),
                            local: local.clone(),
                            identity: identity.clone(),
                            trust: trust.clone(),
                        };
                        tokio::spawn(async move {
                            let result = outgoing_test_message(connector, context, peer, text).await;
                            let _ = response.send(result);
                        });
                    }
                    NetworkCommand::SendFile {
                        peer,
                        path,
                        name,
                        size,
                        cleanup_source,
                        transfer_id,
                    } => {
                        let file_connector = connector.clone();
                        let (cancel_sender, cancel) = watch::channel(false);
                        active_transfers.lock().insert(transfer_id.clone(), cancel_sender);
                        let context = FileTransferContext {
                            app: app.clone(),
                            local: local.clone(),
                            identity: identity.clone(),
                            trust: trust.clone(),
                            pending_offers: Arc::clone(&pending_offers),
                            active_transfers: Arc::clone(&active_transfers),
                        };
                        let info = TransferInfo {
                            id: transfer_id.clone(),
                            peer_id: peer.id.clone(),
                            peer_name: peer.name.clone(),
                            name,
                            size,
                            direction: "sent".into(),
                            path: Some(path),
                            cleanup_source,
                        };
                        tokio::spawn(async move {
                            run_outgoing_file_transfer(
                                file_connector,
                                context,
                                peer,
                                info,
                                cancel,
                            ).await;
                        });
                    }
                    NetworkCommand::DecideFileOffer { transfer_id, accepted, response } => {
                        let result = pending_offers
                            .lock()
                            .remove(&transfer_id)
                            .ok_or_else(|| "该文件接收请求已过期".to_string())
                            .and_then(|sender| sender.send(accepted).map_err(|_| "文件接收会话已关闭".to_string()));
                        let _ = response.send(result);
                    }
                    NetworkCommand::CancelTransfer { transfer_id, response } => {
                        let cancelled = active_transfers
                            .lock()
                            .get(&transfer_id)
                            .map(|cancel| {
                                cancel.send_replace(true);
                                true
                            })
                            .unwrap_or(false);
                        let _ = response.send(cancelled);
                    }
                    NetworkCommand::BroadcastClipboard { peers, event_id, text } => {
                        for peer in peers {
                            let connector = connector.clone();
                            let app = app.clone();
                            let context = TrustedSendContext {
                                app: app.clone(),
                                local: local.clone(),
                                identity: identity.clone(),
                                trust: trust.clone(),
                            };
                            let event_id = event_id.clone();
                            let text = text.clone();
                            tokio::spawn(async move {
                                let bytes = text.len();
                                let outcome = outgoing_clipboard_update(
                                    connector,
                                    context,
                                    &peer,
                                    &event_id,
                                    &text,
                                ).await;
                                let (status, message) = match outcome {
                                    Ok(message) => ("synced", message),
                                    Err(message) => ("failed", message),
                                };
                                let _ = app.emit(
                                    "clipboard-sync-event",
                                    ClipboardSyncEvent {
                                        id: event_id,
                                        peer_id: peer.id,
                                        peer_name: peer.name,
                                        direction: "sent".into(),
                                        status: status.into(),
                                        bytes,
                                        message,
                                        at_ms: unix_millis(),
                                    },
                                );
                            });
                        }
                    }
                }
            }
        }
    }
}

async fn begin_outgoing_pairing(
    connector: TransportConnector,
    context: PairingContext,
    peer: PeerDevice,
    response: oneshot::Sender<Result<PairingRequest, String>>,
) {
    let (send, receive) = match connector.connect(&peer).await {
        Ok(streams) => streams,
        Err(error) => {
            let _ = response.send(Err(error));
            return;
        }
    };
    let session =
        match initiator_handshake(send, receive, &context.identity, &context.local, "pair").await {
            Ok(session) => session,
            Err(error) => {
                let _ = response.send(Err(error));
                return;
            }
        };
    if let Err(error) = ensure_expected_peer(&peer.id, &session.peer.id) {
        let _ = response.send(Err(error));
        return;
    }

    let request = pairing_request(&session, "outgoing");
    let (confirmation, decision) = oneshot::channel();
    context
        .pending
        .lock()
        .insert(request.session_id.clone(), confirmation);
    if response.send(Ok(request.clone())).is_err() {
        context.pending.lock().remove(&request.session_id);
        return;
    }

    if let Err(error) = finish_pairing(
        context.app.clone(),
        context.trust,
        Arc::clone(&context.pending),
        session,
        request.clone(),
        decision,
    )
    .await
    {
        context.pending.lock().remove(&request.session_id);
        let _ = context.app.emit(
            "pairing-result",
            PairingResult {
                session_id: request.session_id,
                peer_id: peer.id,
                peer_name: peer.name,
                accepted: false,
                message: error,
            },
        );
    }
}

async fn handle_incoming(
    context: IncomingContext,
    send: SessionSend,
    receive: SessionReceive,
) -> Result<(), String> {
    let session =
        responder_handshake(send, receive, &context.file.identity, &context.file.local).await?;
    match session.peer.purpose.as_str() {
        "pair" => {
            let request = pairing_request(&session, "incoming");
            let (confirmation, decision) = oneshot::channel();
            context
                .pending_pairing
                .lock()
                .insert(request.session_id.clone(), confirmation);
            context
                .file
                .app
                .emit("pairing-request", request.clone())
                .map_err(|error| format!("无法显示配对请求：{error}"))?;
            finish_pairing(
                context.file.app,
                context.file.trust,
                context.pending_pairing,
                session,
                request,
                decision,
            )
            .await
        }
        "trusted-test" => {
            incoming_test_message(context.file.app, context.file.trust, session).await
        }
        "file" => handle_incoming_file_transfer(context.file, session).await,
        "clipboard" => {
            incoming_clipboard_update(
                context.file.app,
                context.file.trust,
                context.clipboard,
                session,
            )
            .await
        }
        _ => Err("对端请求了未知的加密会话类型".to_string()),
    }
}

async fn finish_pairing(
    app: AppHandle,
    trust: TrustStore,
    pending: PendingConfirmations,
    mut session: NoiseSession,
    request: PairingRequest,
    decision: oneshot::Receiver<bool>,
) -> Result<(), String> {
    let local_accepted = match timeout(PAIRING_TIMEOUT, decision).await {
        Ok(Ok(accepted)) => accepted,
        _ => {
            pending.lock().remove(&request.session_id);
            false
        }
    };

    write_encrypted(
        &mut session.send,
        &mut session.transport,
        &WireMessage::PairDecision {
            accepted: local_accepted,
        },
    )
    .await?;

    let remote_accepted = match timeout(
        PAIRING_TIMEOUT,
        read_encrypted(&mut session.receive, &mut session.transport),
    )
    .await
    {
        Ok(Ok(WireMessage::PairDecision { accepted })) => accepted,
        Ok(Ok(_)) => return Err("对端返回了错误的配对响应".to_string()),
        Ok(Err(error)) => return Err(error),
        Err(_) => false,
    };

    let accepted = local_accepted && remote_accepted;
    let message = if accepted {
        let now = unix_millis();
        trust.upsert(TrustedDevice {
            id: session.peer.id.clone(),
            name: session.peer.name.clone(),
            platform: session.peer.platform.clone(),
            public_key: hex::encode(session.remote_static),
            fingerprint: fingerprint(&session.remote_static),
            paired_at_ms: now,
            last_verified_ms: now,
        })?;
        app.emit("trusted-devices-changed", trust.list())
            .map_err(|error| format!("无法刷新可信设备列表：{error}"))?;
        "验证码一致，已建立可信关系".to_string()
    } else if !local_accepted {
        "本机已取消配对".to_string()
    } else {
        "对端未确认配对".to_string()
    };

    app.emit(
        "pairing-result",
        PairingResult {
            session_id: request.session_id,
            peer_id: session.peer.id,
            peer_name: session.peer.name,
            accepted,
            message,
        },
    )
    .map_err(|error| format!("无法更新配对结果：{error}"))?;
    // The trust decision has already been exchanged and persisted. Stream shutdown is
    // best-effort here so a platform-specific QUIC close race cannot create one-sided trust.
    let _ = finish_stream(&mut session.send);
    let _ = timeout(
        Duration::from_secs(1),
        wait_stream_stopped(&mut session.send),
    )
    .await;
    Ok(())
}

async fn outgoing_test_message(
    connector: TransportConnector,
    context: TrustedSendContext,
    peer: PeerDevice,
    text: String,
) -> Result<TestMessageEvent, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("测试消息不能为空".to_string());
    }
    if text.len() > TEXT_LIMIT {
        return Err(format!("测试消息不能超过 {TEXT_LIMIT} 字节"));
    }
    let trusted = context
        .trust
        .find(&peer.id)
        .ok_or_else(|| "请先完成设备配对".to_string())?;
    let expected_key = decode_public_key(&trusted.public_key)?;

    let (send, receive) = connector.connect(&peer).await?;
    let mut session = initiator_handshake(
        send,
        receive,
        &context.identity,
        &context.local,
        "trusted-test",
    )
    .await?;
    ensure_expected_peer(&peer.id, &session.peer.id)?;
    ensure_trusted_key(&expected_key, &session.remote_static)?;

    let id = Uuid::new_v4().to_string();
    write_encrypted(
        &mut session.send,
        &mut session.transport,
        &WireMessage::TestMessage {
            id: id.clone(),
            text: text.clone(),
        },
    )
    .await?;
    match timeout(
        CONNECT_TIMEOUT,
        read_encrypted(&mut session.receive, &mut session.transport),
    )
    .await
    {
        Ok(Ok(WireMessage::TestAck { id: ack })) if ack == id => {}
        Ok(Ok(WireMessage::Error { message })) => {
            invalidate_stale_trust(&context.app, &context.trust, &peer.id, &message)?;
            return Err(message);
        }
        Ok(Ok(_)) => return Err("对端返回了错误的消息确认".to_string()),
        Ok(Err(error)) => return Err(error),
        Err(_) => return Err("等待对端确认超时".to_string()),
    }

    finish_stream(&mut session.send)?;
    let at_ms = unix_millis();
    context.trust.touch(&peer.id, at_ms).await?;
    Ok(TestMessageEvent {
        id,
        peer_id: peer.id,
        peer_name: peer.name,
        text,
        direction: "sent".into(),
        at_ms,
    })
}

async fn incoming_test_message(
    app: AppHandle,
    trust: TrustStore,
    mut session: NoiseSession,
) -> Result<(), String> {
    if !validate_incoming_trust(&app, &trust, &mut session).await? {
        return Ok(());
    }

    let (id, text) = match read_encrypted(&mut session.receive, &mut session.transport).await? {
        WireMessage::TestMessage { id, text } => (id, text),
        _ => return Err("可信会话中收到了错误的消息类型".to_string()),
    };
    if text.len() > TEXT_LIMIT {
        write_encrypted(
            &mut session.send,
            &mut session.transport,
            &WireMessage::Error {
                message: "消息内容过长".to_string(),
            },
        )
        .await?;
        return Err("拒绝了过长的测试消息".to_string());
    }

    write_encrypted(
        &mut session.send,
        &mut session.transport,
        &WireMessage::TestAck { id: id.clone() },
    )
    .await?;
    finish_stream(&mut session.send)?;
    let at_ms = unix_millis();
    trust.touch(&session.peer.id, at_ms).await?;
    app.emit(
        "test-message-received",
        TestMessageEvent {
            id,
            peer_id: session.peer.id,
            peer_name: session.peer.name,
            text,
            direction: "received".into(),
            at_ms,
        },
    )
    .map_err(|error| format!("无法显示收到的测试消息：{error}"))?;
    Ok(())
}

async fn outgoing_clipboard_update(
    connector: TransportConnector,
    context: TrustedSendContext,
    peer: &PeerDevice,
    event_id: &str,
    text: &str,
) -> Result<String, String> {
    Uuid::parse_str(event_id).map_err(|_| "剪贴板事件标识无效".to_string())?;
    validate_clipboard_text(text)?;
    let trusted = context
        .trust
        .find(&peer.id)
        .ok_or_else(|| "请先完成设备配对".to_string())?;
    let expected_key = decode_public_key(&trusted.public_key)?;

    let (send, receive) = connector.connect(peer).await?;
    let mut session = initiator_handshake(
        send,
        receive,
        &context.identity,
        &context.local,
        "clipboard",
    )
    .await?;
    ensure_expected_peer(&peer.id, &session.peer.id)?;
    ensure_trusted_key(&expected_key, &session.remote_static)?;
    write_encrypted(
        &mut session.send,
        &mut session.transport,
        &WireMessage::ClipboardUpdate {
            id: event_id.to_string(),
            text: text.to_string(),
        },
    )
    .await?;

    let message = match timeout(
        CONNECT_TIMEOUT,
        read_encrypted(&mut session.receive, &mut session.transport),
    )
    .await
    {
        Ok(Ok(WireMessage::ClipboardAck {
            id,
            accepted: true,
            message,
        })) if id == event_id => message,
        Ok(Ok(WireMessage::ClipboardAck {
            id,
            accepted: false,
            message,
        })) if id == event_id => return Err(message),
        Ok(Ok(WireMessage::Error { message })) => {
            invalidate_stale_trust(&context.app, &context.trust, &peer.id, &message)?;
            return Err(message);
        }
        Ok(Ok(_)) => return Err("对端返回了错误的剪贴板确认".to_string()),
        Ok(Err(error)) => return Err(error),
        Err(_) => return Err("等待剪贴板同步确认超时".to_string()),
    };
    finish_stream(&mut session.send)?;
    context.trust.touch(&peer.id, unix_millis()).await?;
    Ok(message)
}

async fn incoming_clipboard_update(
    app: AppHandle,
    trust: TrustStore,
    clipboard: ClipboardService,
    mut session: NoiseSession,
) -> Result<(), String> {
    if !validate_incoming_trust(&app, &trust, &mut session).await? {
        return Ok(());
    }

    let (id, text) = match timeout(
        CONNECT_TIMEOUT,
        read_encrypted(&mut session.receive, &mut session.transport),
    )
    .await
    {
        Ok(Ok(WireMessage::ClipboardUpdate { id, text })) => (id, text),
        Ok(Ok(_)) => return Err("剪贴板会话中收到了错误的消息类型".to_string()),
        Ok(Err(error)) => return Err(error),
        Err(_) => return Err("等待剪贴板内容超时".to_string()),
    };
    let bytes = text.len();
    let validation = Uuid::parse_str(&id)
        .map(|_| ())
        .map_err(|_| "剪贴板事件标识无效".to_string())
        .and_then(|_| validate_clipboard_text(&text));
    let outcome = match validation {
        Ok(()) => clipboard.apply_remote(id.clone(), text).await,
        Err(message) => Err(message),
    };
    let (accepted, status, message, applied) = match outcome {
        Ok(true) => (true, "synced", "已写入系统剪贴板".to_string(), true),
        Ok(false) => (true, "synced", "重复事件已安全忽略".to_string(), false),
        Err(message) => (false, "failed", message, false),
    };
    write_encrypted(
        &mut session.send,
        &mut session.transport,
        &WireMessage::ClipboardAck {
            id: id.clone(),
            accepted,
            message: message.clone(),
        },
    )
    .await?;
    finish_stream(&mut session.send)?;
    let _ = timeout(
        Duration::from_secs(1),
        wait_stream_stopped(&mut session.send),
    )
    .await;

    if applied {
        trust.touch(&session.peer.id, unix_millis()).await?;
    }
    if applied || !accepted {
        app.emit(
            "clipboard-sync-event",
            ClipboardSyncEvent {
                id,
                peer_id: session.peer.id,
                peer_name: session.peer.name,
                direction: "received".into(),
                status: status.into(),
                bytes,
                message,
                at_ms: unix_millis(),
            },
        )
        .map_err(|error| format!("无法更新剪贴板同步状态：{error}"))?;
    }
    Ok(())
}

async fn run_outgoing_file_transfer(
    connector: TransportConnector,
    context: FileTransferContext,
    peer: PeerDevice,
    info: TransferInfo,
    cancel: TransferCancellation,
) {
    let transfer_id = info.id.clone();
    let Some(path) = info.path.clone() else {
        emit_transfer_result(
            &context.app,
            &info,
            "failed",
            "待发送文件路径无效",
            None,
            None,
        );
        context.active_transfers.lock().remove(&transfer_id);
        return;
    };
    let outcome = outgoing_file_transfer(connector, &context, &peer, &path, &info, &cancel).await;
    let result_path = (!info.cleanup_source).then(|| path.clone());

    match outcome {
        Ok(sha256) => emit_transfer_result(
            &context.app,
            &info,
            "completed",
            "文件已加密发送并由对端校验",
            result_path.clone(),
            Some(sha256),
        ),
        Err(failure) => emit_transfer_result(
            &context.app,
            &info,
            failure.status,
            &failure.message,
            result_path,
            None,
        ),
    }
    if info.cleanup_source {
        let _ = tokio::fs::remove_file(&path).await;
    }
    context.active_transfers.lock().remove(&transfer_id);
}

async fn outgoing_file_transfer(
    connector: TransportConnector,
    context: &FileTransferContext,
    peer: &PeerDevice,
    path: &Path,
    info: &TransferInfo,
    cancel: &TransferCancellation,
) -> Result<String, TransferFailure> {
    let trusted = context
        .trust
        .find(&peer.id)
        .ok_or_else(|| TransferFailure::failed("请先完成设备配对"))?;
    let expected_key = decode_public_key(&trusted.public_key).map_err(TransferFailure::failed)?;
    let sha256 = hash_file(&context.app, info, path, cancel).await?;
    ensure_not_cancelled(cancel)?;

    let (send, receive) = connector
        .connect(peer)
        .await
        .map_err(TransferFailure::failed)?;
    let mut session = initiator_handshake(send, receive, &context.identity, &context.local, "file")
        .await
        .map_err(TransferFailure::failed)?;
    ensure_expected_peer(&peer.id, &session.peer.id).map_err(TransferFailure::failed)?;
    ensure_trusted_key(&expected_key, &session.remote_static).map_err(TransferFailure::failed)?;

    write_encrypted(
        &mut session.send,
        &mut session.transport,
        &WireMessage::FileOffer {
            transfer_id: info.id.clone(),
            name: info.name.clone(),
            size: info.size,
            sha256: sha256.clone(),
        },
    )
    .await
    .map_err(TransferFailure::failed)?;
    emit_transfer_progress(&context.app, info, "waiting", 0, Instant::now());

    let decision = read_control_or_cancel(
        &mut session.receive,
        &mut session.transport,
        FILE_OFFER_TIMEOUT,
        cancel,
        "等待对端确认接收超时",
        "文件发送已取消",
    )
    .await?;
    match decision {
        WireMessage::FileDecision { accepted: true, .. } => {}
        WireMessage::FileDecision {
            accepted: false,
            reason,
        } => {
            return Err(TransferFailure::rejected(
                reason.unwrap_or_else(|| "对端拒绝接收文件".to_string()),
            ));
        }
        WireMessage::Error { message } => {
            invalidate_stale_trust(&context.app, &context.trust, &peer.id, &message)
                .map_err(TransferFailure::failed)?;
            return Err(TransferFailure::failed(message));
        }
        _ => return Err(TransferFailure::failed("对端返回了错误的文件接收响应")),
    }

    let mut file = File::open(path)
        .await
        .map_err(|error| TransferFailure::failed(format!("无法打开待发送文件：{error}")))?;
    let mut buffer = vec![0_u8; FILE_CHUNK_SIZE];
    let mut chunk_buffers = ChunkWriteBuffers::new();
    let mut transferred = 0_u64;
    let mut send_hash = Sha256::new();
    let started = Instant::now();
    let mut last_progress = Instant::now() - PROGRESS_INTERVAL;

    loop {
        if is_cancelled(cancel) {
            let _ = write_encrypted(
                &mut session.send,
                &mut session.transport,
                &WireMessage::FileCancel {
                    message: "发送方已取消".into(),
                },
            )
            .await;
            let _ = finish_stream(&mut session.send);
            return Err(TransferFailure::cancelled("文件发送已取消"));
        }
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| TransferFailure::failed(format!("读取待发送文件失败：{error}")))?;
        if read == 0 {
            break;
        }
        if transferred.saturating_add(read as u64) > info.size {
            return Err(TransferFailure::failed("文件在发送期间发生变化，请重试"));
        }
        send_hash.update(&buffer[..read]);
        write_encrypted_chunk(
            &mut session.send,
            &mut session.transport,
            transferred,
            &buffer[..read],
            &mut chunk_buffers,
        )
        .await
        .map_err(|error| {
            if is_cancelled(cancel) {
                TransferFailure::cancelled("文件发送已取消")
            } else {
                TransferFailure::failed(format!("文件传输中断：{error}"))
            }
        })?;
        transferred += read as u64;
        if should_emit_progress(&mut last_progress, transferred == info.size) {
            emit_transfer_progress(&context.app, info, "transferring", transferred, started);
        }
    }

    if transferred != info.size {
        return Err(TransferFailure::failed("文件在发送期间发生变化，请重试"));
    }
    let sent_sha256 = hex::encode(send_hash.finalize());
    if sent_sha256 != sha256 {
        return Err(TransferFailure::failed(
            "文件在发送期间发生变化，请重新选择",
        ));
    }

    write_encrypted(
        &mut session.send,
        &mut session.transport,
        &WireMessage::FileComplete {
            size: transferred,
            sha256: sent_sha256.clone(),
        },
    )
    .await
    .map_err(TransferFailure::failed)?;
    finish_stream(&mut session.send).map_err(TransferFailure::failed)?;
    let receipt = read_control_or_cancel(
        &mut session.receive,
        &mut session.transport,
        FILE_IDLE_TIMEOUT,
        cancel,
        "等待接收端校验结果超时",
        "文件发送已取消",
    )
    .await?;
    match receipt {
        WireMessage::FileReceipt { accepted: true, .. } => {}
        WireMessage::FileReceipt {
            accepted: false,
            message,
        } => return Err(TransferFailure::failed(message)),
        WireMessage::Error { message } => {
            invalidate_stale_trust(&context.app, &context.trust, &peer.id, &message)
                .map_err(TransferFailure::failed)?;
            return Err(TransferFailure::failed(message));
        }
        _ => return Err(TransferFailure::failed("接收端返回了错误的文件校验结果")),
    }
    context
        .trust
        .touch(&peer.id, unix_millis())
        .await
        .map_err(TransferFailure::failed)?;
    Ok(sent_sha256)
}

async fn handle_incoming_file_transfer(
    context: FileTransferContext,
    mut session: NoiseSession,
) -> Result<(), String> {
    if !validate_incoming_trust(&context.app, &context.trust, &mut session).await? {
        return Ok(());
    }

    let offer = timeout(
        FILE_IDLE_TIMEOUT,
        read_encrypted(&mut session.receive, &mut session.transport),
    )
    .await
    .map_err(|_| "等待文件信息超时".to_string())??;
    let (transfer_id, raw_name, size, sha256) = match offer {
        WireMessage::FileOffer {
            transfer_id,
            name,
            size,
            sha256,
        } => (transfer_id, name, size, sha256),
        _ => return Err("文件会话没有提供有效的文件信息".to_string()),
    };
    Uuid::parse_str(&transfer_id).map_err(|_| "文件传输标识无效".to_string())?;
    let name = validate_file_name(&raw_name)?;
    validate_sha256(&sha256)?;

    let info = TransferInfo {
        id: transfer_id.clone(),
        peer_id: session.peer.id.clone(),
        peer_name: session.peer.name.clone(),
        name: name.clone(),
        size,
        direction: "received".into(),
        path: None,
        cleanup_source: false,
    };
    let (cancel_sender, cancel) = watch::channel(false);
    {
        let mut active = context.active_transfers.lock();
        if active.contains_key(&transfer_id) {
            return Err("检测到重复的文件传输标识".to_string());
        }
        active.insert(transfer_id.clone(), cancel_sender);
    }

    let result = async {
        let (confirmation, decision) = oneshot::channel();
        context
            .pending_offers
            .lock()
            .insert(transfer_id.clone(), confirmation);
        context
            .app
            .emit(
                "file-offer",
                FileOfferEvent {
                    transfer_id: transfer_id.clone(),
                    peer_id: info.peer_id.clone(),
                    peer_name: info.peer_name.clone(),
                    name: name.clone(),
                    size,
                    sha256: sha256.clone(),
                },
            )
            .map_err(|error| format!("无法显示文件接收请求：{error}"))?;

        let accepted = match timeout(FILE_OFFER_TIMEOUT, decision).await {
            Ok(Ok(accepted)) => accepted,
            _ => {
                context.pending_offers.lock().remove(&transfer_id);
                false
            }
        };
        write_encrypted(
            &mut session.send,
            &mut session.transport,
            &WireMessage::FileDecision {
                accepted,
                reason: (!accepted).then(|| "接收端已拒绝或请求超时".to_string()),
            },
        )
        .await?;

        if !accepted {
            let _ = finish_stream(&mut session.send);
            emit_transfer_result(
                &context.app,
                &info,
                "rejected",
                "已拒绝接收文件",
                None,
                Some(sha256),
            );
            return Ok(());
        }

        emit_transfer_progress(&context.app, &info, "transferring", 0, Instant::now());
        let outcome =
            receive_file_payload(&context.app, &info, &sha256, &cancel, &mut session).await;
        match outcome {
            Ok(path) => {
                write_encrypted(
                    &mut session.send,
                    &mut session.transport,
                    &WireMessage::FileReceipt {
                        accepted: true,
                        message: "大小和 SHA-256 校验通过".into(),
                    },
                )
                .await?;
                finish_stream(&mut session.send)?;
                let _ = timeout(
                    Duration::from_secs(1),
                    wait_stream_stopped(&mut session.send),
                )
                .await;
                context.trust.touch(&info.peer_id, unix_millis()).await?;
                emit_transfer_result(
                    &context.app,
                    &info,
                    "completed",
                    "文件已校验并保存",
                    Some(path),
                    Some(sha256),
                );
            }
            Err(failure) => {
                if failure.status == "failed" {
                    let _ = write_encrypted(
                        &mut session.send,
                        &mut session.transport,
                        &WireMessage::FileReceipt {
                            accepted: false,
                            message: failure.message.clone(),
                        },
                    )
                    .await;
                    let _ = finish_stream(&mut session.send);
                }
                emit_transfer_result(
                    &context.app,
                    &info,
                    failure.status,
                    &failure.message,
                    None,
                    Some(sha256),
                );
            }
        }
        Ok(())
    }
    .await;
    context.pending_offers.lock().remove(&transfer_id);
    context.active_transfers.lock().remove(&transfer_id);
    result
}

async fn receive_file_payload(
    app: &AppHandle,
    info: &TransferInfo,
    expected_sha256: &str,
    cancel: &TransferCancellation,
    session: &mut NoiseSession,
) -> Result<PathBuf, TransferFailure> {
    #[cfg(target_os = "ios")]
    let download_dir = app
        .path()
        .document_dir()
        .map_err(|error| TransferFailure::failed(format!("无法定位应用文稿目录：{error}")))?
        .join("Neloa");
    #[cfg(not(target_os = "ios"))]
    let download_dir = app
        .path()
        .download_dir()
        .map_err(|error| TransferFailure::failed(format!("无法定位系统下载目录：{error}")))?
        .join("Neloa");
    tokio::fs::create_dir_all(&download_dir)
        .await
        .map_err(|error| TransferFailure::failed(format!("无法创建接收目录：{error}")))?;
    let temporary_path = download_dir.join(format!(".neloa-{}.part", info.id));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary_path)
        .await
        .map_err(|error| TransferFailure::failed(format!("无法创建临时接收文件：{error}")))?;

    let receive_result = async {
        let mut transferred = 0_u64;
        let mut hasher = Sha256::new();
        let mut record_buffers = SecureRecordBuffers::new();
        let started = Instant::now();
        let mut last_progress = Instant::now() - PROGRESS_INTERVAL;
        loop {
            if is_cancelled(cancel) {
                let _ = stop_receive(&mut session.receive);
                let _ = reset_send(&mut session.send);
                return Err(TransferFailure::cancelled("文件接收已取消"));
            }
            let record = read_record_or_cancel(
                &mut session.receive,
                &mut session.transport,
                &mut record_buffers,
                FILE_IDLE_TIMEOUT,
                cancel,
                "文件传输长时间没有数据",
                "文件接收已取消",
            )
            .await?;
            match record {
                BorrowedSecureRecord::FileChunk { offset, bytes } => {
                    if offset != transferred {
                        return Err(TransferFailure::failed("文件分块顺序或偏移无效"));
                    }
                    if bytes.is_empty()
                        || transferred.saturating_add(bytes.len() as u64) > info.size
                    {
                        return Err(TransferFailure::failed("文件分块大小无效"));
                    }
                    file.write_all(bytes).await.map_err(|error| {
                        TransferFailure::failed(format!("写入临时文件失败：{error}"))
                    })?;
                    hasher.update(bytes);
                    transferred += bytes.len() as u64;
                    if should_emit_progress(&mut last_progress, transferred == info.size) {
                        emit_transfer_progress(app, info, "transferring", transferred, started);
                    }
                }
                BorrowedSecureRecord::Control(WireMessage::FileComplete { size, sha256 }) => {
                    if size != info.size || transferred != info.size {
                        return Err(TransferFailure::failed("接收文件大小与发送信息不一致"));
                    }
                    let actual_sha256 = hex::encode(hasher.finalize());
                    if sha256 != expected_sha256 || actual_sha256 != expected_sha256 {
                        return Err(TransferFailure::failed("SHA-256 校验失败，文件已丢弃"));
                    }
                    emit_transfer_progress(app, info, "verifying", transferred, started);
                    file.flush().await.map_err(|error| {
                        TransferFailure::failed(format!("刷新接收文件失败：{error}"))
                    })?;
                    file.sync_all().await.map_err(|error| {
                        TransferFailure::failed(format!("同步接收文件失败：{error}"))
                    })?;
                    drop(file);
                    return publish_without_overwrite(&temporary_path, &download_dir, &info.name)
                        .await;
                }
                BorrowedSecureRecord::Control(WireMessage::FileCancel { message }) => {
                    return Err(TransferFailure::cancelled(message));
                }
                _ => return Err(TransferFailure::failed("文件会话收到了错误的消息类型")),
            }
        }
    }
    .await;

    if receive_result.is_err() {
        let _ = tokio::fs::remove_file(&temporary_path).await;
    }
    receive_result
}

async fn hash_file(
    app: &AppHandle,
    info: &TransferInfo,
    path: &Path,
    cancel: &TransferCancellation,
) -> Result<String, TransferFailure> {
    let mut file = File::open(path)
        .await
        .map_err(|error| TransferFailure::failed(format!("无法打开待发送文件：{error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut processed = 0_u64;
    let started = Instant::now();
    let mut last_progress = Instant::now() - PROGRESS_INTERVAL;
    loop {
        ensure_not_cancelled(cancel)?;
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| TransferFailure::failed(format!("读取待发送文件失败：{error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        processed += read as u64;
        if processed > info.size {
            return Err(TransferFailure::failed("文件大小已变化，请重新选择"));
        }
        if should_emit_progress(&mut last_progress, processed == info.size) {
            emit_transfer_progress(app, info, "hashing", processed, started);
        }
    }
    if processed != info.size {
        return Err(TransferFailure::failed("文件大小已变化，请重新选择"));
    }
    if info.size == 0 {
        emit_transfer_progress(app, info, "hashing", 0, started);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn publish_without_overwrite(
    temporary_path: &Path,
    directory: &Path,
    file_name: &str,
) -> Result<PathBuf, TransferFailure> {
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());

    for index in 0..10_000_u32 {
        let candidate_name = if index == 0 {
            file_name.to_string()
        } else if let Some(extension) = extension {
            format!("{stem} ({index}).{extension}")
        } else {
            format!("{stem} ({index})")
        };
        let candidate = directory.join(candidate_name);
        match tokio::fs::hard_link(temporary_path, &candidate).await {
            Ok(()) => {
                tokio::fs::remove_file(temporary_path)
                    .await
                    .map_err(|error| {
                        TransferFailure::failed(format!("清理临时文件失败：{error}"))
                    })?;
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(TransferFailure::failed(format!(
                    "无法原子保存接收文件：{error}"
                )));
            }
        }
    }
    Err(TransferFailure::failed("接收目录中存在过多同名文件"))
}

fn validate_file_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    let base_name = trimmed
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let windows_reserved = matches!(base_name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base_name.len() == 4
            && (base_name.starts_with("COM") || base_name.starts_with("LPT"))
            && matches!(base_name.as_bytes()[3], b'1'..=b'9'));
    let invalid_character = trimmed.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
    });
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.len() > 240
        || trimmed.ends_with(['.', ' '])
        || windows_reserved
        || invalid_character
        || Path::new(trimmed)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(trimmed)
    {
        return Err("对端提供了不安全或不兼容的文件名".to_string());
    }
    Ok(trimmed.to_string())
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("文件 SHA-256 格式无效".to_string())
    }
}

fn is_cancelled(cancel: &TransferCancellation) -> bool {
    *cancel.borrow()
}

fn ensure_not_cancelled(cancel: &TransferCancellation) -> Result<(), TransferFailure> {
    if is_cancelled(cancel) {
        Err(TransferFailure::cancelled("文件发送已取消"))
    } else {
        Ok(())
    }
}

async fn wait_for_cancel(mut cancel: TransferCancellation) {
    if *cancel.borrow_and_update() {
        return;
    }
    while cancel.changed().await.is_ok() {
        if *cancel.borrow_and_update() {
            return;
        }
    }
}

async fn read_control_or_cancel(
    receive: &mut SessionReceive,
    transport: &mut TransportState,
    duration: Duration,
    cancel: &TransferCancellation,
    timeout_message: &str,
    cancel_message: &str,
) -> Result<WireMessage, TransferFailure> {
    tokio::select! {
        result = timeout(duration, read_encrypted(receive, transport)) => {
            result
                .map_err(|_| TransferFailure::failed(timeout_message))?
                .map_err(TransferFailure::failed)
        }
        _ = wait_for_cancel(cancel.clone()) => Err(TransferFailure::cancelled(cancel_message)),
    }
}

async fn read_record_or_cancel<'a>(
    receive: &mut SessionReceive,
    transport: &mut TransportState,
    buffers: &'a mut SecureRecordBuffers,
    duration: Duration,
    cancel: &TransferCancellation,
    timeout_message: &str,
    cancel_message: &str,
) -> Result<BorrowedSecureRecord<'a>, TransferFailure> {
    tokio::select! {
        result = timeout(duration, read_secure_record_buffered(receive, transport, buffers)) => {
            result
                .map_err(|_| TransferFailure::failed(timeout_message))?
                .map_err(TransferFailure::failed)
        }
        _ = wait_for_cancel(cancel.clone()) => Err(TransferFailure::cancelled(cancel_message)),
    }
}

fn should_emit_progress(last: &mut Instant, complete: bool) -> bool {
    if complete || last.elapsed() >= PROGRESS_INTERVAL {
        *last = Instant::now();
        true
    } else {
        false
    }
}

fn emit_transfer_progress(
    app: &AppHandle,
    info: &TransferInfo,
    stage: &str,
    transferred: u64,
    started: Instant,
) {
    let elapsed = started.elapsed().as_secs_f64();
    let bytes_per_second = if elapsed > 0.0 {
        (transferred as f64 / elapsed) as u64
    } else {
        0
    };
    let _ = app.emit(
        "file-transfer-progress",
        FileTransferProgress {
            transfer_id: info.id.clone(),
            peer_id: info.peer_id.clone(),
            peer_name: info.peer_name.clone(),
            name: info.name.clone(),
            direction: info.direction.clone(),
            stage: stage.to_string(),
            transferred,
            size: info.size,
            bytes_per_second,
        },
    );
}

fn emit_transfer_result(
    app: &AppHandle,
    info: &TransferInfo,
    status: &str,
    message: &str,
    path: Option<PathBuf>,
    sha256: Option<String>,
) {
    let _ = app.emit(
        "file-transfer-result",
        FileTransferResult {
            transfer_id: info.id.clone(),
            peer_id: info.peer_id.clone(),
            peer_name: info.peer_name.clone(),
            name: info.name.clone(),
            direction: info.direction.clone(),
            status: status.to_string(),
            message: message.to_string(),
            path: path.map(|value| value.to_string_lossy().into_owned()),
            sha256,
            size: info.size,
            at_ms: unix_millis(),
        },
    );
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HandshakeMetadata {
    id: String,
    name: String,
    platform: String,
    version: String,
    #[serde(default)]
    protocol_version: u16,
    #[serde(default)]
    min_protocol_version: u16,
    #[serde(default)]
    capabilities: Vec<String>,
    purpose: String,
}

struct NoiseSession {
    send: SessionSend,
    receive: SessionReceive,
    transport: TransportState,
    peer: HandshakeMetadata,
    remote_static: [u8; 32],
    handshake_hash: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WireMessage {
    PairDecision {
        accepted: bool,
    },
    TestMessage {
        id: String,
        text: String,
    },
    TestAck {
        id: String,
    },
    ClipboardUpdate {
        id: String,
        text: String,
    },
    ClipboardAck {
        id: String,
        accepted: bool,
        message: String,
    },
    FileOffer {
        transfer_id: String,
        name: String,
        size: u64,
        sha256: String,
    },
    FileDecision {
        accepted: bool,
        reason: Option<String>,
    },
    FileComplete {
        size: u64,
        sha256: String,
    },
    FileReceipt {
        accepted: bool,
        message: String,
    },
    FileCancel {
        message: String,
    },
    Error {
        message: String,
    },
}

enum SecureRecord {
    Control(WireMessage),
    FileChunk { offset: u64, bytes: Vec<u8> },
}

enum BorrowedSecureRecord<'a> {
    Control(WireMessage),
    FileChunk { offset: u64, bytes: &'a [u8] },
}

struct ChunkWriteBuffers {
    payload: Vec<u8>,
    encrypted: Vec<u8>,
}

impl ChunkWriteBuffers {
    fn new() -> Self {
        Self {
            payload: Vec::with_capacity(9 + FILE_CHUNK_SIZE),
            encrypted: vec![0_u8; 9 + FILE_CHUNK_SIZE + 64],
        }
    }
}

struct SecureRecordBuffers {
    encrypted: Vec<u8>,
    payload: Vec<u8>,
}

impl SecureRecordBuffers {
    fn new() -> Self {
        Self {
            encrypted: Vec::with_capacity(FILE_CHUNK_SIZE + 9 + 64),
            payload: vec![0_u8; FILE_CHUNK_SIZE + 9 + 64],
        }
    }
}

async fn initiator_handshake(
    mut send: SessionSend,
    mut receive: SessionReceive,
    identity: &NoiseIdentity,
    local: &LocalDevice,
    purpose: &str,
) -> Result<NoiseSession, String> {
    let params = noise_params()?;
    let mut noise = Builder::new(params)
        .local_private_key(identity.secret())
        .map_err(|error| format!("无法载入本机 Noise 密钥：{error}"))?
        .build_initiator()
        .map_err(|error| format!("无法初始化 Noise 发起方：{error}"))?;
    let mut output = vec![0_u8; FRAME_LIMIT];

    let size = noise
        .write_message(&[], &mut output)
        .map_err(|error| format!("无法生成 Noise 握手消息 1：{error}"))?;
    write_frame(&mut send, &output[..size]).await?;

    let message = read_frame(&mut receive).await?;
    let size = noise
        .read_message(&message, &mut output)
        .map_err(|error| format!("无法验证 Noise 握手消息 2：{error}"))?;
    let peer = serde_json::from_slice(&output[..size])
        .map_err(|error| format!("对端握手信息无效：{error}"))?;

    let metadata = handshake_metadata(local, purpose);
    let payload =
        serde_json::to_vec(&metadata).map_err(|error| format!("无法编码本机握手信息：{error}"))?;
    let size = noise
        .write_message(&payload, &mut output)
        .map_err(|error| format!("无法生成 Noise 握手消息 3：{error}"))?;
    write_frame(&mut send, &output[..size]).await?;

    let session = finish_handshake(send, receive, noise, peer)?;
    ensure_peer_capability(&session.peer, purpose)?;
    Ok(session)
}

async fn responder_handshake(
    mut send: SessionSend,
    mut receive: SessionReceive,
    identity: &NoiseIdentity,
    local: &LocalDevice,
) -> Result<NoiseSession, String> {
    let params = noise_params()?;
    let mut noise = Builder::new(params)
        .local_private_key(identity.secret())
        .map_err(|error| format!("无法载入本机 Noise 密钥：{error}"))?
        .build_responder()
        .map_err(|error| format!("无法初始化 Noise 响应方：{error}"))?;
    let mut output = vec![0_u8; FRAME_LIMIT];

    let message = read_frame(&mut receive).await?;
    noise
        .read_message(&message, &mut output)
        .map_err(|error| format!("无法验证 Noise 握手消息 1：{error}"))?;

    let metadata = handshake_metadata(local, "response");
    let payload =
        serde_json::to_vec(&metadata).map_err(|error| format!("无法编码本机握手信息：{error}"))?;
    let size = noise
        .write_message(&payload, &mut output)
        .map_err(|error| format!("无法生成 Noise 握手消息 2：{error}"))?;
    write_frame(&mut send, &output[..size]).await?;

    let message = read_frame(&mut receive).await?;
    let size = noise
        .read_message(&message, &mut output)
        .map_err(|error| format!("无法验证 Noise 握手消息 3：{error}"))?;
    let peer = serde_json::from_slice(&output[..size])
        .map_err(|error| format!("对端握手信息无效：{error}"))?;

    let session = finish_handshake(send, receive, noise, peer)?;
    ensure_peer_capability(&session.peer, &session.peer.purpose)?;
    Ok(session)
}

fn finish_handshake(
    send: SessionSend,
    receive: SessionReceive,
    noise: snow::HandshakeState,
    peer: HandshakeMetadata,
) -> Result<NoiseSession, String> {
    ensure_protocol_compatible(&peer)?;
    let remote_static: [u8; 32] = noise
        .get_remote_static()
        .ok_or_else(|| "Noise 握手未提供对端静态公钥".to_string())?
        .try_into()
        .map_err(|_| "Noise 对端静态公钥长度无效".to_string())?;
    let handshake_hash = noise.get_handshake_hash().to_vec();
    let transport = noise
        .into_transport_mode()
        .map_err(|error| format!("无法进入 Noise 传输模式：{error}"))?;
    Ok(NoiseSession {
        send,
        receive,
        transport,
        peer,
        remote_static,
        handshake_hash,
    })
}

fn handshake_metadata(local: &LocalDevice, purpose: &str) -> HandshakeMetadata {
    HandshakeMetadata {
        id: local.id.clone(),
        name: local.name.clone(),
        platform: local.platform.clone(),
        version: local.version.clone(),
        protocol_version: PROTOCOL_VERSION,
        min_protocol_version: MIN_PROTOCOL_VERSION,
        capabilities: CAPABILITIES
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        purpose: purpose.to_string(),
    }
}

fn ensure_protocol_compatible(peer: &HandshakeMetadata) -> Result<(), String> {
    if crate::model::protocol_compatible(peer.protocol_version, peer.min_protocol_version) {
        Ok(())
    } else {
        Err(format!(
            "协议版本不兼容：本机支持 v{MIN_PROTOCOL_VERSION}–v{PROTOCOL_VERSION}，对端支持 v{}–v{}。请将两端升级到兼容版本。",
            peer.min_protocol_version, peer.protocol_version
        ))
    }
}

fn ensure_peer_capability(peer: &HandshakeMetadata, purpose: &str) -> Result<(), String> {
    let required = match purpose {
        "pair" => Some("pairing"),
        "trusted-test" => Some("test-message"),
        "file" => Some("file-transfer"),
        "clipboard" => Some("clipboard-text"),
        "response" => None,
        _ => return Err(format!("对端请求了未知的加密会话类型：{purpose}")),
    };
    if !peer.capabilities.iter().any(|value| value == "noise-xx") {
        return Err("对端未声明 Noise XX 加密能力".into());
    }
    if let Some(required) = required {
        if !peer.capabilities.iter().any(|value| value == required) {
            return Err(format!("对端不支持当前功能：{required}"));
        }
    }
    Ok(())
}

fn pairing_request(session: &NoiseSession, direction: &str) -> PairingRequest {
    PairingRequest {
        session_id: hex::encode(&session.handshake_hash[..16]),
        peer: PairingPeer {
            id: session.peer.id.clone(),
            name: session.peer.name.clone(),
            platform: session.peer.platform.clone(),
            fingerprint: fingerprint(&session.remote_static),
        },
        code: short_authentication_string(&session.handshake_hash),
        direction: direction.to_string(),
    }
}

fn short_authentication_string(hash: &[u8]) -> String {
    let bytes: [u8; 4] = hash
        .get(..4)
        .unwrap_or(&[0; 4])
        .try_into()
        .unwrap_or_default();
    format!("{:06}", u32::from_be_bytes(bytes) % 1_000_000)
}

fn noise_params() -> Result<NoiseParams, String> {
    NOISE_PATTERN
        .parse()
        .map_err(|error| format!("Noise 协议参数无效：{error}"))
}

async fn write_encrypted(
    send: &mut SessionSend,
    transport: &mut TransportState,
    message: &WireMessage,
) -> Result<(), String> {
    let payload =
        serde_json::to_vec(message).map_err(|error| format!("无法编码加密消息：{error}"))?;
    let mut encrypted = vec![0_u8; payload.len() + 64];
    let size = transport
        .write_message(&payload, &mut encrypted)
        .map_err(|error| format!("无法加密消息：{error}"))?;
    write_frame(send, &encrypted[..size]).await
}

async fn read_encrypted(
    receive: &mut SessionReceive,
    transport: &mut TransportState,
) -> Result<WireMessage, String> {
    match read_secure_record(receive, transport).await? {
        SecureRecord::Control(message) => Ok(message),
        SecureRecord::FileChunk { offset, bytes } => Err(format!(
            "控制会话中收到了文件分块（偏移 {offset}，{} 字节）",
            bytes.len()
        )),
    }
}

async fn write_encrypted_chunk(
    send: &mut SessionSend,
    transport: &mut TransportState,
    offset: u64,
    bytes: &[u8],
    buffers: &mut ChunkWriteBuffers,
) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > FILE_CHUNK_SIZE {
        return Err("文件分块大小无效".to_string());
    }
    buffers.payload.clear();
    buffers.payload.push(0);
    buffers.payload.extend_from_slice(&offset.to_be_bytes());
    buffers.payload.extend_from_slice(bytes);
    let size = transport
        .write_message(&buffers.payload, &mut buffers.encrypted)
        .map_err(|error| format!("无法加密文件分块：{error}"))?;
    write_frame(send, &buffers.encrypted[..size]).await
}

async fn read_secure_record(
    receive: &mut SessionReceive,
    transport: &mut TransportState,
) -> Result<SecureRecord, String> {
    let encrypted = read_frame(receive).await?;
    let mut payload = vec![0_u8; encrypted.len()];
    let size = transport
        .read_message(&encrypted, &mut payload)
        .map_err(|error| format!("无法解密或验证消息：{error}"))?;
    match decode_secure_payload(&payload[..size])? {
        BorrowedSecureRecord::Control(message) => Ok(SecureRecord::Control(message)),
        BorrowedSecureRecord::FileChunk { offset, bytes } => Ok(SecureRecord::FileChunk {
            offset,
            bytes: bytes.to_vec(),
        }),
    }
}

async fn read_secure_record_buffered<'a>(
    receive: &mut SessionReceive,
    transport: &mut TransportState,
    buffers: &'a mut SecureRecordBuffers,
) -> Result<BorrowedSecureRecord<'a>, String> {
    read_frame_into(receive, &mut buffers.encrypted).await?;
    if buffers.payload.len() < buffers.encrypted.len() {
        buffers.payload.resize(buffers.encrypted.len(), 0);
    }
    let size = transport
        .read_message(&buffers.encrypted, &mut buffers.payload)
        .map_err(|error| format!("无法解密或验证消息：{error}"))?;
    decode_secure_payload(&buffers.payload[..size])
}

fn decode_secure_payload(payload: &[u8]) -> Result<BorrowedSecureRecord<'_>, String> {
    if payload.first() == Some(&0) {
        if payload.len() < 9 {
            return Err("加密文件分块头部不完整".to_string());
        }
        let offset = u64::from_be_bytes(
            payload[1..9]
                .try_into()
                .map_err(|_| "文件分块偏移无效".to_string())?,
        );
        Ok(BorrowedSecureRecord::FileChunk {
            offset,
            bytes: &payload[9..],
        })
    } else {
        serde_json::from_slice(payload)
            .map(BorrowedSecureRecord::Control)
            .map_err(|error| format!("解密后的消息格式无效：{error}"))
    }
}

async fn write_frame(send: &mut SessionSend, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > FRAME_LIMIT {
        return Err("协议消息超过大小限制".to_string());
    }
    send.write_u32(bytes.len() as u32)
        .await
        .map_err(|error| format!("无法写入消息长度：{error}"))?;
    send.write_all(bytes)
        .await
        .map_err(|error| format!("无法写入消息：{error}"))?;
    Ok(())
}

async fn read_frame(receive: &mut SessionReceive) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    read_frame_into(receive, &mut bytes).await?;
    Ok(bytes)
}

async fn read_frame_into(receive: &mut SessionReceive, bytes: &mut Vec<u8>) -> Result<(), String> {
    let size = receive
        .read_u32()
        .await
        .map_err(|error| transport_read_error("读取消息", error))? as usize;
    if size > FRAME_LIMIT {
        return Err("拒绝超过大小限制的协议消息".to_string());
    }
    bytes.resize(size, 0);
    receive
        .read_exact(bytes)
        .await
        .map_err(|error| transport_read_error("读取完整消息", error))?;
    Ok(())
}

fn transport_read_error(action: &str, error: impl std::fmt::Display) -> String {
    let detail = error.to_string();
    if detail.contains("connection lost") || detail.contains("closed") || detail.contains("reset") {
        "连接意外中断，请确认两端仍在线并重试".to_string()
    } else {
        format!("无法{action}：{detail}")
    }
}

fn finish_stream(send: &mut SessionSend) -> Result<(), String> {
    match send {
        SessionSend::Quic(stream) => stream
            .finish()
            .map_err(|error| format!("无法完成加密数据流：{error}")),
        SessionSend::Relay(_) => Ok(()),
    }
}

async fn wait_stream_stopped(send: &mut SessionSend) -> Result<(), String> {
    match send {
        SessionSend::Quic(stream) => stream
            .stopped()
            .await
            .map(|_| ())
            .map_err(|error| format!("等待对端完成数据流失败：{error}")),
        SessionSend::Relay(_) => Ok(()),
    }
}

fn stop_receive(receive: &mut SessionReceive) -> Result<(), String> {
    match receive {
        SessionReceive::Quic(stream) => stream
            .stop(0_u8.into())
            .map_err(|error| format!("无法停止接收数据流：{error}")),
        SessionReceive::Relay(_) => Ok(()),
    }
}

fn reset_send(send: &mut SessionSend) -> Result<(), String> {
    match send {
        SessionSend::Quic(stream) => stream
            .reset(0_u8.into())
            .map_err(|error| format!("无法重置发送数据流：{error}")),
        SessionSend::Relay(_) => Ok(()),
    }
}

async fn connect_quic(
    endpoint: &Endpoint,
    peer: &PeerDevice,
) -> Result<(SendStream, RecvStream), String> {
    let ip = peer
        .addresses
        .iter()
        .find_map(|address| address.parse::<IpAddr>().ok())
        .ok_or_else(|| "设备没有可用的局域网 IP 地址".to_string())?;
    let address = SocketAddr::new(ip, peer.port);
    let connecting = endpoint
        .connect(address, "neloa.local")
        .map_err(|error| format!("无法创建 QUIC 连接：{error}"))?;
    let connection = timeout(CONNECT_TIMEOUT, connecting)
        .await
        .map_err(|_| "连接设备超时".to_string())?
        .map_err(|error| format!("无法连接设备：{error}"))?;
    timeout(CONNECT_TIMEOUT, connection.open_bi())
        .await
        .map_err(|_| "打开加密数据流超时".to_string())?
        .map_err(|error| format!("无法打开加密数据流：{error}"))
}

fn make_endpoint(port: u16) -> Result<Endpoint, String> {
    let certified = rcgen::generate_simple_self_signed(vec!["neloa.local".into()])
        .map_err(|error| format!("无法生成 QUIC 临时证书：{error}"))?;
    let certificate = CertificateDer::from(certified.cert);
    let private_key = PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let mut server = quinn::ServerConfig::with_single_cert(vec![certificate], private_key.into())
        .map_err(|error| format!("无法配置 QUIC 服务：{error}"))?;
    let transport =
        Arc::get_mut(&mut server.transport).ok_or_else(|| "无法配置 QUIC 传输参数".to_string())?;
    transport.max_concurrent_bidi_streams(16_u8.into());
    transport.max_concurrent_uni_streams(0_u8.into());
    Endpoint::server(
        server,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port),
    )
    .map_err(|error| format!("无法监听局域网端口 {port}：{error}"))
}

// QUIC's self-signed certificate is deliberately not the device identity. Every application
// payload is protected and authenticated by Noise XX, whose static key is verified against the
// trust store after pairing. Never send plaintext application data over this QUIC connection.
fn insecure_quic_client_config() -> Result<ClientConfig, String> {
    let rustls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();
    let quic = QuicClientConfig::try_from(rustls)
        .map_err(|error| format!("无法配置 QUIC 客户端：{error}"))?;
    Ok(ClientConfig::new(Arc::new(quic)))
}

#[derive(Debug)]
struct SkipServerVerification(Arc<CryptoProvider>);

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
    }
}

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

async fn validate_incoming_trust(
    app: &AppHandle,
    trust: &TrustStore,
    session: &mut NoiseSession,
) -> Result<bool, String> {
    let trusted = trust.find(&session.peer.id);
    let valid = trusted
        .as_ref()
        .and_then(|device| decode_public_key(&device.public_key).ok())
        .is_some_and(|expected| expected == session.remote_static);
    if valid {
        return Ok(true);
    }

    if trust.remove(&session.peer.id)? {
        app.emit("trusted-devices-changed", trust.list())
            .map_err(|error| format!("无法刷新可信设备列表：{error}"))?;
    }
    write_encrypted(
        &mut session.send,
        &mut session.transport,
        &WireMessage::Error {
            message: REPAIR_REQUIRED_MESSAGE.to_string(),
        },
    )
    .await?;
    let _ = finish_stream(&mut session.send);
    let _ = timeout(
        Duration::from_secs(1),
        wait_stream_stopped(&mut session.send),
    )
    .await;
    Ok(false)
}

fn invalidate_stale_trust(
    app: &AppHandle,
    trust: &TrustStore,
    peer_id: &str,
    message: &str,
) -> Result<(), String> {
    if message == REPAIR_REQUIRED_MESSAGE && trust.remove(peer_id)? {
        app.emit("trusted-devices-changed", trust.list())
            .map_err(|error| format!("无法刷新可信设备列表：{error}"))?;
    }
    Ok(())
}

fn decode_public_key(encoded: &str) -> Result<[u8; 32], String> {
    hex::decode(encoded)
        .map_err(|_| "可信设备公钥格式无效".to_string())?
        .try_into()
        .map_err(|_| "可信设备公钥长度无效".to_string())
}

fn ensure_expected_peer(expected: &str, actual: &str) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err("连接设备的身份与局域网广播不一致".to_string())
    }
}

fn ensure_trusted_key(expected: &[u8; 32], actual: &[u8; 32]) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err("设备密钥与配对记录不一致，已拒绝连接".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transfer_cancellation_wakes_without_polling() {
        let (cancel, receiver) = watch::channel(false);
        let waiter = tokio::spawn(wait_for_cancel(receiver));

        cancel.send_replace(true);

        timeout(Duration::from_millis(100), waiter)
            .await
            .expect("cancellation should wake immediately")
            .unwrap();
    }

    #[test]
    fn authentication_code_is_six_digits_and_stable() {
        let hash = [42_u8; 32];
        let first = short_authentication_string(&hash);
        let second = short_authentication_string(&hash);
        assert_eq!(first, second);
        assert_eq!(first.len(), 6);
        assert!(first.chars().all(|character| character.is_ascii_digit()));
    }

    #[test]
    fn handshake_rejects_legacy_and_missing_capabilities() {
        let legacy: HandshakeMetadata = serde_json::from_str(
            r#"{"id":"old","name":"Old","platform":"windows","version":"0.0.9","purpose":"pair"}"#,
        )
        .unwrap();
        assert!(ensure_protocol_compatible(&legacy)
            .unwrap_err()
            .contains("协议版本不兼容"));

        let mut current = handshake_metadata(
            &LocalDevice {
                id: "current".into(),
                name: "Current".into(),
                platform: "macos".into(),
                version: "test".into(),
            },
            "file",
        );
        current
            .capabilities
            .retain(|value| value != "file-transfer");
        assert!(ensure_peer_capability(&current, "file")
            .unwrap_err()
            .contains("file-transfer"));
    }

    #[test]
    fn file_name_validation_is_cross_platform_safe() {
        assert_eq!(
            validate_file_name("report 2026.pdf").unwrap(),
            "report 2026.pdf"
        );
        for unsafe_name in [
            "../secret",
            "folder/file",
            "folder\\file",
            "bad:name",
            "CON.txt",
            "LPT9",
            "..",
            "trailing.",
        ] {
            assert!(validate_file_name(unsafe_name).is_err(), "{unsafe_name}");
        }
    }

    #[tokio::test]
    async fn atomic_publish_never_overwrites_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let existing = directory.path().join("report.txt");
        let temporary = directory.path().join(".transfer.part");
        tokio::fs::write(&existing, b"original").await.unwrap();
        tokio::fs::write(&temporary, b"received").await.unwrap();

        let published = publish_without_overwrite(&temporary, directory.path(), "report.txt")
            .await
            .unwrap();

        assert_eq!(tokio::fs::read(existing).await.unwrap(), b"original");
        assert_eq!(published.file_name().unwrap(), "report (1).txt");
        assert_eq!(tokio::fs::read(published).await.unwrap(), b"received");
        assert!(!temporary.exists());
    }

    #[test]
    fn noise_xx_binds_both_static_keys_and_encrypts() {
        let initiator_identity = NoiseIdentity::generate_for_test();
        let responder_identity = NoiseIdentity::generate_for_test();
        let params = noise_params().unwrap();
        let mut initiator = Builder::new(params.clone())
            .local_private_key(initiator_identity.secret())
            .unwrap()
            .build_initiator()
            .unwrap();
        let mut responder = Builder::new(params)
            .local_private_key(responder_identity.secret())
            .unwrap()
            .build_responder()
            .unwrap();
        let mut message = [0_u8; 1024];
        let mut payload = [0_u8; 1024];

        let size = initiator.write_message(&[], &mut message).unwrap();
        responder
            .read_message(&message[..size], &mut payload)
            .unwrap();
        let size = responder.write_message(b"responder", &mut message).unwrap();
        initiator
            .read_message(&message[..size], &mut payload)
            .unwrap();
        let size = initiator.write_message(b"initiator", &mut message).unwrap();
        responder
            .read_message(&message[..size], &mut payload)
            .unwrap();

        assert_eq!(
            initiator.get_handshake_hash(),
            responder.get_handshake_hash()
        );
        assert_eq!(
            initiator.get_remote_static(),
            Some(responder_identity.public().as_slice())
        );
        assert_eq!(
            responder.get_remote_static(),
            Some(initiator_identity.public().as_slice())
        );

        let mut initiator = initiator.into_transport_mode().unwrap();
        let mut responder = responder.into_transport_mode().unwrap();
        let size = initiator
            .write_message(b"encrypted hello", &mut message)
            .unwrap();
        let size = responder
            .read_message(&message[..size], &mut payload)
            .unwrap();
        assert_eq!(&payload[..size], b"encrypted hello");
    }

    #[tokio::test]
    async fn relay_virtual_stream_carries_noise_encrypted_protocol() {
        let initiator_identity = NoiseIdentity::generate_for_test();
        let responder_identity = NoiseIdentity::generate_for_test();
        let initiator_device = LocalDevice {
            id: "relay-initiator".into(),
            name: "Initiator".into(),
            platform: "ios".into(),
            version: "test".into(),
        };
        let responder_device = LocalDevice {
            id: "relay-responder".into(),
            name: "Responder".into(),
            platform: "macos".into(),
            version: "test".into(),
        };
        let (initiator_stream, responder_stream) = tokio::io::duplex(256 * 1024);
        let (initiator_receive, initiator_send) = tokio::io::split(initiator_stream);
        let (responder_receive, responder_send) = tokio::io::split(responder_stream);
        let expected_initiator_key = initiator_identity.public();
        let expected_responder_key = responder_identity.public();

        let responder = tokio::spawn(async move {
            let mut session = responder_handshake(
                SessionSend::Relay(responder_send),
                SessionReceive::Relay(responder_receive),
                &responder_identity,
                &responder_device,
            )
            .await
            .unwrap();
            assert_eq!(session.remote_static, expected_initiator_key);
            match read_encrypted(&mut session.receive, &mut session.transport)
                .await
                .unwrap()
            {
                WireMessage::TestMessage { id, text } => {
                    assert_eq!(text, "encrypted across relay");
                    write_encrypted(
                        &mut session.send,
                        &mut session.transport,
                        &WireMessage::TestAck { id },
                    )
                    .await
                    .unwrap();
                }
                _ => panic!("unexpected relay test message"),
            }
        });

        let mut session = initiator_handshake(
            SessionSend::Relay(initiator_send),
            SessionReceive::Relay(initiator_receive),
            &initiator_identity,
            &initiator_device,
            "trusted-test",
        )
        .await
        .unwrap();
        assert_eq!(session.remote_static, expected_responder_key);
        let id = Uuid::new_v4().to_string();
        write_encrypted(
            &mut session.send,
            &mut session.transport,
            &WireMessage::TestMessage {
                id: id.clone(),
                text: "encrypted across relay".into(),
            },
        )
        .await
        .unwrap();
        match read_encrypted(&mut session.receive, &mut session.transport)
            .await
            .unwrap()
        {
            WireMessage::TestAck { id: acknowledged } => assert_eq!(acknowledged, id),
            _ => panic!("unexpected relay test acknowledgement"),
        }
        responder.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quic_loopback_carries_noise_encrypted_file_protocol() {
        let server_identity = NoiseIdentity::generate_for_test();
        let client_identity = NoiseIdentity::generate_for_test();
        let server_device = LocalDevice {
            id: "server-id".into(),
            name: "Server".into(),
            platform: "macos".into(),
            version: "test".into(),
        };
        let client_device = LocalDevice {
            id: "client-id".into(),
            name: "Client".into(),
            platform: "windows".into(),
            version: "test".into(),
        };

        let mut server = make_endpoint(0).unwrap();
        server.set_default_client_config(insecure_quic_client_config().unwrap());
        let server_address = server.local_addr().unwrap();
        let expected_client_key = client_identity.public();
        let responder = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            let (send, receive) = connection.accept_bi().await.unwrap();
            let mut session = responder_handshake(
                SessionSend::Quic(send),
                SessionReceive::Quic(receive),
                &server_identity,
                &server_device,
            )
            .await
            .unwrap();
            assert_eq!(session.remote_static, expected_client_key);
            let authentication_code = short_authentication_string(&session.handshake_hash);
            let (transfer_id, expected_size, expected_sha256) =
                match read_encrypted(&mut session.receive, &mut session.transport)
                    .await
                    .unwrap()
                {
                    WireMessage::FileOffer {
                        transfer_id,
                        name,
                        size,
                        sha256,
                    } => {
                        assert_eq!(name, "report.txt");
                        (transfer_id, size, sha256)
                    }
                    _ => panic!("unexpected file offer"),
                };
            write_encrypted(
                &mut session.send,
                &mut session.transport,
                &WireMessage::FileDecision {
                    accepted: true,
                    reason: None,
                },
            )
            .await
            .unwrap();
            let mut received = Vec::new();
            let mut record_buffers = SecureRecordBuffers::new();
            let expected_chunks: &[(u64, &[u8])] = &[(0, b"encrypted "), (10, b"file bytes")];
            for (expected_offset, expected_bytes) in expected_chunks {
                match read_secure_record_buffered(
                    &mut session.receive,
                    &mut session.transport,
                    &mut record_buffers,
                )
                .await
                .unwrap()
                {
                    BorrowedSecureRecord::FileChunk { offset, bytes } => {
                        assert_eq!(offset, *expected_offset);
                        assert_eq!(bytes, *expected_bytes);
                        received.extend_from_slice(bytes);
                    }
                    _ => panic!("unexpected encrypted file record"),
                }
            }
            match read_encrypted(&mut session.receive, &mut session.transport)
                .await
                .unwrap()
            {
                WireMessage::FileComplete { size, sha256 } => {
                    assert_eq!(size, expected_size);
                    assert_eq!(sha256, expected_sha256);
                    assert_eq!(hex::encode(Sha256::digest(&received)), expected_sha256);
                    write_encrypted(
                        &mut session.send,
                        &mut session.transport,
                        &WireMessage::FileReceipt {
                            accepted: true,
                            message: format!("accepted {transfer_id}"),
                        },
                    )
                    .await
                    .unwrap();
                    finish_stream(&mut session.send).unwrap();
                    let _ = timeout(
                        Duration::from_secs(1),
                        wait_stream_stopped(&mut session.send),
                    )
                    .await;
                }
                _ => panic!("unexpected file completion"),
            }
            authentication_code
        });

        let mut client = make_endpoint(0).unwrap();
        client.set_default_client_config(insecure_quic_client_config().unwrap());
        let peer = PeerDevice {
            id: "server-id".into(),
            name: "Server".into(),
            platform: "macos".into(),
            version: "test".into(),
            protocol_version: PROTOCOL_VERSION,
            min_protocol_version: MIN_PROTOCOL_VERSION,
            capabilities: CAPABILITIES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            addresses: vec![Ipv4Addr::LOCALHOST.to_string()],
            port: server_address.port(),
            last_seen_ms: 0,
            relay_available: false,
            service_fullname: String::new(),
        };
        let (send, receive) = connect_quic(&client, &peer).await.unwrap();
        let mut session = initiator_handshake(
            SessionSend::Quic(send),
            SessionReceive::Quic(receive),
            &client_identity,
            &client_device,
            "trusted-test",
        )
        .await
        .unwrap();
        let authentication_code = short_authentication_string(&session.handshake_hash);
        let file_bytes = b"encrypted file bytes";
        let sha256 = hex::encode(Sha256::digest(file_bytes));
        let transfer_id = Uuid::new_v4().to_string();
        write_encrypted(
            &mut session.send,
            &mut session.transport,
            &WireMessage::FileOffer {
                transfer_id: transfer_id.clone(),
                name: "report.txt".into(),
                size: file_bytes.len() as u64,
                sha256: sha256.clone(),
            },
        )
        .await
        .unwrap();
        match read_encrypted(&mut session.receive, &mut session.transport)
            .await
            .unwrap()
        {
            WireMessage::FileDecision { accepted, .. } => assert!(accepted),
            _ => panic!("unexpected file decision"),
        }
        let mut chunk_buffers = ChunkWriteBuffers::new();
        write_encrypted_chunk(
            &mut session.send,
            &mut session.transport,
            0,
            b"encrypted ",
            &mut chunk_buffers,
        )
        .await
        .unwrap();
        write_encrypted_chunk(
            &mut session.send,
            &mut session.transport,
            b"encrypted ".len() as u64,
            b"file bytes",
            &mut chunk_buffers,
        )
        .await
        .unwrap();
        write_encrypted(
            &mut session.send,
            &mut session.transport,
            &WireMessage::FileComplete {
                size: file_bytes.len() as u64,
                sha256,
            },
        )
        .await
        .unwrap();
        finish_stream(&mut session.send).unwrap();
        match read_encrypted(&mut session.receive, &mut session.transport)
            .await
            .unwrap()
        {
            WireMessage::FileReceipt { accepted, message } => {
                assert!(accepted);
                assert_eq!(message, format!("accepted {transfer_id}"));
            }
            _ => panic!("unexpected file receipt"),
        }
        assert_eq!(authentication_code, responder.await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quic_loopback_carries_noise_encrypted_clipboard_protocol() {
        let server_identity = NoiseIdentity::generate_for_test();
        let client_identity = NoiseIdentity::generate_for_test();
        let server_device = LocalDevice {
            id: "clipboard-server".into(),
            name: "Server".into(),
            platform: "macos".into(),
            version: "test".into(),
        };
        let client_device = LocalDevice {
            id: "clipboard-client".into(),
            name: "Client".into(),
            platform: "windows".into(),
            version: "test".into(),
        };

        let mut server = make_endpoint(0).unwrap();
        server.set_default_client_config(insecure_quic_client_config().unwrap());
        let server_address = server.local_addr().unwrap();
        let expected_client_key = client_identity.public();
        let responder = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            let (send, receive) = connection.accept_bi().await.unwrap();
            let mut session = responder_handshake(
                SessionSend::Quic(send),
                SessionReceive::Quic(receive),
                &server_identity,
                &server_device,
            )
            .await
            .unwrap();
            assert_eq!(session.remote_static, expected_client_key);
            assert_eq!(session.peer.purpose, "clipboard");
            let id = match read_encrypted(&mut session.receive, &mut session.transport)
                .await
                .unwrap()
            {
                WireMessage::ClipboardUpdate { id, text } => {
                    assert_eq!(text, "copied across Noise");
                    id
                }
                _ => panic!("unexpected clipboard message"),
            };
            write_encrypted(
                &mut session.send,
                &mut session.transport,
                &WireMessage::ClipboardAck {
                    id,
                    accepted: true,
                    message: "clipboard applied".into(),
                },
            )
            .await
            .unwrap();
            finish_stream(&mut session.send).unwrap();
            let _ = timeout(
                Duration::from_secs(1),
                wait_stream_stopped(&mut session.send),
            )
            .await;
        });

        let mut client = make_endpoint(0).unwrap();
        client.set_default_client_config(insecure_quic_client_config().unwrap());
        let peer = PeerDevice {
            id: "clipboard-server".into(),
            name: "Server".into(),
            platform: "macos".into(),
            version: "test".into(),
            protocol_version: PROTOCOL_VERSION,
            min_protocol_version: MIN_PROTOCOL_VERSION,
            capabilities: CAPABILITIES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            addresses: vec![Ipv4Addr::LOCALHOST.to_string()],
            port: server_address.port(),
            last_seen_ms: 0,
            relay_available: false,
            service_fullname: String::new(),
        };
        let (send, receive) = connect_quic(&client, &peer).await.unwrap();
        let mut session = initiator_handshake(
            SessionSend::Quic(send),
            SessionReceive::Quic(receive),
            &client_identity,
            &client_device,
            "clipboard",
        )
        .await
        .unwrap();
        let id = Uuid::new_v4().to_string();
        write_encrypted(
            &mut session.send,
            &mut session.transport,
            &WireMessage::ClipboardUpdate {
                id: id.clone(),
                text: "copied across Noise".into(),
            },
        )
        .await
        .unwrap();
        match read_encrypted(&mut session.receive, &mut session.transport)
            .await
            .unwrap()
        {
            WireMessage::ClipboardAck {
                id: ack,
                accepted,
                message,
            } => {
                assert_eq!(ack, id);
                assert!(accepted);
                assert_eq!(message, "clipboard applied");
            }
            _ => panic!("unexpected clipboard acknowledgement"),
        }
        finish_stream(&mut session.send).unwrap();
        responder.await.unwrap();
    }
}
