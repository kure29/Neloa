use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use tauri::AppHandle;

use crate::model::{LocalDevice, PeerDevice};

#[derive(Clone, Default)]
pub(crate) struct PeerToPeerHandle {
    #[cfg(target_os = "android")]
    shared: Option<Arc<android::Shared>>,
}

impl PeerToPeerHandle {
    pub(crate) fn unavailable() -> Self {
        Self::default()
    }

    #[cfg(target_os = "android")]
    pub(crate) async fn connect(&self, peer: &PeerDevice) -> Result<String, String> {
        let shared = self
            .shared
            .as_ref()
            .ok_or_else(|| "点对点 Wi-Fi 未启动；Neloa 不会改用其他连接方式".to_string())?;
        shared.connect(peer).await
    }

    #[cfg(not(target_os = "android"))]
    pub(crate) async fn connect(&self, _peer: &PeerDevice) -> Result<String, String> {
        Err("点对点 Wi-Fi 尚未在当前平台启用；Neloa 不会改用其他连接方式".into())
    }

    #[cfg(target_os = "android")]
    pub(crate) fn update_local_device(&self, device: LocalDevice) -> Result<(), String> {
        if let Some(shared) = &self.shared {
            shared.update_local_device(device)?;
        }
        Ok(())
    }

    #[cfg(not(target_os = "android"))]
    pub(crate) fn update_local_device(&self, _device: LocalDevice) -> Result<(), String> {
        Ok(())
    }
}

pub(crate) fn start(
    app: AppHandle,
    local: LocalDevice,
    peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
) -> PeerToPeerHandle {
    #[cfg(target_os = "android")]
    {
        return android::start(app, local, peers);
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, local, peers);
        PeerToPeerHandle::unavailable()
    }
}

#[cfg(target_os = "android")]
mod android {
    use std::{
        collections::HashMap,
        net::IpAddr,
        sync::{Arc, OnceLock},
        time::Duration,
    };

    use jni::{
        objects::{GlobalRef, JObject, JString, JValue},
        JNIEnv, JavaVM,
    };
    use parking_lot::{Mutex, RwLock};
    use serde::{Deserialize, Serialize};
    use tauri::{AppHandle, Emitter};
    use tokio::{
        sync::Notify,
        time::{timeout_at, Instant},
    };

    use crate::{
        model::{
            local_capabilities, DiscoverySnapshot, LocalDevice, PeerDevice,
            CAPABILITY_PEER_TO_PEER_WIFI, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
        },
        unix_millis,
    };

    use super::PeerToPeerHandle;

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(45);
    const RENDEZVOUS_PORT: u16 = 48_632;

    pub(super) struct Shared {
        app: AppHandle,
        local: RwLock<LocalDevice>,
        peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
        routes: RwLock<HashMap<String, String>>,
        errors: Mutex<HashMap<String, String>>,
        changed: Notify,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct AndroidConfig {
        id: String,
        name: String,
        platform: String,
        version: String,
        protocol_version: u16,
        min_protocol_version: u16,
        capabilities: Vec<String>,
        port: u16,
        rendezvous_port: u16,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AndroidPeer {
        id: String,
        name: String,
        platform: String,
        version: String,
        protocol_version: u16,
        min_protocol_version: u16,
        #[serde(default)]
        capabilities: Vec<String>,
        device_address: String,
    }

    #[derive(Deserialize)]
    #[serde(tag = "type", rename_all = "camelCase")]
    enum AndroidEvent {
        Peer {
            peer: AndroidPeer,
        },
        Route {
            peer_id: String,
            address: String,
            connected: bool,
        },
        Error {
            peer_id: Option<String>,
            message: String,
        },
    }

    struct AndroidBridge {
        vm: JavaVM,
        object: GlobalRef,
    }

    static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
    static BRIDGE: OnceLock<Mutex<Option<AndroidBridge>>> = OnceLock::new();

    fn bridge_slot() -> &'static Mutex<Option<AndroidBridge>> {
        BRIDGE.get_or_init(|| Mutex::new(None))
    }

    impl Shared {
        fn configuration_json(&self) -> Result<String, String> {
            let local = self.local.read().clone();
            serde_json::to_string(&AndroidConfig {
                id: local.id,
                name: local.name,
                platform: local.platform,
                version: local.version,
                protocol_version: PROTOCOL_VERSION,
                min_protocol_version: MIN_PROTOCOL_VERSION,
                capabilities: local_capabilities(),
                port: crate::SERVICE_PORT,
                rendezvous_port: RENDEZVOUS_PORT,
            })
            .map_err(|error| format!("无法生成点对点 Wi-Fi 配置：{error}"))
        }

