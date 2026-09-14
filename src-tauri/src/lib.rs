use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(not(target_os = "ios"))]
use std::thread;
#[cfg(target_os = "ios")]
use std::{
    ffi::{CStr, CString},
    sync::OnceLock,
};

#[cfg(not(target_os = "ios"))]
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use parking_lot::RwLock;
use percent_encoding::percent_decode_str;
use tauri::{Emitter, Manager, WebviewWindow};
use tauri_plugin_fs::FilePath;
#[cfg(target_os = "android")]
use tauri_plugin_fs::{FsExt, OpenOptions};
use uuid::Uuid;

mod clipboard;
mod identity;
mod model;
mod network;
mod trust;

use clipboard::ClipboardService;
use identity::NoiseIdentity;
use model::{
    ClipboardSnapshot, DiagnosticCheck, DiagnosticPeer, DiagnosticsSnapshot, DiscoverySnapshot,
    LocalDevice, PairingRequest, PeerDevice, SecuritySnapshot, SelectedFile, TestMessageEvent,
    CAPABILITIES, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
};
use network::NetworkHandle;
use trust::TrustStore;

#[cfg(not(target_os = "ios"))]
const SERVICE_TYPE: &str = "_neloa._udp.local.";
const SERVICE_PORT: u16 = 48_631;
#[cfg(not(mobile))]
const LEGACY_IDENTIFIER: &str = "app.neloa.desktop";
#[cfg(not(mobile))]
const PERSISTENT_DATA_FILES: [&str; 3] = ["device-id", "trusted-devices.json", "settings.json"];

struct DiscoveryState {
    #[cfg(not(target_os = "ios"))]
    daemon: Option<ServiceDaemon>,
    peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    error: Arc<RwLock<Option<String>>>,
    advertising: Arc<AtomicBool>,
    browsing: Arc<AtomicBool>,
}

impl DiscoveryState {
    fn is_active(&self) -> bool {
        #[cfg(not(target_os = "ios"))]
        let backend_ready = self.daemon.is_some();
        #[cfg(target_os = "ios")]
        let backend_ready = true;

        backend_ready
            && self.advertising.load(Ordering::Relaxed)
            && self.browsing.load(Ordering::Relaxed)
            && self.error.read().is_none()
    }
}

struct AppState {
    local: LocalDevice,
    discovery: DiscoveryState,
    network: NetworkHandle,
    trust: TrustStore,
    clipboard: ClipboardService,
}

struct PreparedTransferSource {
    path: PathBuf,
    name: String,
    size: u64,
    cleanup_source: bool,
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(not(mobile))]
fn migrate_legacy_data_files(legacy_dir: &Path, current_dir: &Path) -> Result<(), String> {
    if legacy_dir == current_dir || !legacy_dir.is_dir() {
        return Ok(());
    }

    let files_to_copy = PERSISTENT_DATA_FILES
        .iter()
        .filter(|name| {
            let source = legacy_dir.join(name);
            let destination = current_dir.join(name);
            source.is_file() && !destination.exists()
        })
        .collect::<Vec<_>>();
    if files_to_copy.is_empty() {
        return Ok(());
    }

    fs::create_dir_all(current_dir)
        .map_err(|error| format!("无法创建新版应用数据目录：{error}"))?;
    for name in files_to_copy {
        let source = legacy_dir.join(name);
        let destination = current_dir.join(name);
        fs::copy(&source, &destination)
            .map_err(|error| format!("无法迁移旧版数据文件 {}：{error}", source.display()))?;
    }
    Ok(())
}

#[cfg(not(mobile))]
fn migrate_legacy_app_data(app: &tauri::AppHandle) -> Result<(), String> {
    let legacy_dir = app
        .path()
        .data_dir()
        .map_err(|error| format!("无法定位旧版应用数据目录：{error}"))?
        .join(LEGACY_IDENTIFIER);
    let current_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位新版应用数据目录：{error}"))?;
    migrate_legacy_data_files(&legacy_dir, &current_dir)
}

fn platform_name() -> String {
    if cfg!(target_os = "windows") {
        "windows".into()
    } else if cfg!(target_os = "macos") {
        "macos".into()
    } else if cfg!(target_os = "ios") {
        "ios".into()
    } else if cfg!(target_os = "android") {
        "android".into()
    } else {
        "linux".into()
    }
}

#[cfg(not(target_os = "ios"))]
fn safe_host_label(name: &str) -> String {
    let label: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = label.trim_matches('-');
    if trimmed.is_empty() {
        "neloa-device".into()
    } else {
        trimmed.chars().take(50).collect()
    }
}

