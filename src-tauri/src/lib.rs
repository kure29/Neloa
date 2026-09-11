use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

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

const SERVICE_TYPE: &str = "_neloa._udp.local.";
const SERVICE_PORT: u16 = 48_631;

struct DiscoveryState {
    daemon: Option<ServiceDaemon>,
    peers: Arc<RwLock<HashMap<String, PeerDevice>>>,
    error: Arc<RwLock<Option<String>>>,
    advertising: bool,
    browsing: bool,
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

fn start_discovery(app: tauri::AppHandle, local: &LocalDevice) -> DiscoveryState {
    let peers = Arc::new(RwLock::new(HashMap::new()));
    let error = Arc::new(RwLock::new(None));

    let daemon = match ServiceDaemon::new() {
        Ok(daemon) => daemon,
        Err(problem) => {
            *error.write() = Some(format!("无法启动 mDNS：{problem}"));
            return DiscoveryState {
                daemon: None,
                peers,
                error,
                advertising: false,
                browsing: false,
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

    DiscoveryState {
        daemon: Some(daemon),
        peers,
        error,
        advertising,
        browsing,
    }
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
        state.discovery.daemon.is_some() && state.discovery.advertising && state.discovery.browsing,
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
    let discovery_check =
        if state.discovery.advertising && state.discovery.browsing && discovery_error.is_none() {
            DiagnosticCheck {
                id: "discovery".into(),
                label: "mDNS 自动发现".into(),
                state: "ok".into(),
                detail: format!("广播与扫描正常 · 发现 {} 台设备", peers.len()),
                guidance: None,
            }
        } else {
            DiagnosticCheck {
                id: "discovery".into(),
                label: "mDNS 自动发现".into(),
                state: "error".into(),
                detail: discovery_error.unwrap_or_else(|| "广播或扫描未能启动".into()),
                guidance: Some("确认两端位于同一局域网，且未启用客户端隔离".into()),
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
        state.discovery.advertising,
        state.discovery.browsing,
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
        "hide" => window.hide(),
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