        fn emit_peers(&self) {
            let mut peers = self.peers.read().values().cloned().collect::<Vec<_>>();
            peers.sort_by_cached_key(|peer| peer.name.to_lowercase());
            let _ = self.app.emit(
                "peers-changed",
                DiscoverySnapshot {
                    active: true,
                    error: None,
                    peers,
                },
            );
        }

        fn handle_event(&self, event: AndroidEvent) {
            match event {
                AndroidEvent::Peer { mut peer } => {
                    if peer.id.is_empty() || peer.id == self.local.read().id {
                        return;
                    }
                    if !peer
                        .capabilities
                        .iter()
                        .any(|value| value == CAPABILITY_PEER_TO_PEER_WIFI)
                    {
                        peer.capabilities
                            .push(CAPABILITY_PEER_TO_PEER_WIFI.to_string());
                    }
                    let route = self.routes.read().get(&peer.id).cloned();
                    let mut peers = self.peers.write();
                    if let Some(current) = peers.get_mut(&peer.id) {
                        current.name = peer.name;
                        current.platform = peer.platform;
                        current.version = peer.version;
                        current.protocol_version = peer.protocol_version;
                        current.min_protocol_version = peer.min_protocol_version;
                        current.capabilities = peer.capabilities;
                        current.peer_to_peer_available = true;
                        current.peer_to_peer_device_address = Some(peer.device_address);
                        current.peer_to_peer_address = route;
                        current.last_seen_ms = unix_millis();
                    } else {
                        peers.insert(
                            peer.id.clone(),
                            PeerDevice {
                                id: peer.id,
                                name: peer.name,
                                platform: peer.platform,
                                version: peer.version,
                                protocol_version: peer.protocol_version,
                                min_protocol_version: peer.min_protocol_version,
                                capabilities: peer.capabilities,
                                addresses: Vec::new(),
                                port: crate::SERVICE_PORT,
                                last_seen_ms: unix_millis(),
                                relay_available: false,
                                peer_to_peer_available: true,
                                bluetooth_available: false,
                                peer_to_peer_device_address: Some(peer.device_address),
                                peer_to_peer_address: route,
                                service_fullname: String::new(),
                            },
                        );
                    }
                    drop(peers);
                    self.emit_peers();
                }
                AndroidEvent::Route {
                    peer_id,
                    address,
                    connected,
                } => {
                    if connected {
                        if address.parse::<IpAddr>().is_err() {
                            self.errors
                                .lock()
                                .insert(peer_id, "点对点 Wi-Fi 返回了无效地址".into());
                            self.changed.notify_waiters();
                            return;
                        }
                        self.routes.write().insert(peer_id.clone(), address.clone());
                        if let Some(peer) = self.peers.write().get_mut(&peer_id) {
                            peer.peer_to_peer_address = Some(address);
                            peer.last_seen_ms = unix_millis();
                        }
                    } else {
                        self.routes.write().remove(&peer_id);
                        if let Some(peer) = self.peers.write().get_mut(&peer_id) {
                            peer.peer_to_peer_address = None;
                        }
                    }
                    self.errors.lock().remove(&peer_id);
                    self.changed.notify_waiters();
                    self.emit_peers();
                }
                AndroidEvent::Error { peer_id, message } => {
                    if let Some(peer_id) = peer_id {
                        self.errors.lock().insert(peer_id, message);
                        self.changed.notify_waiters();
                    } else {
                        let _ = self.app.emit("network-error", message);
                    }
                }
            }
        }

        pub(super) async fn connect(&self, peer: &PeerDevice) -> Result<String, String> {
            if let Some(address) = self.routes.read().get(&peer.id).cloned() {
                return Ok(address);
            }
            let device_address = peer
                .peer_to_peer_device_address
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "附近没有发现这台设备的点对点 Wi-Fi 服务".to_string())?;
            self.errors.lock().remove(&peer.id);
            invoke_connect(&peer.id, device_address)?;

            let deadline = Instant::now() + CONNECT_TIMEOUT;
            loop {
                let changed = self.changed.notified();
                if let Some(address) = self.routes.read().get(&peer.id).cloned() {
                    return Ok(address);
                }
                if let Some(error) = self.errors.lock().remove(&peer.id) {
                    return Err(error);
                }
                if timeout_at(deadline, changed).await.is_err() {
                    return Err("等待点对点 Wi-Fi 建链超时；请确认两台设备都已允许附近设备权限和系统连接请求".into());
                }
            }
        }