fn load_or_create_device_id(app: &tauri::AppHandle) -> String {
    let app_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let id_path = app_dir.join("device-id");

    if let Ok(existing) = fs::read_to_string(&id_path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let id = Uuid::new_v4().to_string();
    if fs::create_dir_all(&app_dir).is_ok() {
        let _ = fs::write(id_path, &id);
    }
    id
}

fn local_device(app: &tauri::AppHandle) -> LocalDevice {
    #[cfg(target_os = "ios")]
    let hostname = "iPhone".to_string();
    #[cfg(not(target_os = "ios"))]
    let hostname = hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Neloa Device".into());

    LocalDevice {
        id: load_or_create_device_id(app),
        name: hostname,
        platform: platform_name(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

fn snapshot_from(
    peers: &Arc<RwLock<HashMap<String, PeerDevice>>>,
    error: &Arc<RwLock<Option<String>>>,
    active: bool,
) -> DiscoverySnapshot {
    let mut peers: Vec<_> = peers.read().values().cloned().collect();
    peers.sort_by_key(|peer| peer.name.to_lowercase());
    DiscoverySnapshot {
        active,
        error: error.read().clone(),
        peers,
    }
}

#[cfg(not(target_os = "ios"))]
fn start_discovery(app: tauri::AppHandle, local: &LocalDevice) -> DiscoveryState {
    let peers = Arc::new(RwLock::new(HashMap::new()));
    let error = Arc::new(RwLock::new(None));
    let advertising_state = Arc::new(AtomicBool::new(false));
    let browsing_state = Arc::new(AtomicBool::new(false));

    let daemon = match ServiceDaemon::new() {
        Ok(daemon) => daemon,
        Err(problem) => {
            *error.write() = Some(format!("无法启动 mDNS：{problem}"));
            return DiscoveryState {
                daemon: None,
                peers,
                error,
                advertising: advertising_state,
                browsing: browsing_state,
            };
        }
    };

    let short_id = local.id.chars().take(8).collect::<String>();
    let host_label = safe_host_label(&local.name);
    let host_name = format!("{host_label}.local.");
    let instance_name = format!("Neloa-{short_id}");
    let protocol_version = PROTOCOL_VERSION.to_string();
    let min_protocol_version = MIN_PROTOCOL_VERSION.to_string();
    let capabilities = CAPABILITIES.join(",");
    let properties = [
        ("id", local.id.as_str()),
        ("name", local.name.as_str()),
        ("platform", local.platform.as_str()),
        ("version", local.version.as_str()),
        ("protocolVersion", protocol_version.as_str()),
        ("minProtocolVersion", min_protocol_version.as_str()),
        ("capabilities", capabilities.as_str()),
    ];

    let service = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &host_name,
        "",
        SERVICE_PORT,
        &properties[..],
    )
    .map(ServiceInfo::enable_addr_auto);

    let advertising = match service.and_then(|service| daemon.register(service)) {
        Ok(()) => true,
        Err(problem) => {
            *error.write() = Some(format!("无法广播本机设备：{problem}"));
            false
        }
    };
    advertising_state.store(advertising, Ordering::Relaxed);

    let browsing = match daemon.browse(SERVICE_TYPE) {
        Ok(receiver) => {
            let peers_for_thread = Arc::clone(&peers);
            let error_for_thread = Arc::clone(&error);
            let app_for_thread = app.clone();
            let local_id = local.id.clone();
            let fully_active = advertising;
            thread::spawn(move || {
                while let Ok(event) = receiver.recv() {
                    let changed = match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let id = info
                                .get_property_val_str("id")
                                .unwrap_or_default()
                                .to_string();
                            if id.is_empty() || id == local_id {
                                false
                            } else {
                                let peer = PeerDevice {
                                    id: id.clone(),
                                    name: info
                                        .get_property_val_str("name")
                                        .unwrap_or("Neloa Device")
                                        .to_string(),
                                    platform: info
                                        .get_property_val_str("platform")
                                        .unwrap_or("unknown")
                                        .to_string(),
                                    version: info
                                        .get_property_val_str("version")
                                        .unwrap_or("unknown")
                                        .to_string(),
                                    protocol_version: info
                                        .get_property_val_str("protocolVersion")
                                        .and_then(|value| value.parse().ok())
                                        .unwrap_or_default(),
                                    min_protocol_version: info
                                        .get_property_val_str("minProtocolVersion")
                                        .and_then(|value| value.parse().ok())
                                        .unwrap_or_default(),
                                    capabilities: info
                                        .get_property_val_str("capabilities")
                                        .unwrap_or_default()
                                        .split(',')
                                        .map(str::trim)
                                        .filter(|value| !value.is_empty())
                                        .map(str::to_string)
                                        .collect(),
                                    addresses: info
                                        .get_addresses_v4()
                                        .iter()
                                        .map(ToString::to_string)
                                        .collect(),
                                    port: info.get_port(),
                                    last_seen_ms: unix_millis(),
                                    service_fullname: info.get_fullname().to_string(),
                                };
                                peers_for_thread.write().insert(id, peer);
                                true
                            }
                        }
                        ServiceEvent::ServiceRemoved(_, fullname) => {
                            let before = peers_for_thread.read().len();
                            peers_for_thread
                                .write()
                                .retain(|_, peer| peer.service_fullname != fullname);
                            peers_for_thread.read().len() != before
                        }
                        _ => false,
                    };

                    if changed {
                        let snapshot =
                            snapshot_from(&peers_for_thread, &error_for_thread, fully_active);
                        let _ = app_for_thread.emit("peers-changed", snapshot);
                    }
                }
            });
            true
        }
        Err(problem) => {
            *error.write() = Some(format!("无法扫描局域网：{problem}"));
            false
        }
    };
    browsing_state.store(browsing, Ordering::Relaxed);

    DiscoveryState {
        daemon: Some(daemon),
        peers,
        error,
        advertising: advertising_state,
        browsing: browsing_state,
    }
}

#[cfg(target_os = "ios")]
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct IosDiscoveryConfig<'a> {
    service_type: &'a str,
    port: u16,
    id: &'a str,
    name: &'a str,
    platform: &'a str,
    version: &'a str,
    protocol_version: u16,
    min_protocol_version: u16,
    capabilities: String,
}

