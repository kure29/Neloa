use std::{collections::HashMap, sync::Arc};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::{io, time::Duration};

use parking_lot::RwLock;
use tauri::AppHandle;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use tokio::{
    io::{duplex, split},
    time::timeout,
};
use tokio::{
    io::{DuplexStream, ReadHalf, WriteHalf},
    sync::mpsc,
};

use crate::model::{LocalDevice, PeerDevice};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use crate::packet_stream::{run_packet_stream, PacketFraming};

const PACKET_QUEUE_DEPTH: usize = 256;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const STREAM_BUFFER_SIZE: usize = 1024 * 1024;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(45);
#[cfg(any(target_os = "macos", target_os = "ios"))]
const PACKET_SEND_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) struct BluetoothTunnel {
    pub(crate) send: WriteHalf<DuplexStream>,
    pub(crate) receive: ReadHalf<DuplexStream>,
}

pub(crate) struct IncomingBluetoothLink {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    native: apple::NativeLink,
}

#[derive(Clone)]
pub(crate) struct BluetoothHandle {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    shared: Arc<apple::Shared>,
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    _incoming_guard: Arc<mpsc::Sender<IncomingBluetoothLink>>,
}

impl BluetoothHandle {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub(crate) async fn activate(&self) -> Result<(), String> {
        self.shared.start_cleanup();
        self.shared.ensure_ready().await
    }

    pub(crate) async fn connect(&self, peer: &PeerDevice) -> Result<BluetoothTunnel, String> {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            let native = self.shared.connect(peer.id.clone()).await?;
            Ok(build_tunnel(Arc::clone(&self.shared), native))
        }

        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            let _ = peer;
            Err("蓝牙传输尚未在当前平台启用；Neloa 不会改用其他连接方式".into())
        }
    }

    pub(crate) fn update_local_device(&self, device: LocalDevice) {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        self.shared.update_local_device(device);

        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        let _ = device;
    }

    pub(crate) fn accept(&self, incoming: IncomingBluetoothLink) -> BluetoothTunnel {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            build_tunnel(Arc::clone(&self.shared), incoming.native)
        }

        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            let _ = incoming;
            unreachable!("the unavailable Bluetooth backend cannot produce incoming links")
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn build_tunnel(shared: Arc<apple::Shared>, native: apple::NativeLink) -> BluetoothTunnel {
    let (application, pump) = duplex(STREAM_BUFFER_SIZE);
    let (receive, send) = split(application);
    let session_key = native.session_key.clone();
    let worker_shared = Arc::clone(&shared);
    tokio::spawn(async move {
        let emit_shared = Arc::clone(&worker_shared);
        let emit_key = session_key.clone();
        let result = run_packet_stream(
            pump,
            native.inbound,
            native.maximum_packet_size,
            PacketFraming::Sequenced,
            move |packet| {
                let shared = Arc::clone(&emit_shared);
                let session_key = emit_key.clone();
                async move { shared.send_packet(session_key, packet).await }
            },
        )
        .await;
        worker_shared.close_session(&session_key);
        worker_shared.stop_native_session(session_key);
        if let Err(error) = result {
            let _ = tauri::Emitter::emit(
                &worker_shared.app,
                "network-error",
                format!("蓝牙数据流已关闭：{error}"),
            );
        }
    });
    BluetoothTunnel { send, receive }
}