        pub(super) fn update_local_device(&self, device: LocalDevice) -> Result<(), String> {
            *self.local.write() = device;
            if bridge_slot().lock().is_some() {
                invoke_start(self)?;
            }
            Ok(())
        }
    }

    pub(super) fn start(
        app: AppHandle,
        local: LocalDevice,
        peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    ) -> PeerToPeerHandle {
        let shared = Arc::new(Shared {
            app,
            local: RwLock::new(local),
            peers,
            routes: RwLock::new(HashMap::new()),
            errors: Mutex::new(HashMap::new()),
            changed: Notify::new(),
        });
        let _ = SHARED.set(Arc::clone(&shared));
        if bridge_slot().lock().is_some() {
            if let Err(error) = invoke_start(&shared) {
                let _ = shared.app.emit("network-error", error);
            }
        }
        PeerToPeerHandle {
            shared: Some(shared),
        }
    }

    fn invoke_start(shared: &Shared) -> Result<(), String> {
        invoke_one_string("startWifiDirect", &shared.configuration_json()?)
    }

    fn invoke_connect(peer_id: &str, device_address: &str) -> Result<(), String> {
        let bridge_guard = bridge_slot().lock();
        let bridge = bridge_guard
            .as_ref()
            .ok_or_else(|| "Android 点对点 Wi-Fi 适配层尚未就绪".to_string())?;
        let mut env = bridge
            .vm
            .attach_current_thread()
            .map_err(|error| format!("无法连接 Android 运行时：{error}"))?;
        let peer_id = JObject::from(
            env.new_string(peer_id)
                .map_err(|error| format!("无法编码设备 ID：{error}"))?,
        );
        let device_address = JObject::from(
            env.new_string(device_address)
                .map_err(|error| format!("无法编码点对点设备地址：{error}"))?,
        );
        env.call_method(
            bridge.object.as_obj(),
            "connectWifiDirect",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&peer_id), JValue::Object(&device_address)],
        )
        .map_err(|error| format!("无法请求 Android 点对点 Wi-Fi 连接：{error}"))?;
        Ok(())
    }

    fn invoke_one_string(method: &str, value: &str) -> Result<(), String> {
        let bridge_guard = bridge_slot().lock();
        let bridge = bridge_guard
            .as_ref()
            .ok_or_else(|| "Android 点对点 Wi-Fi 适配层尚未就绪".to_string())?;
        let mut env = bridge
            .vm
            .attach_current_thread()
            .map_err(|error| format!("无法连接 Android 运行时：{error}"))?;
        let value = JObject::from(
            env.new_string(value)
                .map_err(|error| format!("无法编码点对点 Wi-Fi 配置：{error}"))?,
        );
        env.call_method(
            bridge.object.as_obj(),
            method,
            "(Ljava/lang/String;)V",
            &[JValue::Object(&value)],
        )
        .map_err(|error| format!("无法调用 Android 点对点 Wi-Fi 适配层：{error}"))?;
        Ok(())
    }

    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "system" fn Java_com_kure29_neloa_WifiDirectBridge_nativeRegister(
        env: JNIEnv,
        bridge: JObject,
    ) {
        let registration = env.get_java_vm().and_then(|vm| {
            env.new_global_ref(bridge)
                .map(|object| AndroidBridge { vm, object })
        });
        if let Ok(registration) = registration {
            *bridge_slot().lock() = Some(registration);
            if let Some(shared) = SHARED.get() {
                if let Err(error) = invoke_start(shared) {
                    let _ = shared.app.emit("network-error", error);
                }
            }
        }
    }

    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "system" fn Java_com_kure29_neloa_WifiDirectBridge_nativeEvent(
        mut env: JNIEnv,
        _bridge: JObject,
        event: JString,
    ) {
        let Some(shared) = SHARED.get() else {
            return;
        };
        let result = env
            .get_string(&event)
            .map(|value| value.to_string_lossy().into_owned())
            .map_err(|error| error.to_string())
            .and_then(|json| {
                serde_json::from_str::<AndroidEvent>(&json).map_err(|error| error.to_string())
            });
        match result {
            Ok(event) => shared.handle_event(event),
            Err(error) => {
                let _ = shared.app.emit(
                    "network-error",
                    format!("无法读取 Android 点对点 Wi-Fi 事件：{error}"),
                );
            }
        }
    }
}