#[cfg(target_os = "ios")]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct IosDiscoveredPeer {
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
    addresses: Vec<String>,
    port: u16,
    service_fullname: String,
}

#[cfg(target_os = "ios")]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct IosDiscoveryStatus {
    advertising: bool,
    browsing: bool,
    error: Option<String>,
}

#[cfg(target_os = "ios")]
struct IosDiscoveryContext {
    app: tauri::AppHandle,
    local_id: String,
    peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    error: Arc<RwLock<Option<String>>>,
    advertising: Arc<AtomicBool>,
    browsing: Arc<AtomicBool>,
}

#[cfg(target_os = "ios")]
static IOS_DISCOVERY: OnceLock<IosDiscoveryContext> = OnceLock::new();

#[cfg(target_os = "ios")]
type IosDiscoveryStart = unsafe extern "C" fn(*const std::ffi::c_char) -> i32;

#[cfg(target_os = "ios")]
static IOS_DISCOVERY_START: OnceLock<IosDiscoveryStart> = OnceLock::new();

#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn neloa_ios_discovery_register_start(start: IosDiscoveryStart) {
    let _ = IOS_DISCOVERY_START.set(start);
}

#[cfg(target_os = "ios")]
fn ios_discovery_start(config_json: &CString) -> Result<(), String> {
    let start = IOS_DISCOVERY_START
        .get()
        .ok_or_else(|| "iOS Bonjour 适配层尚未注册".to_string())?;
    let result = unsafe { start(config_json.as_ptr()) };
    (result == 0)
        .then_some(())
        .ok_or_else(|| "无法启动 iOS Bonjour 适配层".to_string())
}

#[cfg(target_os = "ios")]
fn ios_callback_json(pointer: *const std::ffi::c_char) -> Result<String, String> {
    if pointer.is_null() {
        return Err("iOS Bonjour 回调返回了空数据".into());
    }
    // Swift keeps the temporary C string alive until this callback returns.
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map(str::to_owned)
        .map_err(|error| format!("iOS Bonjour 回调不是有效 UTF-8：{error}"))
}

#[cfg(target_os = "ios")]
fn emit_ios_discovery_snapshot(context: &IosDiscoveryContext) {
    let active = context.advertising.load(Ordering::Relaxed)
        && context.browsing.load(Ordering::Relaxed)
        && context.error.read().is_none();
    let snapshot = snapshot_from(&context.peers, &context.error, active);
    let _ = context.app.emit("peers-changed", snapshot);
}

#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn neloa_ios_discovery_peer_upsert(peer_json: *const std::ffi::c_char) {
    let Some(context) = IOS_DISCOVERY.get() else {
        return;
    };
    let result = ios_callback_json(peer_json).and_then(|json| {
        serde_json::from_str::<IosDiscoveredPeer>(&json).map_err(|error| error.to_string())
    });
    match result {
        Ok(peer) if !peer.id.is_empty() && peer.id != context.local_id => {
            let peer = PeerDevice {
                id: peer.id.clone(),
                name: peer.name,
                platform: peer.platform,
                version: peer.version,
                protocol_version: peer.protocol_version,
                min_protocol_version: peer.min_protocol_version,
                capabilities: peer.capabilities,
                addresses: peer.addresses,
                port: peer.port,
                last_seen_ms: unix_millis(),
                service_fullname: peer.service_fullname,
            };
            context.peers.write().insert(peer.id.clone(), peer);
            emit_ios_discovery_snapshot(context);
        }
        Ok(_) => {}
        Err(problem) => {
            *context.error.write() = Some(format!("无法解析 Bonjour 设备信息：{problem}"));
            emit_ios_discovery_snapshot(context);
        }
    }
}