pub(crate) fn start(
    app: AppHandle,
    local: LocalDevice,
    peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
) -> (BluetoothHandle, mpsc::Receiver<IncomingBluetoothLink>) {
    let (incoming, receiver) = mpsc::channel(PACKET_QUEUE_DEPTH);

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let handle = BluetoothHandle {
        shared: apple::Shared::new(app, local, peers, incoming),
    };

    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let handle = {
        let _ = (app, local, peers);
        BluetoothHandle {
            _incoming_guard: Arc::new(incoming),
        }
    };

    (handle, receiver)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple {
    use std::{
        cell::RefCell,
        collections::{HashMap, VecDeque},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Weak,
        },
        time::Duration,
    };

    use objc2::{
        define_class, msg_send, rc::Retained, runtime::AnyObject, runtime::ProtocolObject,
        AnyThread, DefinedClass, MainThreadOnly, Message,
    };
    use objc2_core_bluetooth::{
        CBATTError, CBATTRequest, CBAdvertisementDataServiceUUIDsKey, CBAttributePermissions,
        CBCentral, CBCentralManager, CBCentralManagerDelegate, CBCharacteristic,
        CBCharacteristicProperties, CBCharacteristicWriteType, CBManagerState,
        CBMutableCharacteristic, CBMutableService, CBPeripheral, CBPeripheralDelegate,
        CBPeripheralManager, CBPeripheralManagerDelegate, CBService, CBUUID,
    };
    use objc2_foundation::{
        MainThreadMarker, NSArray, NSData, NSDictionary, NSError, NSNumber, NSObject,
        NSObjectProtocol, NSString,
    };
    use parking_lot::{Mutex, RwLock};
    use serde::{Deserialize, Serialize};
    use tauri::{AppHandle, Emitter};
    use tokio::{
        sync::{mpsc, oneshot},
        time::{timeout_at, Instant},
    };

    use crate::{
        model::{
            local_capabilities, DiscoverySnapshot, LocalDevice, PeerDevice, CAPABILITY_BLUETOOTH,
            MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
        },
        unix_millis,
    };

    use super::{
        io, timeout, IncomingBluetoothLink, CONNECTION_TIMEOUT, PACKET_QUEUE_DEPTH,
        PACKET_SEND_TIMEOUT,
    };

    const SERVICE_UUID: &str = "95F4A100-7676-4B6D-934A-4E454C4F4100";
    const IDENTITY_UUID: &str = "95F4A101-7676-4B6D-934A-4E454C4F4100";
    const WRITE_UUID: &str = "95F4A102-7676-4B6D-934A-4E454C4F4100";
    const NOTIFY_UUID: &str = "95F4A103-7676-4B6D-934A-4E454C4F4100";
    const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(30);

    type PacketAck = oneshot::Sender<Result<(), String>>;

    pub(super) struct NativeLink {
        pub(super) session_key: String,
        pub(super) maximum_packet_size: usize,
        pub(super) inbound: mpsc::Receiver<Vec<u8>>,
    }

    struct PendingConnect {
        response: oneshot::Sender<Result<NativeLink, String>>,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct BluetoothIdentity {
        id: String,
        name: String,
        platform: String,
        version: String,
        protocol_version: u16,
        min_protocol_version: u16,
        capabilities: Vec<String>,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    enum ManagerState {
        #[default]
        Unknown,
        Resetting,
        Unsupported,
        Unauthorized,
        PoweredOff,
        PoweredOn,
    }

    impl From<CBManagerState> for ManagerState {
        fn from(state: CBManagerState) -> Self {
            match state {
                CBManagerState::Resetting => Self::Resetting,
                CBManagerState::Unsupported => Self::Unsupported,
                CBManagerState::Unauthorized => Self::Unauthorized,
                CBManagerState::PoweredOff => Self::PoweredOff,
                CBManagerState::PoweredOn => Self::PoweredOn,
                _ => Self::Unknown,
            }
        }
    }

    #[derive(Clone, Debug, Default)]
    struct Availability {
        central: ManagerState,
        peripheral: ManagerState,
        startup_error: Option<String>,
    }

    impl Availability {
        fn outcome(&self) -> Option<Result<(), String>> {
            if let Some(error) = &self.startup_error {
                return Some(Err(error.clone()));
            }
            if self.central == ManagerState::Unsupported
                || self.peripheral == ManagerState::Unsupported
            {
                return Some(Err("这台 Apple 设备不支持低功耗蓝牙传输".into()));
            }
            if self.central == ManagerState::Unauthorized
                || self.peripheral == ManagerState::Unauthorized
            {
                return Some(Err(
                    "蓝牙访问被拒绝，请在系统设置的隐私与安全性中允许 Neloa 使用蓝牙".into(),
                ));
            }
            if self.central == ManagerState::PoweredOff
                || self.peripheral == ManagerState::PoweredOff
            {
                return Some(Err("蓝牙已关闭，请先在系统设置中打开蓝牙".into()));
            }
            if self.central == ManagerState::PoweredOn && self.peripheral == ManagerState::PoweredOn
            {
                return Some(Ok(()));
            }
            None
        }
    }

    pub(super) struct Shared {
        pub(super) app: AppHandle,
        local: RwLock<LocalDevice>,
        peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
        incoming: mpsc::Sender<IncomingBluetoothLink>,
        started: AtomicBool,
        scan_scheduled: AtomicBool,
        cleanup_started: AtomicBool,
        bluetooth_seen: Mutex<HashMap<String, u128>>,
        availability: RwLock<Availability>,
        changed: tokio::sync::Notify,
        pending_connect: Mutex<Option<PendingConnect>>,
        sessions: Mutex<HashMap<String, mpsc::Sender<Vec<u8>>>>,
    }

    impl Shared {
        pub(super) fn new(
            app: AppHandle,
            local: LocalDevice,
            peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
            incoming: mpsc::Sender<IncomingBluetoothLink>,
        ) -> Arc<Self> {
            Arc::new(Self {
                app,
                local: RwLock::new(local),
                peers,
                incoming,
                started: AtomicBool::new(false),
                scan_scheduled: AtomicBool::new(false),
                cleanup_started: AtomicBool::new(false),
                bluetooth_seen: Mutex::new(HashMap::new()),
                availability: RwLock::new(Availability::default()),
                changed: tokio::sync::Notify::new(),
                pending_connect: Mutex::new(None),
                sessions: Mutex::new(HashMap::new()),
            })
        }

        pub(super) async fn connect(
            self: &Arc<Self>,
            target_id: String,
        ) -> Result<NativeLink, String> {
            self.ensure_ready().await?;
            let (response, result) = oneshot::channel();
            {
                let mut pending = self.pending_connect.lock();
                if pending.is_some() {
                    return Err("已有蓝牙连接正在建立，请稍后再试".into());
                }
                *pending = Some(PendingConnect { response });
            }

            let shared = Arc::clone(self);
            if let Err(error) = self.app.run_on_main_thread(move || {
                APPLE_RUNTIME.with_borrow_mut(|runtime| {
                    let Some(runtime) = runtime.as_mut() else {
                        shared.fail_connect("Apple 蓝牙运行时尚未初始化".into());
                        return;
                    };
                    runtime.begin_connect(target_id);
                });
            }) {
                self.fail_connect(format!("无法调度蓝牙连接：{error}"));
            }

            match timeout(CONNECTION_TIMEOUT, result).await {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(_)) => Err("蓝牙连接流程意外停止".into()),
                Err(_) => {
                    self.pending_connect.lock().take();
                    self.stop_native_session("outgoing-pending".into());
                    Err("搜索并连接目标蓝牙设备超时；请确认两端蓝牙均已启用".into())
                }
            }
        }

        pub(super) async fn ensure_ready(self: &Arc<Self>) -> Result<(), String> {
            if let Some(outcome) = self.availability.read().outcome() {
                return outcome;
            }
            self.start_managers()?;

            let deadline = Instant::now() + AVAILABILITY_TIMEOUT;
            loop {
                let changed = self.changed.notified();
                if let Some(outcome) = self.availability.read().outcome() {
                    return outcome;
                }
                timeout_at(deadline, changed).await.map_err(|_| {
                    "等待蓝牙授权或硬件状态超时；请检查系统蓝牙权限后重试".to_string()
                })?;
            }
        }

        fn start_managers(self: &Arc<Self>) -> Result<(), String> {
            if self
                .started
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Ok(());
            }

            let shared = Arc::clone(self);
            if let Err(error) = self.app.run_on_main_thread(move || {
                let Some(marker) = MainThreadMarker::new() else {
                    shared.fail_startup("无法在主线程初始化 Apple 蓝牙服务".into());
                    return;
                };
                APPLE_RUNTIME.with_borrow_mut(|runtime| {
                    if runtime.is_none() {
                        *runtime = Some(AppleRuntime::new(marker, shared));
                    }
                });
            }) {
                self.started.store(false, Ordering::Release);
                return Err(format!("无法调度 Apple 蓝牙初始化：{error}"));
            }
            Ok(())
        }

        fn update_central(&self, state: CBManagerState) {
            self.availability.write().central = state.into();
            self.changed.notify_waiters();
        }

        fn update_peripheral(&self, state: CBManagerState) {
            self.availability.write().peripheral = state.into();
            self.changed.notify_waiters();
        }

        fn fail_startup(&self, error: String) {
            self.availability.write().startup_error = Some(error);
            self.changed.notify_waiters();
        }

        pub(super) fn update_local_device(&self, device: LocalDevice) {
            *self.local.write() = device;
        }

        fn identity_json(&self) -> Vec<u8> {
            let local = self.local.read();
            serde_json::to_vec(&BluetoothIdentity {
                id: local.id.clone(),
                name: local.name.clone(),
                platform: local.platform.clone(),
                version: local.version.clone(),
                protocol_version: PROTOCOL_VERSION,
                min_protocol_version: MIN_PROTOCOL_VERSION,
                capabilities: local_capabilities(),
            })
            .expect("Bluetooth identity contains only serializable application metadata")
        }

        fn create_native_link(
            &self,
            session_key: String,
            maximum_packet_size: usize,
        ) -> NativeLink {
            let (packets, inbound) = mpsc::channel(PACKET_QUEUE_DEPTH);
            self.sessions.lock().insert(session_key.clone(), packets);
            NativeLink {
                session_key,
                maximum_packet_size,
                inbound,
            }
        }

        fn establish_outgoing(&self, session_key: String, maximum_packet_size: usize) {
            let native = self.create_native_link(session_key, maximum_packet_size);
            if let Some(pending) = self.pending_connect.lock().take() {
                if let Err(Ok(native)) = pending.response.send(Ok(native)) {
                    self.sessions.lock().remove(&native.session_key);
                }
            } else {
                self.sessions.lock().remove(&native.session_key);
            }
        }

        fn establish_incoming(&self, session_key: String, maximum_packet_size: usize) -> bool {
            if self.sessions.lock().contains_key(&session_key) {
                return true;
            }
            let native = self.create_native_link(session_key.clone(), maximum_packet_size);
            match self.incoming.try_send(IncomingBluetoothLink { native }) {
                Ok(()) => true,
                Err(_) => {
                    self.sessions.lock().remove(&session_key);
                    false
                }
            }
        }

        fn fail_connect(&self, error: String) {
            if let Some(pending) = self.pending_connect.lock().take() {
                let _ = pending.response.send(Err(error));
            }
        }

        fn schedule_discovery(self: &Arc<Self>) {
            if self
                .scan_scheduled
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return;
            }
            let shared = Arc::clone(self);
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(15)).await;
                shared.scan_scheduled.store(false, Ordering::Release);
                let _ = shared.app.run_on_main_thread(|| {
                    APPLE_RUNTIME.with_borrow_mut(|runtime| {
                        if let Some(runtime) = runtime.as_mut() {
                            runtime.start_scan_if_idle();
                        }
                    });
                });
            });
        }

        pub(super) fn start_cleanup(self: &Arc<Self>) {
            if self
                .cleanup_started
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return;
            }
            let shared = Arc::downgrade(self);
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let Some(shared) = Weak::upgrade(&shared) else {
                        return;
                    };
                    shared.prune_stale_peers();
                }
            });
        }

        fn prune_stale_peers(&self) {
            let cutoff = unix_millis().saturating_sub(120_000);
            let expired = {
                let mut seen = self.bluetooth_seen.lock();
                let expired = seen
                    .iter()
                    .filter(|(_, last_seen)| **last_seen < cutoff)
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                seen.retain(|_, last_seen| *last_seen >= cutoff);
                expired
            };
            if expired.is_empty() {
                return;
            }
            let mut peers = self.peers.write();
            for id in expired {
                if let Some(peer) = peers.get_mut(&id) {
                    peer.bluetooth_available = false;
                }
            }
            peers.retain(|_, peer| {
                !peer.addresses.is_empty()
                    || peer.relay_available
                    || peer.peer_to_peer_available
                    || peer.bluetooth_available
            });
            drop(peers);
            self.emit_peers();
        }

        fn clear_bluetooth_peers(&self) {
            let ids = self
                .bluetooth_seen
                .lock()
                .drain()
                .map(|(id, _)| id)
                .collect::<Vec<_>>();
            if ids.is_empty() {
                return;
            }
            let mut peers = self.peers.write();
            for id in ids {
                if let Some(peer) = peers.get_mut(&id) {
                    peer.bluetooth_available = false;
                }
            }
            peers.retain(|_, peer| {
                !peer.addresses.is_empty()
                    || peer.relay_available
                    || peer.peer_to_peer_available
                    || peer.bluetooth_available
            });
            drop(peers);
            self.emit_peers();
        }

        fn upsert_bluetooth_peer(&self, mut identity: BluetoothIdentity) {
            if identity.id.is_empty() || identity.id == self.local.read().id {
                return;
            }
            if !identity
                .capabilities
                .iter()
                .any(|capability| capability == CAPABILITY_BLUETOOTH)
            {
                identity.capabilities.push(CAPABILITY_BLUETOOTH.to_string());
            }
            let now = unix_millis();
            self.bluetooth_seen.lock().insert(identity.id.clone(), now);
            let mut peers = self.peers.write();
            if let Some(peer) = peers.get_mut(&identity.id) {
                peer.name = identity.name;
                peer.platform = identity.platform;
                peer.version = identity.version;
                peer.protocol_version = identity.protocol_version;
                peer.min_protocol_version = identity.min_protocol_version;
                peer.capabilities = identity.capabilities;
                peer.bluetooth_available = true;
                peer.last_seen_ms = now;
            } else {
                peers.insert(
                    identity.id.clone(),
                    PeerDevice {
                        id: identity.id,
                        name: identity.name,
                        platform: identity.platform,
                        version: identity.version,
                        protocol_version: identity.protocol_version,
                        min_protocol_version: identity.min_protocol_version,
                        capabilities: identity.capabilities,
                        addresses: Vec::new(),
                        port: 0,
                        last_seen_ms: now,
                        relay_available: false,
                        peer_to_peer_available: false,
                        bluetooth_available: true,
                        peer_to_peer_device_address: None,
                        peer_to_peer_address: None,
                        service_fullname: String::new(),
                    },
                );
            }
            drop(peers);
            self.emit_peers();
        }

        fn emit_peers(&self) {
            let mut snapshot_peers = self.peers.read().values().cloned().collect::<Vec<_>>();
            snapshot_peers.sort_by_cached_key(|peer| peer.name.to_lowercase());
            let _ = self.app.emit(
                "peers-changed",
                DiscoverySnapshot {
                    active: true,
                    error: None,
                    peers: snapshot_peers,
                },
            );
        }

        fn receive_packet(&self, session_key: &str, packet: Vec<u8>) -> Result<(), String> {
            self.receive_packets(vec![(session_key.to_string(), packet)])
        }

        fn receive_packets(&self, packets: Vec<(String, Vec<u8>)>) -> Result<(), String> {
            let senders = {
                let sessions = self.sessions.lock();
                packets
                    .iter()
                    .map(|(session_key, _)| {
                        sessions
                            .get(session_key)
                            .cloned()
                            .ok_or_else(|| "蓝牙会话尚未准备好".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            let permits = senders
                .into_iter()
                .map(|sender| {
                    sender.try_reserve_owned().map_err(|error| match error {
                        mpsc::error::TrySendError::Full(_) => {
                            "蓝牙接收队列已满，为避免静默丢包已终止会话".to_string()
                        }
                        mpsc::error::TrySendError::Closed(_) => "蓝牙接收端已关闭".to_string(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            for (permit, (_, packet)) in permits.into_iter().zip(packets) {
                permit.send(packet);
            }
            Ok(())
        }

        pub(super) async fn send_packet(
            self: &Arc<Self>,
            session_key: String,
            packet: Vec<u8>,
        ) -> io::Result<()> {
            let (response, result) = oneshot::channel();
            self.app
                .run_on_main_thread(move || {
                    APPLE_RUNTIME.with_borrow_mut(|runtime| {
                        let outcome = if let Some(runtime) = runtime.as_mut() {
                            runtime.queue_packet(session_key, packet, response)
                        } else {
                            Err(("Apple 蓝牙运行时已停止".to_string(), response))
                        };
                        if let Err((error, response)) = outcome {
                            let _ = response.send(Err(error));
                        }
                    });
                })
                .map_err(|error| io::Error::other(format!("无法调度蓝牙发送：{error}")))?;

            timeout(PACKET_SEND_TIMEOUT, result)
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "蓝牙发送等待可写状态超时"))?
                .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "蓝牙发送回执已关闭"))?
                .map_err(io::Error::other)
        }

        pub(super) fn close_session(&self, session_key: &str) {
            self.sessions.lock().remove(session_key);
        }

        pub(super) fn stop_native_session(&self, session_key: String) {
            let _ = self.app.run_on_main_thread(move || {
                APPLE_RUNTIME.with_borrow_mut(|runtime| {
                    if let Some(runtime) = runtime.as_mut() {
                        runtime.stop_session(&session_key);
                    }
                });
            });
        }
    }

    struct BluetoothDelegateIvars {
        shared: Arc<Shared>,
    }

    define_class!(
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        #[ivars = BluetoothDelegateIvars]
        struct BluetoothDelegate;

        unsafe impl NSObjectProtocol for BluetoothDelegate {}

        unsafe impl CBCentralManagerDelegate for BluetoothDelegate {
            #[unsafe(method(centralManagerDidUpdateState:))]
            fn central_manager_did_update_state(&self, central: &CBCentralManager) {
                let state = unsafe { central.state() };
                self.ivars().shared.update_central(state);
                with_runtime(|runtime| runtime.central_state_changed(state));
            }

            #[unsafe(method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:))]
            fn did_discover(
                &self,
                central: &CBCentralManager,
                peripheral: &CBPeripheral,
                _advertisement_data: &NSDictionary<NSString, AnyObject>,
                _rssi: &NSNumber,
            ) {
                with_runtime(|runtime| runtime.did_discover(central, peripheral));
            }

            #[unsafe(method(centralManager:didConnectPeripheral:))]
            fn did_connect(&self, _central: &CBCentralManager, peripheral: &CBPeripheral) {
                with_runtime(|runtime| runtime.did_connect(peripheral));
            }

            #[unsafe(method(centralManager:didFailToConnectPeripheral:error:))]
            fn did_fail_connect(
                &self,
                _central: &CBCentralManager,
                peripheral: &CBPeripheral,
                _error: Option<&NSError>,
            ) {
                with_runtime(|runtime| runtime.did_fail_connect(peripheral));
            }

            #[unsafe(method(centralManager:didDisconnectPeripheral:error:))]
            fn did_disconnect(
                &self,
                _central: &CBCentralManager,
                peripheral: &CBPeripheral,
                error: Option<&NSError>,
            ) {
                with_runtime(|runtime| runtime.did_disconnect(peripheral, error));
            }
        }

        unsafe impl CBPeripheralDelegate for BluetoothDelegate {
            #[unsafe(method(peripheral:didDiscoverServices:))]
            fn did_discover_services(&self, peripheral: &CBPeripheral, error: Option<&NSError>) {
                with_runtime(|runtime| runtime.did_discover_services(peripheral, error));
            }

            #[unsafe(method(peripheral:didDiscoverCharacteristicsForService:error:))]
            fn did_discover_characteristics(
                &self,
                peripheral: &CBPeripheral,
                service: &CBService,
                error: Option<&NSError>,
            ) {
                with_runtime(|runtime| {
                    runtime.did_discover_characteristics(peripheral, service, error)
                });
            }

            #[unsafe(method(peripheral:didUpdateValueForCharacteristic:error:))]
            fn did_update_value(
                &self,
                peripheral: &CBPeripheral,
                characteristic: &CBCharacteristic,
                error: Option<&NSError>,
            ) {
                with_runtime(|runtime| runtime.did_update_value(peripheral, characteristic, error));
            }

            #[unsafe(method(peripheral:didUpdateNotificationStateForCharacteristic:error:))]
            fn did_update_notification_state(
                &self,
                peripheral: &CBPeripheral,
                characteristic: &CBCharacteristic,
                error: Option<&NSError>,
            ) {
                with_runtime(|runtime| {
                    runtime.did_update_notification_state(peripheral, characteristic, error)
                });
            }

            #[unsafe(method(peripheralIsReadyToSendWriteWithoutResponse:))]
            fn ready_to_write(&self, peripheral: &CBPeripheral) {
                with_runtime(|runtime| runtime.flush_outgoing(peripheral));
            }
        }

        unsafe impl CBPeripheralManagerDelegate for BluetoothDelegate {
            #[unsafe(method(peripheralManagerDidUpdateState:))]
            fn peripheral_manager_did_update_state(&self, peripheral: &CBPeripheralManager) {
                let state = unsafe { peripheral.state() };
                self.ivars().shared.update_peripheral(state);
                with_runtime(|runtime| runtime.peripheral_state_changed(state));
            }

            #[unsafe(method(peripheralManager:didAddService:error:))]
            fn did_add_service(
                &self,
                peripheral: &CBPeripheralManager,
                _service: &CBService,
                error: Option<&NSError>,
            ) {
                with_runtime(|runtime| runtime.did_add_service(peripheral, error));
            }

            #[unsafe(method(peripheralManagerDidStartAdvertising:error:))]
            fn did_start_advertising(
                &self,
                _peripheral: &CBPeripheralManager,
                error: Option<&NSError>,
            ) {
                if let Some(error) = error {
                    self.ivars()
                        .shared
                        .fail_startup(format!("无法广播 Neloa 蓝牙服务：{}", error_text(error)));
                }
            }

            #[unsafe(method(peripheralManager:central:didSubscribeToCharacteristic:))]
            fn did_subscribe(
                &self,
                _peripheral: &CBPeripheralManager,
                central: &CBCentral,
                characteristic: &CBCharacteristic,
            ) {
                with_runtime(|runtime| runtime.did_subscribe(central, characteristic));
            }

            #[unsafe(method(peripheralManager:central:didUnsubscribeFromCharacteristic:))]
            fn did_unsubscribe(
                &self,
                _peripheral: &CBPeripheralManager,
                central: &CBCentral,
                _characteristic: &CBCharacteristic,
            ) {
                with_runtime(|runtime| runtime.did_unsubscribe(central));
            }

            #[unsafe(method(peripheralManager:didReceiveWriteRequests:))]
            fn did_receive_writes(
                &self,
                peripheral: &CBPeripheralManager,
                requests: &NSArray<CBATTRequest>,
            ) {
                with_runtime(|runtime| runtime.did_receive_writes(peripheral, requests));
            }

            #[unsafe(method(peripheralManager:didReceiveReadRequest:))]
            fn did_receive_read(&self, peripheral: &CBPeripheralManager, request: &CBATTRequest) {
                with_runtime(|runtime| runtime.did_receive_read(peripheral, request));
            }

            #[unsafe(method(peripheralManagerIsReadyToUpdateSubscribers:))]
            fn ready_to_notify(&self, peripheral: &CBPeripheralManager) {
                with_runtime(|runtime| runtime.flush_incoming(peripheral));
            }
        }
    );

    impl BluetoothDelegate {
        fn new(marker: MainThreadMarker, shared: Arc<Shared>) -> Retained<Self> {
            let this = Self::alloc(marker).set_ivars(BluetoothDelegateIvars { shared });
            unsafe { msg_send![super(this), init] }
        }
    }

    struct QueuedPacket {
        data: Vec<u8>,
        response: PacketAck,
    }

    struct OutgoingNative {
        target_id: String,
        peripheral: Option<Retained<CBPeripheral>>,
        write: Option<Retained<CBCharacteristic>>,
        notify: Option<Retained<CBCharacteristic>>,
        session_key: Option<String>,
        pending: VecDeque<QueuedPacket>,
        retry_after_disconnect: bool,
        established: bool,
    }

    impl OutgoingNative {
        fn new(target_id: String) -> Self {
            Self {
                target_id,
                peripheral: None,
                write: None,
                notify: None,
                session_key: None,
                pending: VecDeque::new(),
                retry_after_disconnect: false,
                established: false,
            }
        }
    }

    struct IncomingNative {
        central: Retained<CBCentral>,
        pending: VecDeque<QueuedPacket>,
    }

    struct ProbeNative {
        peripheral: Retained<CBPeripheral>,
    }

    struct AppleRuntime {
        shared: Arc<Shared>,
        delegate: Retained<BluetoothDelegate>,
        central: Retained<CBCentralManager>,
        peripheral: Retained<CBPeripheralManager>,
        service: Retained<CBMutableService>,
        notify_characteristic: Retained<CBMutableCharacteristic>,
        outgoing: Option<OutgoingNative>,
        probe: Option<ProbeNative>,
        incoming: HashMap<String, IncomingNative>,
        service_published: bool,
    }

    impl AppleRuntime {
        fn new(marker: MainThreadMarker, shared: Arc<Shared>) -> Self {
            let delegate = BluetoothDelegate::new(marker, Arc::clone(&shared));
            let central_delegate = ProtocolObject::from_ref(&*delegate);
            let peripheral_delegate = ProtocolObject::from_ref(&*delegate);
            let central = unsafe {
                CBCentralManager::initWithDelegate_queue(
                    CBCentralManager::alloc(),
                    Some(central_delegate),
                    None,
                )
            };
            let peripheral = unsafe {
                CBPeripheralManager::initWithDelegate_queue(
                    CBPeripheralManager::alloc(),
                    Some(peripheral_delegate),
                    None,
                )
            };

            let identity = unsafe {
                CBMutableCharacteristic::initWithType_properties_value_permissions(
                    CBMutableCharacteristic::alloc(),
                    &uuid(IDENTITY_UUID),
                    CBCharacteristicProperties::Read,
                    None,
                    CBAttributePermissions::Readable,
                )
            };
            let write = unsafe {
                CBMutableCharacteristic::initWithType_properties_value_permissions(
                    CBMutableCharacteristic::alloc(),
                    &uuid(WRITE_UUID),
                    CBCharacteristicProperties::Write
                        | CBCharacteristicProperties::WriteWithoutResponse,
                    None,
                    CBAttributePermissions::Writeable,
                )
            };
            let notify = unsafe {
                CBMutableCharacteristic::initWithType_properties_value_permissions(
                    CBMutableCharacteristic::alloc(),
                    &uuid(NOTIFY_UUID),
                    CBCharacteristicProperties::Notify,
                    None,
                    CBAttributePermissions::empty(),
                )
            };
            let characteristics = NSArray::from_retained_slice(&[
                identity.into_super(),
                write.into_super(),
                notify.clone().into_super(),
            ]);
            let service = unsafe {
                CBMutableService::initWithType_primary(
                    CBMutableService::alloc(),
                    &uuid(SERVICE_UUID),
                    true,
                )
            };
            unsafe { service.setCharacteristics(Some(&characteristics)) };

            Self {
                shared,
                delegate,
                central,
                peripheral,
                service,
                notify_characteristic: notify,
                outgoing: None,
                probe: None,
                incoming: HashMap::new(),
                service_published: false,
            }
        }

        fn peripheral_state_changed(&mut self, state: CBManagerState) {
            if state == CBManagerState::PoweredOn && !self.service_published {
                unsafe { self.peripheral.addService(&self.service) };
                self.service_published = true;
            } else if state != CBManagerState::PoweredOn {
                self.service_published = false;
                self.fail_all_incoming("蓝牙外设服务已停止");
            }
        }

        fn central_state_changed(&mut self, state: CBManagerState) {
            if state == CBManagerState::PoweredOn && self.outgoing.is_none() && self.probe.is_none()
            {
                self.start_scan();
            } else if state != CBManagerState::PoweredOn {
                unsafe { self.central.stopScan() };
                self.shared.clear_bluetooth_peers();
                if let Some(probe) = self.probe.take() {
                    unsafe { self.central.cancelPeripheralConnection(&probe.peripheral) };
                }
                if let Some(mut outgoing) = self.outgoing.take() {
                    while let Some(packet) = outgoing.pending.pop_front() {
                        let _ = packet.response.send(Err("蓝牙中心服务已停止".into()));
                    }
                    if let Some(session_key) = outgoing.session_key {
                        self.shared.close_session(&session_key);
                    }
                }
                self.shared.fail_connect("蓝牙中心服务已停止".into());
            }
        }

        fn did_add_service(&mut self, peripheral: &CBPeripheralManager, error: Option<&NSError>) {
            if let Some(error) = error {
                self.service_published = false;
                self.shared
                    .fail_startup(format!("无法发布 Neloa 蓝牙服务：{}", error_text(error)));
                return;
            }
            let service_uuid = uuid(SERVICE_UUID);
            let service_uuids = NSArray::from_slice(&[&*service_uuid]);
            let service_uuids_object: &AnyObject = service_uuids.as_ref();
            let service_key = unsafe { CBAdvertisementDataServiceUUIDsKey };
            let advertisement = NSDictionary::<NSString, AnyObject>::from_slices(
                &[service_key],
                &[service_uuids_object],
            );
            unsafe { peripheral.startAdvertising(Some(&advertisement)) };
        }

        fn begin_connect(&mut self, target_id: String) {
            if self.outgoing.is_some() {
                self.shared.fail_connect("已有蓝牙会话正在使用中".into());
                return;
            }
            unsafe { self.central.stopScan() };
            self.outgoing = Some(OutgoingNative::new(target_id));
            if let Some(probe) = self.probe.as_ref() {
                unsafe { self.central.cancelPeripheralConnection(&probe.peripheral) };
                return;
            }
            self.start_scan();
        }

        fn start_scan(&self) {
            if self.shared.availability.read().central != ManagerState::PoweredOn {
                return;
            }
            let service = uuid(SERVICE_UUID);
            let services = NSArray::from_slice(&[&*service]);
            unsafe {
                self.central
                    .scanForPeripheralsWithServices_options(Some(&services), None)
            };
        }

        fn start_scan_if_idle(&self) {
            if self.outgoing.is_none()
                && self.probe.is_none()
                && self.shared.availability.read().central == ManagerState::PoweredOn
            {
                self.start_scan();
            }
        }

        fn did_discover(&mut self, central: &CBCentralManager, peripheral: &CBPeripheral) {
            if self.outgoing.is_none() {
                if self.probe.is_some() {
                    return;
                }
                unsafe { central.stopScan() };
                let retained = peripheral.retain();
                let delegate = ProtocolObject::from_ref(&*self.delegate);
                unsafe {
                    retained.setDelegate(Some(delegate));
                    central.connectPeripheral_options(&retained, None);
                }
                self.probe = Some(ProbeNative {
                    peripheral: retained,
                });
                return;
            }
            let Some(outgoing) = self.outgoing.as_mut() else {
                return;
            };
            if outgoing.peripheral.is_some() {
                return;
            }
            unsafe { central.stopScan() };
            let retained = peripheral.retain();
            let delegate = ProtocolObject::from_ref(&*self.delegate);
            unsafe {
                retained.setDelegate(Some(delegate));
                central.connectPeripheral_options(&retained, None);
            }
            outgoing.peripheral = Some(retained);
        }

        fn did_connect(&mut self, peripheral: &CBPeripheral) {
            if !self.is_current_peripheral(peripheral) && !self.is_probe_peripheral(peripheral) {
                return;
            }
            let services = NSArray::from_slice(&[&*uuid(SERVICE_UUID)]);
            unsafe { peripheral.discoverServices(Some(&services)) };
        }

        fn did_discover_services(&mut self, peripheral: &CBPeripheral, error: Option<&NSError>) {
            let is_outgoing = self.is_current_peripheral(peripheral);
            let is_probe = self.is_probe_peripheral(peripheral);
            if error.is_some() || (!is_outgoing && !is_probe) {
                self.reject_candidate(peripheral);
                return;
            }
            let service = unsafe { peripheral.services() }.and_then(|services| {
                services
                    .iter()
                    .find(|service| uuid_matches(service, SERVICE_UUID))
            });
            let Some(service) = service else {
                self.reject_candidate(peripheral);
                return;
            };
            let uuids = if is_probe {
                vec![uuid(IDENTITY_UUID)]
            } else {
                vec![uuid(IDENTITY_UUID), uuid(WRITE_UUID), uuid(NOTIFY_UUID)]
            };
            let uuids = NSArray::from_retained_slice(&uuids);
            unsafe { peripheral.discoverCharacteristics_forService(Some(&uuids), &service) };
        }

        fn did_discover_characteristics(
            &mut self,
            peripheral: &CBPeripheral,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            let is_outgoing = self.is_current_peripheral(peripheral);
            let is_probe = self.is_probe_peripheral(peripheral);
            if error.is_some() || (!is_outgoing && !is_probe) {
                self.reject_candidate(peripheral);
                return;
            }
            let Some(characteristics) = (unsafe { service.characteristics() }) else {
                self.reject_candidate(peripheral);
                return;
            };
            let mut identity = None;
            let mut write = None;
            let mut notify = None;
            for characteristic in characteristics.iter() {
                if uuid_matches(&characteristic, IDENTITY_UUID) {
                    identity = Some(characteristic.retain());
                } else if uuid_matches(&characteristic, WRITE_UUID) {
                    write = Some(characteristic.retain());
                } else if uuid_matches(&characteristic, NOTIFY_UUID) {
                    notify = Some(characteristic.retain());
                }
            }
            if is_probe {
                let Some(identity) = identity else {
                    self.reject_candidate(peripheral);
                    return;
                };
                unsafe { peripheral.readValueForCharacteristic(&identity) };
                return;
            }
            let (Some(identity), Some(write), Some(notify)) = (identity, write, notify) else {
                self.reject_candidate(peripheral);
                return;
            };
            if let Some(outgoing) = self.outgoing.as_mut() {
                outgoing.write = Some(write);
                outgoing.notify = Some(notify);
            }
            unsafe { peripheral.readValueForCharacteristic(&identity) };
        }

        fn did_update_value(
            &mut self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            let is_probe = self.is_probe_peripheral(peripheral);
            if error.is_some() || (!self.is_current_peripheral(peripheral) && !is_probe) {
                self.reject_candidate(peripheral);
                return;
            }
            let Some(value) = (unsafe { characteristic.value() }) else {
                self.reject_candidate(peripheral);
                return;
            };
            if uuid_matches(characteristic, IDENTITY_UUID) {
                if is_probe {
                    if let Ok(identity) =
                        serde_json::from_slice::<BluetoothIdentity>(&value.to_vec())
                    {
                        self.shared.upsert_bluetooth_peer(identity);
                    }
                    self.finish_probe(peripheral);
                    return;
                }
                let identity = serde_json::from_slice::<BluetoothIdentity>(&value.to_vec()).ok();
                let expected = self.outgoing.as_ref().map(|outgoing| &outgoing.target_id);
                if identity.as_ref().map(|identity| &identity.id) != expected {
                    self.reject_candidate(peripheral);
                    return;
                }
                if let Some(notify) = self
                    .outgoing
                    .as_ref()
                    .and_then(|outgoing| outgoing.notify.clone())
                {
                    unsafe { peripheral.setNotifyValue_forCharacteristic(true, &notify) };
                }
                return;
            }
            if uuid_matches(characteristic, NOTIFY_UUID) {
                let session_key = self
                    .outgoing
                    .as_ref()
                    .and_then(|outgoing| outgoing.session_key.clone());
                if let Some(session_key) = session_key {
                    if self
                        .shared
                        .receive_packet(&session_key, value.to_vec())
                        .is_err()
                    {
                        self.stop_session(&session_key);
                    }
                }
            }
        }

        fn did_update_notification_state(
            &mut self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            if error.is_some()
                || !uuid_matches(characteristic, NOTIFY_UUID)
                || !self.is_current_peripheral(peripheral)
                || !(unsafe { characteristic.isNotifying() })
            {
                self.reject_candidate(peripheral);
                return;
            }
            let maximum = unsafe {
                peripheral
                    .maximumWriteValueLengthForType(CBCharacteristicWriteType::WithoutResponse)
            };
            let session_key = format!("central:{}", peer_identifier(peripheral));
            if let Some(outgoing) = self.outgoing.as_mut() {
                outgoing.session_key = Some(session_key.clone());
                outgoing.established = true;
            }
            self.shared.establish_outgoing(session_key, maximum);
        }

        fn reject_candidate(&mut self, peripheral: &CBPeripheral) {
            if self.is_probe_peripheral(peripheral) {
                unsafe { self.central.cancelPeripheralConnection(peripheral) };
                return;
            }
            if !self.is_current_peripheral(peripheral) {
                return;
            }
            if let Some(outgoing) = self.outgoing.as_mut() {
                outgoing.retry_after_disconnect = true;
            }
            unsafe { self.central.cancelPeripheralConnection(peripheral) };
        }

        fn did_fail_connect(&mut self, peripheral: &CBPeripheral) {
            if self.is_probe_peripheral(peripheral) {
                self.probe = None;
                if self.outgoing.is_some() {
                    self.start_scan();
                } else {
                    self.shared.schedule_discovery();
                }
                return;
            }
            if !self.is_current_peripheral(peripheral) {
                return;
            }
            let target = self.outgoing.take().map(|outgoing| outgoing.target_id);
            if let Some(target) = target {
                self.outgoing = Some(OutgoingNative::new(target));
                self.start_scan();
            }
        }

        fn finish_probe(&mut self, peripheral: &CBPeripheral) {
            if self.is_probe_peripheral(peripheral) {
                unsafe { self.central.cancelPeripheralConnection(peripheral) };
            }
        }

        fn did_disconnect(&mut self, peripheral: &CBPeripheral, error: Option<&NSError>) {
            if self.is_probe_peripheral(peripheral) {
                self.probe = None;
                if self.outgoing.is_some() {
                    self.start_scan();
                } else {
                    self.shared.schedule_discovery();
                }
                return;
            }
            if !self.is_current_peripheral(peripheral) {
                return;
            }
            let Some(mut outgoing) = self.outgoing.take() else {
                return;
            };
            while let Some(packet) = outgoing.pending.pop_front() {
                let _ = packet.response.send(Err("蓝牙连接已断开".into()));
            }
            if let Some(session_key) = outgoing.session_key {
                self.shared.close_session(&session_key);
            }
            if outgoing.retry_after_disconnect && !outgoing.established {
                let target = outgoing.target_id;
                self.outgoing = Some(OutgoingNative::new(target));
                self.start_scan();
            } else if !outgoing.established {
                let detail = error
                    .map(error_text)
                    .unwrap_or_else(|| "连接被目标设备关闭".into());
                self.shared.fail_connect(format!("蓝牙连接失败：{detail}"));
                self.shared.schedule_discovery();
            } else {
                self.shared.schedule_discovery();
            }
        }

        fn did_subscribe(&mut self, central: &CBCentral, characteristic: &CBCharacteristic) {
            if !uuid_matches(characteristic, NOTIFY_UUID) {
                return;
            }
            let session_key = format!("peripheral:{}", peer_identifier(central));
            if self.incoming.contains_key(&session_key) {
                return;
            }
            let maximum = unsafe { central.maximumUpdateValueLength() };
            if !self.shared.establish_incoming(session_key.clone(), maximum) {
                return;
            }
            self.incoming.insert(
                session_key,
                IncomingNative {
                    central: central.retain(),
                    pending: VecDeque::new(),
                },
            );
        }

        fn did_unsubscribe(&mut self, central: &CBCentral) {
            let session_key = format!("peripheral:{}", peer_identifier(central));
            if let Some(mut incoming) = self.incoming.remove(&session_key) {
                while let Some(packet) = incoming.pending.pop_front() {
                    let _ = packet.response.send(Err("蓝牙订阅已关闭".into()));
                }
            }
            self.shared.close_session(&session_key);
        }

        fn did_receive_writes(
            &mut self,
            peripheral: &CBPeripheralManager,
            requests: &NSArray<CBATTRequest>,
        ) {
            let mut result = CBATTError::Success;
            let mut packets = Vec::with_capacity(requests.len());
            for request in requests.iter() {
                let characteristic = unsafe { request.characteristic() };
                if !uuid_matches(&*characteristic, WRITE_UUID) {
                    result = CBATTError::WriteNotPermitted;
                    break;
                }
                if unsafe { request.offset() } != 0 {
                    result = CBATTError::InvalidOffset;
                    break;
                }
                let Some(value) = (unsafe { request.value() }) else {
                    result = CBATTError::InvalidAttributeValueLength;
                    break;
                };
                let central = unsafe { request.central() };
                let session_key = format!("peripheral:{}", peer_identifier(&*central));
                if !self.incoming.contains_key(&session_key) {
                    result = CBATTError::InsufficientAuthorization;
                    break;
                }
                packets.push((session_key, value.to_vec()));
            }
            if result == CBATTError::Success && self.shared.receive_packets(packets).is_err() {
                result = CBATTError::InsufficientResources;
            }
            if let Some(request) = requests.iter().next() {
                unsafe { peripheral.respondToRequest_withResult(&request, result) };
            }
        }

        fn did_receive_read(&mut self, peripheral: &CBPeripheralManager, request: &CBATTRequest) {
            let characteristic = unsafe { request.characteristic() };
            if !uuid_matches(&*characteristic, IDENTITY_UUID) {
                unsafe {
                    peripheral.respondToRequest_withResult(request, CBATTError::ReadNotPermitted)
                };
                return;
            }
            let identity = self.shared.identity_json();
            let offset = unsafe { request.offset() };
            if offset > identity.len() {
                unsafe {
                    peripheral.respondToRequest_withResult(request, CBATTError::InvalidOffset)
                };
                return;
            }
            let value = NSData::with_bytes(&identity[offset..]);
            unsafe {
                request.setValue(Some(&value));
                peripheral.respondToRequest_withResult(request, CBATTError::Success);
            }
        }

        fn queue_packet(
            &mut self,
            session_key: String,
            data: Vec<u8>,
            response: PacketAck,
        ) -> Result<(), (String, PacketAck)> {
            if self
                .outgoing
                .as_ref()
                .and_then(|outgoing| outgoing.session_key.as_deref())
                == Some(session_key.as_str())
            {
                if let Some(outgoing) = self.outgoing.as_mut() {
                    outgoing.pending.push_back(QueuedPacket { data, response });
                }
                let peripheral = self
                    .outgoing
                    .as_ref()
                    .and_then(|outgoing| outgoing.peripheral.clone())
                    .expect("established outgoing session has a peripheral");
                self.flush_outgoing(&peripheral);
                return Ok(());
            }
            if let Some(incoming) = self.incoming.get_mut(&session_key) {
                incoming.pending.push_back(QueuedPacket { data, response });
                let manager = self.peripheral.retain();
                self.flush_incoming(&manager);
                return Ok(());
            }
            Err(("蓝牙会话不存在或已关闭".into(), response))
        }

        fn flush_outgoing(&mut self, peripheral: &CBPeripheral) {
            let Some(outgoing) = self.outgoing.as_mut() else {
                return;
            };
            let Some(write) = outgoing.write.as_ref() else {
                return;
            };
            while unsafe { peripheral.canSendWriteWithoutResponse() } {
                let Some(packet) = outgoing.pending.pop_front() else {
                    break;
                };
                let data = NSData::with_bytes(&packet.data);
                unsafe {
                    peripheral.writeValue_forCharacteristic_type(
                        &data,
                        write,
                        CBCharacteristicWriteType::WithoutResponse,
                    )
                };
                let _ = packet.response.send(Ok(()));
            }
        }

        fn flush_incoming(&mut self, peripheral: &CBPeripheralManager) {
            let keys = self.incoming.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                let Some(incoming) = self.incoming.get_mut(&key) else {
                    continue;
                };
                while let Some(packet) = incoming.pending.pop_front() {
                    let data = NSData::with_bytes(&packet.data);
                    let centrals = NSArray::from_slice(&[&*incoming.central]);
                    let sent = unsafe {
                        peripheral.updateValue_forCharacteristic_onSubscribedCentrals(
                            &data,
                            &self.notify_characteristic,
                            Some(&centrals),
                        )
                    };
                    if sent {
                        let _ = packet.response.send(Ok(()));
                    } else {
                        incoming.pending.push_front(packet);
                        return;
                    }
                }
            }
        }

        fn stop_session(&mut self, session_key: &str) {
            if session_key == "outgoing-pending" {
                if let Some(outgoing) = self.outgoing.take() {
                    unsafe { self.central.stopScan() };
                    if let Some(peripheral) = outgoing.peripheral {
                        unsafe { self.central.cancelPeripheralConnection(&peripheral) };
                    }
                }
                self.shared.schedule_discovery();
                return;
            }
            if self
                .outgoing
                .as_ref()
                .and_then(|outgoing| outgoing.session_key.as_deref())
                == Some(session_key)
            {
                if let Some(mut outgoing) = self.outgoing.take() {
                    while let Some(packet) = outgoing.pending.pop_front() {
                        let _ = packet.response.send(Err("蓝牙会话已关闭".into()));
                    }
                    if let Some(peripheral) = outgoing.peripheral {
                        unsafe { self.central.cancelPeripheralConnection(&peripheral) };
                    }
                }
                self.shared.schedule_discovery();
                return;
            }
            if let Some(mut incoming) = self.incoming.remove(session_key) {
                while let Some(packet) = incoming.pending.pop_front() {
                    let _ = packet.response.send(Err("蓝牙会话已关闭".into()));
                }
            }
        }

        fn fail_all_incoming(&mut self, reason: &str) {
            for (session_key, mut incoming) in self.incoming.drain() {
                while let Some(packet) = incoming.pending.pop_front() {
                    let _ = packet.response.send(Err(reason.into()));
                }
                self.shared.close_session(&session_key);
            }
        }

        fn is_current_peripheral(&self, peripheral: &CBPeripheral) -> bool {
            self.outgoing
                .as_ref()
                .and_then(|outgoing| outgoing.peripheral.as_ref())
                .is_some_and(|current| peer_identifier(&**current) == peer_identifier(peripheral))
        }

        fn is_probe_peripheral(&self, peripheral: &CBPeripheral) -> bool {
            self.probe.as_ref().is_some_and(|probe| {
                peer_identifier(&*probe.peripheral) == peer_identifier(peripheral)
            })
        }
    }

    fn with_runtime(action: impl FnOnce(&mut AppleRuntime)) {
        APPLE_RUNTIME.with_borrow_mut(|runtime| {
            if let Some(runtime) = runtime.as_mut() {
                action(runtime);
            }
        });
    }

    fn uuid(value: &str) -> Retained<CBUUID> {
        unsafe { CBUUID::UUIDWithString(&NSString::from_str(value)) }
    }

    fn uuid_matches(
        attribute: &impl AsRef<objc2_core_bluetooth::CBAttribute>,
        value: &str,
    ) -> bool {
        let actual = unsafe { attribute.as_ref().UUID().UUIDString() };
        actual.to_string().eq_ignore_ascii_case(value)
    }

    fn peer_identifier(peer: &impl AsRef<objc2_core_bluetooth::CBPeer>) -> String {
        unsafe { peer.as_ref().identifier().UUIDString().to_string() }
    }

    fn error_text(error: &NSError) -> String {
        error.localizedDescription().to_string()
    }

    thread_local! {
        static APPLE_RUNTIME: RefCell<Option<AppleRuntime>> = const { RefCell::new(None) };
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn availability_requires_both_core_bluetooth_roles() {
            let mut availability = Availability {
                central: ManagerState::PoweredOn,
                peripheral: ManagerState::Unknown,
                startup_error: None,
            };
            assert!(availability.outcome().is_none());
            availability.peripheral = ManagerState::PoweredOn;
            assert_eq!(availability.outcome(), Some(Ok(())));
        }

        #[test]
        fn availability_reports_actionable_failures() {
            let denied = Availability {
                central: ManagerState::Unauthorized,
                peripheral: ManagerState::PoweredOn,
                startup_error: None,
            };
            assert!(denied
                .outcome()
                .unwrap()
                .unwrap_err()
                .contains("隐私与安全性"));

            let powered_off = Availability {
                central: ManagerState::PoweredOn,
                peripheral: ManagerState::PoweredOff,
                startup_error: None,
            };
            assert!(powered_off
                .outcome()
                .unwrap()
                .unwrap_err()
                .contains("打开蓝牙"));
        }

        #[test]
        fn discovery_identity_round_trips_within_gatt_value_limit() {
            let identity = BluetoothIdentity {
                id: "12345678-1234-1234-1234-123456789abc".into(),
                name: "🦀".repeat(crate::device_settings::MAX_DEVICE_NAME_CHARS),
                platform: "macos".into(),
                version: "0.1.13".into(),
                protocol_version: PROTOCOL_VERSION,
                min_protocol_version: MIN_PROTOCOL_VERSION,
                capabilities: local_capabilities(),
            };
            let encoded = serde_json::to_vec(&identity).unwrap();
            assert!(encoded.len() <= 512);
            let decoded: BluetoothIdentity = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(decoded.id, identity.id);
            assert_eq!(decoded.capabilities, identity.capabilities);
        }
    }
}