#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn neloa_ios_discovery_peer_remove(fullname: *const std::ffi::c_char) {
    let Some(context) = IOS_DISCOVERY.get() else {
        return;
    };
    let Ok(fullname) = ios_callback_json(fullname) else {
        return;
    };
    let before = context.peers.read().len();
    context
        .peers
        .write()
        .retain(|_, peer| peer.service_fullname != fullname);
    if context.peers.read().len() != before {
        emit_ios_discovery_snapshot(context);
    }
}

#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn neloa_ios_discovery_status(status_json: *const std::ffi::c_char) {
    let Some(context) = IOS_DISCOVERY.get() else {
        return;
    };
    let result = ios_callback_json(status_json).and_then(|json| {
        serde_json::from_str::<IosDiscoveryStatus>(&json).map_err(|error| error.to_string())
    });
    match result {
        Ok(status) => {
            context
                .advertising
                .store(status.advertising, Ordering::Relaxed);
            context.browsing.store(status.browsing, Ordering::Relaxed);
            *context.error.write() = status.error;
        }
        Err(problem) => {
            *context.error.write() = Some(format!("无法读取 Bonjour 状态：{problem}"));
        }
    }
    emit_ios_discovery_snapshot(context);
}

#[cfg(target_os = "ios")]
fn start_discovery(app: tauri::AppHandle, local: &LocalDevice) -> DiscoveryState {
    let peers = Arc::new(RwLock::new(HashMap::new()));
    let error = Arc::new(RwLock::new(None));
    let advertising = Arc::new(AtomicBool::new(false));
    let browsing = Arc::new(AtomicBool::new(false));
    let config = IosDiscoveryConfig {
        service_type: "_neloa._udp",
        port: SERVICE_PORT,
        id: &local.id,
        name: &local.name,
        platform: &local.platform,
        version: &local.version,
        protocol_version: PROTOCOL_VERSION,
        min_protocol_version: MIN_PROTOCOL_VERSION,
        capabilities: CAPABILITIES.join(","),
    };

    let state = DiscoveryState {
        peers: Arc::clone(&peers),
        error: Arc::clone(&error),
        advertising: Arc::clone(&advertising),
        browsing: Arc::clone(&browsing),
    };
    if IOS_DISCOVERY
        .set(IosDiscoveryContext {
            app,
            local_id: local.id.clone(),
            peers,
            error: Arc::clone(&error),
            advertising,
            browsing,
        })
        .is_err()
    {
        *error.write() = Some("iOS Bonjour 发现服务已经启动".into());
        return state;
    }

    let start_result = serde_json::to_string(&config)
        .map_err(|problem| format!("无法生成 Bonjour 配置：{problem}"))
        .and_then(|json| CString::new(json).map_err(|problem| problem.to_string()))
        .and_then(|json| ios_discovery_start(&json));
    if let Err(problem) = start_result {
        *error.write() = Some(problem);
    }
    state
}

#[tauri::command]
fn get_local_device(state: tauri::State<'_, AppState>) -> LocalDevice {
    state.local.clone()
}

#[tauri::command]
fn get_discovery_snapshot(state: tauri::State<'_, AppState>) -> DiscoverySnapshot {
    snapshot_from(
        &state.discovery.peers,
        &state.discovery.error,
        state.discovery.is_active(),
    )
}

#[tauri::command]
fn get_security_snapshot(state: tauri::State<'_, AppState>) -> SecuritySnapshot {
    SecuritySnapshot {
        network: state.network.status(),
        trusted_devices: state.trust.list(),
    }
}

#[tauri::command]
fn get_clipboard_snapshot(state: tauri::State<'_, AppState>) -> ClipboardSnapshot {
    state.clipboard.snapshot()
}

fn diagnostics_snapshot(state: &AppState) -> DiagnosticsSnapshot {
    let network = state.network.status();
    let discovery_error = state.discovery.error.read().clone();
    let clipboard = state.clipboard.snapshot();
    let mut peers: Vec<_> = state
        .discovery
        .peers
        .read()
        .values()
        .map(|peer| DiagnosticPeer {
            id_prefix: peer.id.chars().take(8).collect(),
            name: peer.name.clone(),
            platform: peer.platform.clone(),
            app_version: peer.version.clone(),
            protocol_version: peer.protocol_version,
            min_protocol_version: peer.min_protocol_version,
            compatible: peer.is_protocol_compatible(),
            capabilities: peer.capabilities.clone(),
            last_seen_ms: peer.last_seen_ms,
        })
        .collect();
    peers.sort_by_key(|peer| peer.name.to_lowercase());
    let incompatible_peers = peers.iter().filter(|peer| !peer.compatible).count();
    let identity_ready = network.identity_fingerprint != "不可用";

    let network_check = if network.active {
        DiagnosticCheck {
            id: "network".into(),
            label: "加密传输端口".into(),
            state: "ok".into(),
            detail: format!("UDP {} 正在监听 QUIC 连接", network.port),
            guidance: None,
        }
    } else {
        DiagnosticCheck {
            id: "network".into(),
            label: "加密传输端口".into(),
            state: "error".into(),
            detail: network
                .error
                .clone()
                .unwrap_or_else(|| "网络服务尚未就绪".into()),
            guidance: Some("关闭占用端口的程序，或检查系统防火墙权限".into()),
        }
    };
    let discovery_check = if state.discovery.is_active() {
        DiagnosticCheck {
            id: "discovery".into(),
            label: if cfg!(target_os = "ios") {
                "Bonjour 自动发现"
            } else {
                "mDNS 自动发现"
            }
            .into(),
            state: "ok".into(),
            detail: format!("广播与扫描正常 · 发现 {} 台设备", peers.len()),
            guidance: None,
        }
    } else {
        DiagnosticCheck {
            id: "discovery".into(),
            label: if cfg!(target_os = "ios") {
                "Bonjour 自动发现"
            } else {
                "mDNS 自动发现"
            }
            .into(),
            state: "error".into(),
            detail: discovery_error.unwrap_or_else(|| "广播或扫描未能启动".into()),
            guidance: Some(
                if cfg!(target_os = "ios") {
                    "请在系统设置中允许 Neloa 访问本地网络，并确认两端位于同一局域网"
                } else {
                    "确认两端位于同一局域网，且未启用客户端隔离"
                }
                .into(),
            ),
        }
    };
    let identity_check = DiagnosticCheck {
        id: "identity".into(),
        label: "设备加密身份".into(),
        state: if identity_ready { "ok" } else { "error" }.into(),
        detail: if identity_ready {
            format!("系统凭据库密钥可用 · 指纹 {}", network.identity_fingerprint)
        } else {
            network
                .error
                .clone()
                .unwrap_or_else(|| "无法载入设备密钥".into())
        },
        guidance: (!identity_ready).then(|| "检查钥匙串或 Windows 凭据管理器访问权限".into()),
    };
    let protocol_check = DiagnosticCheck {
        id: "protocol".into(),
        label: "协议兼容性".into(),
        state: if incompatible_peers == 0 {
            "ok"
        } else {
            "warning"
        }
        .into(),
        detail: if incompatible_peers == 0 {
            format!("本机支持协议 v{MIN_PROTOCOL_VERSION}–v{PROTOCOL_VERSION}")
        } else {
            format!("{incompatible_peers} 台设备版本不兼容，传输已被阻止")
        },
        guidance: (incompatible_peers > 0).then(|| "请将两端 Neloa 更新到兼容版本".into()),
    };
    let clipboard_check = DiagnosticCheck {
        id: "clipboard".into(),
        label: "剪贴板守护服务".into(),
        state: if clipboard.enabled { "ok" } else { "idle" }.into(),
        detail: if clipboard.enabled {
            format!("已监听新复制的纯文本 · 上限 {} 字节", clipboard.max_bytes)
        } else {
            "服务已加载 · 自动同步当前关闭".into()
        },
        guidance: None,
    };

    let firewall_guidance = if state.local.platform == "windows" {
        "在 Windows Defender 防火墙中允许 Neloa 访问“专用网络”；局域网传输使用 UDP 48631，mDNS 使用 UDP 5353。"
    } else if state.local.platform == "ios" {
        "请在系统设置中允许 Neloa 访问本地网络；文件传输使用 UDP 48631，设备发现使用系统 Bonjour。"
    } else {
        "若 macOS 弹出网络访问提示，请允许 Neloa 接收入站连接；局域网传输使用 UDP 48631，mDNS 使用 UDP 5353。"
    };

    DiagnosticsSnapshot {
        generated_at_ms: unix_millis(),
        app_version: state.local.version.clone(),
        platform: state.local.platform.clone(),
        protocol_version: PROTOCOL_VERSION,
        min_protocol_version: MIN_PROTOCOL_VERSION,
        device_id_prefix: state.local.id.chars().take(8).collect(),
        checks: vec![
            network_check,
            discovery_check,
            identity_check,
            protocol_check,
            clipboard_check,
        ],
        peers,
        firewall_guidance: firewall_guidance.into(),
        report_privacy: "报告不包含 IP、完整设备 ID、公钥、文件路径或剪贴板正文".into(),
    }
}

fn diagnostic_report(state: &AppState, snapshot: &DiagnosticsSnapshot) -> String {
    let network = state.network.status();
    let discovery_error = state.discovery.error.read().is_some();
    let clipboard = state.clipboard.snapshot();
    let peer_lines = if snapshot.peers.is_empty() {
        "peer.none=true".to_string()
    } else {
        snapshot
            .peers
            .iter()
            .enumerate()
            .map(|(index, peer)| {
                format!(
                    "peer.{}=platform:{} app:{} protocol:{}-{} compatible:{} capabilities:{}",
                    index + 1,
                    peer.platform,
                    peer.app_version,
                    peer.min_protocol_version,
                    peer.protocol_version,
                    peer.compatible,
                    peer.capabilities.join(",")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Neloa sanitized diagnostic report\n\
generatedAtMs={}\n\
appVersion={}\n\
platform={}\n\
deviceIdPrefix={}\n\
protocolRange={}-{}\n\
network.active={}\n\
network.udpPort={}\n\
network.errorPresent={}\n\
identity.available={}\n\
discovery.advertising={}\n\
discovery.browsing={}\n\
discovery.errorPresent={}\n\
discovery.peerCount={}\n\
trust.count={}\n\
clipboard.enabled={}\n\
clipboard.maxBytes={}\n\
{}\n\
privacy=No IP addresses, full device IDs, public keys, file paths, or clipboard content included.",
        snapshot.generated_at_ms,
        snapshot.app_version,
        snapshot.platform,
        snapshot.device_id_prefix,
        snapshot.min_protocol_version,
        snapshot.protocol_version,
        network.active,
        network.port,
        network.error.is_some(),
        network.identity_fingerprint != "不可用",
        state.discovery.advertising.load(Ordering::Relaxed),
        state.discovery.browsing.load(Ordering::Relaxed),
        discovery_error,
        snapshot.peers.len(),
        state.trust.list().len(),
        clipboard.enabled,
        clipboard.max_bytes,
        peer_lines,
    )
}

#[tauri::command]
fn get_diagnostics_snapshot(state: tauri::State<'_, AppState>) -> DiagnosticsSnapshot {
    diagnostics_snapshot(&state)
}

#[tauri::command]
async fn copy_diagnostic_report(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let snapshot = diagnostics_snapshot(&state);
    let report = diagnostic_report(&state, &snapshot);
    state.clipboard.write_local(report).await?;
    Ok("脱敏诊断报告已复制到系统剪贴板".into())
}

#[tauri::command(rename_all = "camelCase")]
fn set_clipboard_enabled(
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<ClipboardSnapshot, String> {
    state.clipboard.set_enabled(enabled)
}

fn find_peer(state: &AppState, peer_id: &str) -> Result<PeerDevice, String> {
    let peer = state
        .discovery
        .peers
        .read()
        .get(peer_id)
        .cloned()
        .ok_or_else(|| "目标设备已离线，请重新扫描".to_string())?;
    if !peer.is_protocol_compatible() {
        return Err(format!(
            "{} 的协议版本不兼容（本机 v{}–v{}，对端 v{}–v{}），请先更新另一端 Neloa",
            peer.name,
            MIN_PROTOCOL_VERSION,
            PROTOCOL_VERSION,
            peer.min_protocol_version,
            peer.protocol_version
        ));
    }
    Ok(peer)
}

#[tauri::command(rename_all = "camelCase")]
async fn begin_pairing(
    peer_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<PairingRequest, String> {
    let peer = find_peer(&state, &peer_id)?;
    state.network.begin_pairing(peer).await
}

#[tauri::command(rename_all = "camelCase")]
async fn decide_pairing(
    session_id: String,
    accepted: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.network.confirm_pairing(session_id, accepted).await
}

#[tauri::command(rename_all = "camelCase")]
async fn send_test_message(
    peer_id: String,
    text: String,
    state: tauri::State<'_, AppState>,
) -> Result<TestMessageEvent, String> {
    let peer = find_peer(&state, &peer_id)?;
    state.network.send_test_message(peer, text).await
}

#[tauri::command(rename_all = "camelCase")]
async fn start_file_transfer(
    app: tauri::AppHandle,
    peer_id: String,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let peer = find_peer(&state, &peer_id)?;
    let network = state.network.clone();
    let app_for_prepare = app.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        prepare_transfer_source(&app_for_prepare, &path)
    })
    .await
    .map_err(|error| format!("准备待发送文件失败：{error}"))??;

    let result = network.start_file_transfer(
        peer,
        prepared.path.clone(),
        prepared.name,
        prepared.size,
        prepared.cleanup_source,
    );
    if result.is_err() && prepared.cleanup_source {
        let _ = fs::remove_file(prepared.path);
    }
    result
}

#[tauri::command(rename_all = "camelCase")]
async fn decide_file_offer(
    transfer_id: String,
    accepted: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.network.decide_file_offer(transfer_id, accepted).await
}

#[tauri::command(rename_all = "camelCase")]
async fn cancel_file_transfer(
    transfer_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    state.network.cancel_file_transfer(transfer_id).await
}

#[tauri::command(rename_all = "camelCase")]
fn revoke_trusted_device(
    app: tauri::AppHandle,
    peer_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let removed = state.trust.remove(&peer_id)?;
    if removed {
        app.emit("trusted-devices-changed", state.trust.list())
            .map_err(|error| format!("无法刷新可信设备列表：{error}"))?;
    }
    Ok(removed)
}

fn safe_selected_file_name(file_path: &FilePath) -> String {
    let candidate: String = match file_path {
        FilePath::Path(path) => path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default(),
        FilePath::Url(url) => url
            .to_file_path()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|value| value.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| {
                let encoded = url
                    .path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .unwrap_or_default();
                percent_decode_str(encoded).decode_utf8_lossy().into_owned()
            }),
    };
    let leaf = candidate
        .rsplit(['/', '\\', ':'])
        .find(|part| !part.trim().is_empty())
        .unwrap_or("未命名文件");
    let cleaned: String = leaf
        .chars()
        .filter(|character| !character.is_control() && *character != '/' && *character != '\\')
        .take(160)
        .collect();
    if cleaned.trim().is_empty() {
        "未命名文件".to_string()
    } else {
        cleaned
    }
}

fn local_file_path(file_path: FilePath) -> Result<PathBuf, String> {
    match file_path {
        FilePath::Path(path) => Ok(path),
        FilePath::Url(url) if url.scheme() == "file" => url
            .to_file_path()
            .map_err(|_| "系统返回的文件地址无效".to_string()),
        FilePath::Url(url) => Err(format!("当前平台无法直接读取 {} 文件地址", url.scheme())),
    }
}

fn inspect_selected_file(app: &tauri::AppHandle, path: &str) -> Result<SelectedFile, String> {
    let file_path: FilePath = path.parse().expect("FilePath parsing is infallible");
    let name = safe_selected_file_name(&file_path);

    #[cfg(target_os = "android")]
    if matches!(&file_path, FilePath::Url(url) if url.scheme() == "content") {
        let mut options = OpenOptions::new();
        options.read(true);
        let file = app
            .fs()
            .open(file_path, options)
            .map_err(|error| format!("无法读取 Android 选中的文件：{error}"))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("无法读取 Android 文件信息：{error}"))?;
        return Ok(SelectedFile {
            path: path.to_string(),
            name,
            size: metadata.len(),
        });
    }

    #[cfg(not(target_os = "android"))]
    let _ = app;
    let local_path = local_file_path(file_path)?;
    let metadata =
        fs::metadata(&local_path).map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("当前阶段仅支持选择单个文件".into());
    }
    Ok(SelectedFile {
        path: path.to_string(),
        name,
        size: metadata.len(),
    })
}

#[cfg(target_os = "android")]
fn stage_android_content_uri(
    app: &tauri::AppHandle,
    file_path: FilePath,
    name: String,
) -> Result<PreparedTransferSource, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    let mut source = app
        .fs()
        .open(file_path, options)
        .map_err(|error| format!("无法打开 Android 选中的文件：{error}"))?;
    let outbox = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("无法定位应用缓存目录：{error}"))?
        .join("outbox");
    fs::create_dir_all(&outbox).map_err(|error| format!("无法创建发送缓存：{error}"))?;
    let staged_path = outbox.join(format!("{}-{name}", Uuid::new_v4()));
    let mut staged = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_path)
        .map_err(|error| format!("无法创建发送缓存文件：{error}"))?;
    let copy_result =
        std::io::copy(&mut source, &mut staged).and_then(|size| staged.sync_all().map(|_| size));
    let size = match copy_result {
        Ok(size) => size,
        Err(error) => {
            let _ = fs::remove_file(&staged_path);
            return Err(format!("无法暂存 Android 文件：{error}"));
        }
    };
    Ok(PreparedTransferSource {
        path: staged_path,
        name,
        size,
        cleanup_source: true,
    })
}

fn prepare_transfer_source(
    app: &tauri::AppHandle,
    path: &str,
) -> Result<PreparedTransferSource, String> {
    let file_path: FilePath = path.parse().expect("FilePath parsing is infallible");
    let name = safe_selected_file_name(&file_path);

    #[cfg(target_os = "android")]
    if matches!(&file_path, FilePath::Url(url) if url.scheme() == "content") {
        return stage_android_content_uri(app, file_path, name);
    }

    #[cfg(not(target_os = "android"))]
    let _ = app;
    let source = local_file_path(file_path)?;
    let metadata =
        fs::metadata(&source).map_err(|error| format!("无法读取待发送文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("只能发送单个普通文件".to_string());
    }
    Ok(PreparedTransferSource {
        path: source,
        name,
        size: metadata.len(),
        cleanup_source: cfg!(target_os = "ios"),
    })
}

#[tauri::command]
fn inspect_file(app: tauri::AppHandle, path: String) -> Result<SelectedFile, String> {
    inspect_selected_file(&app, &path)
}

/// Set once a tray icon is actually live. The Windows and Linux close button
/// hides the window so discovery keeps running in the background; without a
/// tray that window is unreachable and the process can only be killed from the
/// task manager, so hiding is only offered when there is a way back.
#[cfg(all(desktop, not(target_os = "macos")))]
static TRAY_READY: AtomicBool = AtomicBool::new(false);

#[cfg(all(desktop, not(target_os = "macos")))]
fn reveal_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Left click reveals the window; the menu is the explicit way out, since a
/// hidden window leaves no other affordance to quit.
#[cfg(all(desktop, not(target_os = "macos")))]
fn install_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    let show = MenuItem::with_id(app, "show", "显示 Neloa", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 Neloa", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &PredefinedMenuItem::separator(app)?, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Neloa")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => reveal_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.build(app)?;
    Ok(())
}

/// Hiding the window is only recoverable while a tray icon is live. macOS has
/// no tray here and no close button that hides, so it minimises instead.
#[cfg(desktop)]
fn hiding_is_recoverable() -> bool {
    #[cfg(not(target_os = "macos"))]
    {
        TRAY_READY.load(Ordering::Relaxed)
    }
    #[cfg(target_os = "macos")]
    {
        false
    }
}

#[tauri::command]
#[cfg(desktop)]
fn window_action(window: WebviewWindow, action: &str) -> Result<(), String> {
    let result = match action {
        "minimize" => window.minimize(),
        "maximize" => {
            let is_maximized = window
                .is_maximized()
                .map_err(|problem| problem.to_string())?;
            if is_maximized {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        "hide" => {
            if hiding_is_recoverable() {
                window.hide()
            } else {
                window.minimize()
            }
        }
        "close" => window.close(),
        _ => return Err(format!("未知窗口操作：{action}")),
    };
    result.map_err(|problem| problem.to_string())
}

#[tauri::command]
#[cfg(mobile)]
fn window_action(_window: WebviewWindow, _action: &str) -> Result<(), String> {
    Err("移动端不支持桌面窗口操作".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            #[cfg(not(mobile))]
            migrate_legacy_app_data(app.handle())?;

            let local = local_device(app.handle());
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| format!("无法定位应用数据目录：{error}"))?;
            let trust = TrustStore::load(app_data_dir.join("trusted-devices.json"))?;
            let clipboard =
                ClipboardService::load(app.handle().clone(), app_data_dir.join("settings.json"));
            let network = match NoiseIdentity::load_or_create() {
                Ok(identity) => NetworkHandle::start(
                    app.handle().clone(),
                    local.clone(),
                    identity,
                    trust.clone(),
                    clipboard.clone(),
                    SERVICE_PORT,
                ),
                Err(error) => NetworkHandle::unavailable(error, SERVICE_PORT),
            };
            let discovery = start_discovery(app.handle().clone(), &local);
            clipboard.configure(network.clone(), Arc::clone(&discovery.peers), trust.clone())?;

            // Non-fatal: without it the close button falls back to minimising,
            // which is worse but still recoverable.
            #[cfg(all(desktop, not(target_os = "macos")))]
            match install_tray(app.handle()) {
                Ok(()) => TRAY_READY.store(true, Ordering::Relaxed),
                Err(error) => {
                    eprintln!("托盘图标不可用，关闭按钮将改为最小化：{error}");
                }
            }

            app.manage(AppState {
                local,
                discovery,
                network,
                trust,
                clipboard,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_local_device,
            get_discovery_snapshot,
            get_security_snapshot,
            get_clipboard_snapshot,
            set_clipboard_enabled,
            get_diagnostics_snapshot,
            copy_diagnostic_report,
            begin_pairing,
            decide_pairing,
            send_test_message,
            start_file_transfer,
            decide_file_offer,
            cancel_file_transfer,
            revoke_trusted_device,
            inspect_file,
            window_action,
        ])
        .run(tauri::generate_context!())
        .expect("Neloa failed to start");
}

#[cfg(all(test, not(mobile)))]
mod app_data_migration_tests {
    use super::migrate_legacy_data_files;
    use std::fs;

    #[test]
    fn copies_only_persistent_files_from_legacy_directory() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("legacy");
        let current = root.path().join("current");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("device-id"), "device-123").unwrap();
        fs::write(legacy.join("trusted-devices.json"), "[]").unwrap();
        fs::write(legacy.join("settings.json"), "{}").unwrap();
        fs::write(legacy.join("temporary-file"), "ignore me").unwrap();

        migrate_legacy_data_files(&legacy, &current).unwrap();

        assert_eq!(
            fs::read_to_string(current.join("device-id")).unwrap(),
            "device-123"
        );
        assert_eq!(
            fs::read_to_string(current.join("trusted-devices.json")).unwrap(),
            "[]"
        );
        assert_eq!(
            fs::read_to_string(current.join("settings.json")).unwrap(),
            "{}"
        );
        assert!(!current.join("temporary-file").exists());
    }

    #[test]
    fn never_overwrites_current_data() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("legacy");
        let current = root.path().join("current");
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::write(legacy.join("device-id"), "old-device").unwrap();
        fs::write(current.join("device-id"), "current-device").unwrap();

        migrate_legacy_data_files(&legacy, &current).unwrap();

        assert_eq!(
            fs::read_to_string(current.join("device-id")).unwrap(),
            "current-device"
        );
    }
}
