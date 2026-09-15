import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";

import type {
  ClipboardSnapshot,
  ClipboardSyncEvent,
  DiagnosticsSnapshot,
  DiscoverySnapshot,
  FileOffer,
  FileTransferProgress,
  FileTransferResult,
  LocalDevice,
  PairingRequest,
  PairingResult,
  Platform,
  RelaySnapshot,
  SecuritySnapshot,
  SelectedFile,
  Shell,
  TestMessageEvent,
  TrustedDevice,
} from "./types";

const query = new URLSearchParams(window.location.search);
const forcedPlatform = query.get("platform");

export const isDesktopRuntime = "__TAURI_INTERNALS__" in window;

let previewTrustedDevices: TrustedDevice[] = [];
let previewClipboardEnabled = false;
let previewDeviceName: string | null = null;
let previewRelay: RelaySnapshot = {
  enabled: false,
  url: "",
  hasToken: false,
  connected: false,
  onlineDevices: 0,
  error: null,
};
const previewPairings = new Map<string, PairingRequest>();
const previewTransferTimers = new Map<string, number[]>();
const previewTransferDetails = new Map<string, Omit<FileTransferResult, "status" | "message" | "sha256" | "atMs">>();

function emitPreview<T>(name: string, detail: T) {
  window.dispatchEvent(new CustomEvent(`neloa:${name}`, { detail }));
}

async function onAppEvent<T>(name: string, callback: (payload: T) => void): Promise<UnlistenFn> {
  if (isDesktopRuntime) {
    return listen<T>(name, (event) => callback(event.payload));
  }
  const listener = (event: Event) => callback((event as CustomEvent<T>).detail);
  window.addEventListener(`neloa:${name}`, listener);
  return () => window.removeEventListener(`neloa:${name}`, listener);
}

const DESKTOP_PLATFORMS: Platform[] = ["macos", "windows", "linux"];
const MOBILE_PLATFORMS: Platform[] = ["ios", "android"];
/** A touch device narrow enough that window chrome and hover affordances make no sense. */
const MOBILE_MEDIA = "(pointer: coarse) and (max-width: 819px)";
const MOBILE_USER_AGENT = /iPhone|iPod|iPad|Android/i;

function isPlatform(value: string | null): value is Platform {
  return DESKTOP_PLATFORMS.includes(value as Platform) || MOBILE_PLATFORMS.includes(value as Platform);
}

/**
 * The operating system, used for the window chrome and the system font stack.
 * `?platform=mobile` deliberately does not change this: it only forces the shell,
 * so the mobile layout can be previewed in a desktop browser.
 */
export function previewPlatform(): Platform {
  if (isPlatform(forcedPlatform)) return forcedPlatform;
  const agent = navigator.userAgent;
  if (/iPhone|iPod|iPad/i.test(agent)) return "ios";
  if (/Android/i.test(agent)) return "android";
  if (/Windows/i.test(agent)) return "windows";
  if (/Linux/i.test(agent)) return "linux";
  return "macos";
}

function forcedShell(): Shell | null {
  if (forcedPlatform === "mobile") return "mobile";
  if (MOBILE_PLATFORMS.includes(forcedPlatform as Platform)) return "mobile";
  if (DESKTOP_PLATFORMS.includes(forcedPlatform as Platform)) return "desktop";
  return null;
}

/** Which shell to render. Independent of {@link previewPlatform}. */
export function detectShell(): Shell {
  const forced = forcedShell();
  if (forced) return forced;
  if (MOBILE_USER_AGENT.test(navigator.userAgent)) return "mobile";
  return window.matchMedia(MOBILE_MEDIA).matches ? "mobile" : "desktop";
}

/** Re-evaluates the shell when the viewport changes, so browser previews stay live. */
export function onShellChange(callback: (shell: Shell) => void): () => void {
  if (forcedShell()) return () => {};
  const media = window.matchMedia(MOBILE_MEDIA);
  const handle = () => callback(detectShell());
  media.addEventListener("change", handle);
  return () => media.removeEventListener("change", handle);
}

function previewDevice(): LocalDevice {
  const platform = previewPlatform();
  const names: Record<Platform, string> = {
    macos: "Yuki 的 MacBook",
    windows: "Studio-PC",
    linux: "Neloa Workstation",
    ios: "Yuki 的 iPhone",
    android: "Pixel 8",
  };
  return {
    id: "preview-device",
    name: previewDeviceName ?? names[platform],
    platform,
    version: "0.1.11-preview",
  };
}

export async function getLocalDevice(): Promise<LocalDevice> {
  if (!isDesktopRuntime) return previewDevice();
  return invoke<LocalDevice>("get_local_device");
}

export async function setDeviceName(name: string): Promise<LocalDevice> {
  if (!isDesktopRuntime) {
    previewDeviceName = name.trim();
    const device = previewDevice();
    emitPreview("local-device-changed", device);
    return device;
  }
  return invoke<LocalDevice>("set_device_name", { name });
}

export const onLocalDeviceChanged = (callback: (device: LocalDevice) => void) =>
  onAppEvent("local-device-changed", callback);

export async function getDiscoverySnapshot(): Promise<DiscoverySnapshot> {
  if (!isDesktopRuntime) {
    const platform = previewPlatform();
    return {
      active: true,
      error: null,
      peers: [
        platform === "windows"
          ? {
              id: "preview-mac",
              name: "MacBook Pro M3",
              platform: "macos",
              version: "0.1.11",
              protocolVersion: 1,
              minProtocolVersion: 1,
              capabilities: ["discovery", "pairing", "noise-xx", "test-message", "file-transfer", "clipboard-text"],
              addresses: ["192.168.1.18"],
              port: 48631,
              lastSeenMs: Date.now(),
              relayAvailable: false,
            }
          : {
              id: "preview-windows",
              name: "Surface Laptop",
              platform: "windows",
              version: "0.1.11",
              protocolVersion: 1,
              minProtocolVersion: 1,
              capabilities: ["discovery", "pairing", "noise-xx", "test-message", "file-transfer", "clipboard-text"],
              addresses: ["192.168.1.23"],
              port: 48631,
              lastSeenMs: Date.now(),
              relayAvailable: false,
            },
      ],
    };
  }
  return invoke<DiscoverySnapshot>("get_discovery_snapshot");
}

export async function onPeersChanged(
  callback: (snapshot: DiscoverySnapshot) => void,
): Promise<UnlistenFn> {
  return onAppEvent("peers-changed", callback);
}

export async function getSecuritySnapshot(): Promise<SecuritySnapshot> {
  if (!isDesktopRuntime) {
    return {
      network: {
        active: true,
        error: null,
        port: 48631,
        identityFingerprint: "74A9:10C2:5F31:8D07",
      },
      trustedDevices: [...previewTrustedDevices],
    };
  }
  return invoke<SecuritySnapshot>("get_security_snapshot");
}

export async function getRelaySnapshot(): Promise<RelaySnapshot> {
  if (!isDesktopRuntime) return { ...previewRelay };
  return invoke<RelaySnapshot>("get_relay_snapshot");
}

export async function setRelayConfig(
  enabled: boolean,
  url: string,
  token: string,
): Promise<RelaySnapshot> {
  if (!isDesktopRuntime) {
    previewRelay = {
      enabled,
      url: url.trim(),
      hasToken: previewRelay.hasToken || token.length > 0,
      connected: enabled,
      onlineDevices: enabled ? previewTrustedDevices.length : 0,
      error: null,
    };
    emitPreview("relay-status-changed", previewRelay);
    return { ...previewRelay };
  }
  return invoke<RelaySnapshot>("set_relay_config", {
    enabled,
    url,
    token: token.length > 0 ? token : null,
  });
}

export const onRelayStatusChanged = (callback: (snapshot: RelaySnapshot) => void) =>
  onAppEvent("relay-status-changed", callback);

function previewClipboardSnapshot(): ClipboardSnapshot {
  return {
    enabled: previewClipboardEnabled,
    maxBytes: 8 * 1024,
    protectSensitive: true,
    pollIntervalMs: 450,
  };
}

export async function getClipboardSnapshot(): Promise<ClipboardSnapshot> {
  if (!isDesktopRuntime) return previewClipboardSnapshot();
  return invoke<ClipboardSnapshot>("get_clipboard_snapshot");
}

export async function setClipboardEnabled(enabled: boolean): Promise<ClipboardSnapshot> {
  if (!isDesktopRuntime) {
    previewClipboardEnabled = enabled;
    const snapshot = previewClipboardSnapshot();
    emitPreview("clipboard-settings-changed", snapshot);
    if (enabled) {
      window.setTimeout(() => emitPreview<ClipboardSyncEvent>("clipboard-sync-event", {
        id: crypto.randomUUID(),
        peerId: previewPlatform() === "windows" ? "preview-mac" : "preview-windows",
        peerName: previewPlatform() === "windows" ? "MacBook Pro M3" : "Surface Laptop",
        direction: "received",
        status: "synced",
        bytes: 38,
        message: "已写入系统剪贴板",
        atMs: Date.now(),
      }), 900);
    }
    return snapshot;
  }
  return invoke<ClipboardSnapshot>("set_clipboard_enabled", { enabled });
}

export async function getDiagnosticsSnapshot(): Promise<DiagnosticsSnapshot> {
  if (isDesktopRuntime) return invoke<DiagnosticsSnapshot>("get_diagnostics_snapshot");
  const [device, discovery, security, clipboard, relay] = await Promise.all([
    getLocalDevice(),
    getDiscoverySnapshot(),
    getSecuritySnapshot(),
    getClipboardSnapshot(),
    getRelaySnapshot(),
  ]);
  const incompatible = discovery.peers.filter((peer) => (
    peer.minProtocolVersion < 1
    || peer.minProtocolVersion > peer.protocolVersion
    || peer.protocolVersion < 1
    || peer.minProtocolVersion > 1
  )).length;
  return {
    generatedAtMs: Date.now(),
    appVersion: device.version,
    platform: device.platform,
    protocolVersion: 1,
    minProtocolVersion: 1,
    deviceIdPrefix: device.id.slice(0, 8),
    checks: [
      {
        id: "network",
        label: "加密传输端口",
        state: security.network.active ? "ok" : "error",
        detail: security.network.active ? "UDP 48631 正在监听 QUIC 连接" : "网络服务尚未就绪",
        guidance: security.network.active ? null : "关闭占用端口的程序，或检查系统防火墙权限",
      },
      {
        id: "discovery",
        label: "mDNS 自动发现",
        state: discovery.active && !discovery.error ? "ok" : "error",
        detail: discovery.active
          ? `广播与扫描正常 · 发现 ${discovery.peers.length} 台设备`
          : discovery.error ?? "广播或扫描未能启动",
        guidance: discovery.active ? null : "确认两端位于同一局域网，且未启用客户端隔离",
      },
      {
        id: "identity",
        label: "设备加密身份",
        state: security.network.identityFingerprint === "不可用" ? "error" : "ok",
        detail: `系统凭据库密钥可用 · 指纹 ${security.network.identityFingerprint}`,
        guidance: null,
      },
      {
        id: "protocol",
        label: "协议兼容性",
        state: incompatible > 0 ? "warning" : "ok",
        detail: incompatible > 0 ? `${incompatible} 台设备版本不兼容，传输已被阻止` : "本机支持协议 v1–v1",
        guidance: incompatible > 0 ? "请将两端 Neloa 更新到兼容版本" : null,
      },
      {
        id: "clipboard",
        label: "剪贴板守护服务",
        state: clipboard.enabled ? "ok" : "idle",
        detail: clipboard.enabled ? "已监听新复制的纯文本 · 上限 8192 字节" : "服务已加载 · 自动同步当前关闭",
        guidance: null,
      },
      {
        id: "relay",
        label: "自建中继",
        state: relay.connected ? "ok" : relay.enabled && relay.error ? "warning" : "idle",
        detail: relay.connected
          ? `已连接 · ${relay.onlineDevices} 台已配对设备在线`
          : relay.error ?? "未启用 · 局域网传输不受影响",
        guidance: relay.enabled && !relay.connected
          ? "确认中继地址、令牌和 TLS 证书有效"
          : null,
      },
    ],
    peers: discovery.peers.map((peer) => ({
      idPrefix: peer.id.slice(0, 8),
      name: peer.name,
      platform: peer.platform,
      appVersion: peer.version,
      protocolVersion: peer.protocolVersion,
      minProtocolVersion: peer.minProtocolVersion,
      compatible: peer.minProtocolVersion === 1 && peer.protocolVersion >= 1,
      capabilities: peer.capabilities,
      lastSeenMs: peer.lastSeenMs,
    })),
    firewallGuidance: device.platform === "windows"
      ? "在 Windows Defender 防火墙中允许 Neloa 访问“专用网络”；局域网传输使用 UDP 48631，mDNS 使用 UDP 5353。"
      : "若 macOS 弹出网络访问提示，请允许 Neloa 接收入站连接；局域网传输使用 UDP 48631，mDNS 使用 UDP 5353。",
    reportPrivacy: "报告不包含 IP、完整设备 ID、公钥、文件路径或剪贴板正文",
  };
}

export async function copyDiagnosticReport(): Promise<string> {
  if (isDesktopRuntime) return invoke<string>("copy_diagnostic_report");
  const snapshot = await getDiagnosticsSnapshot();
  const report = [
    "Neloa sanitized diagnostic report",
    `generatedAtMs=${snapshot.generatedAtMs}`,
    `appVersion=${snapshot.appVersion}`,
    `platform=${snapshot.platform}`,
    `deviceIdPrefix=${snapshot.deviceIdPrefix}`,
    `protocolRange=${snapshot.minProtocolVersion}-${snapshot.protocolVersion}`,
    `discovery.peerCount=${snapshot.peers.length}`,
    "privacy=No IP addresses, full device IDs, public keys, file paths, or clipboard content included.",
  ].join("\n");
  await navigator.clipboard.writeText(report);
  return "脱敏诊断报告已复制到系统剪贴板";
}

export async function beginPairing(peerId: string): Promise<PairingRequest> {
  if (!isDesktopRuntime) {
    const peer = (await getDiscoverySnapshot()).peers.find((device) => device.id === peerId);
    if (!peer) throw new Error("目标设备已离线");
    await new Promise((resolve) => window.setTimeout(resolve, 420));
    const request: PairingRequest = {
      sessionId: crypto.randomUUID(),
      peer: {
        id: peer.id,
        name: peer.name,
        platform: peer.platform,
        fingerprint: "2EC4:91B8:7A30:DD12",
      },
      code: "428195",
      direction: "outgoing",
    };
    previewPairings.set(request.sessionId, request);
    return request;
  }
  return invoke<PairingRequest>("begin_pairing", { peerId });
}

export async function decidePairing(sessionId: string, accepted: boolean): Promise<void> {
  if (!isDesktopRuntime) {
    const request = previewPairings.get(sessionId);
    if (!request) throw new Error("该配对请求已过期");
    previewPairings.delete(sessionId);
    if (accepted) {
      const now = Date.now();
      previewTrustedDevices = [
        {
          id: request.peer.id,
          name: request.peer.name,
          platform: request.peer.platform,
          publicKey: "preview-public-key",
          fingerprint: request.peer.fingerprint,
          pairedAtMs: now,
          lastVerifiedMs: now,
        },
        ...previewTrustedDevices.filter((device) => device.id !== request.peer.id),
      ];
      emitPreview("trusted-devices-changed", [...previewTrustedDevices]);
    }
    const result: PairingResult = {
      sessionId,
      peerId: request.peer.id,
      peerName: request.peer.name,
      accepted,
      message: accepted ? "验证码一致，已建立可信关系" : "本机已取消配对",
    };
    window.setTimeout(() => emitPreview("pairing-result", result), 180);
    return;
  }
  await invoke("decide_pairing", { sessionId, accepted });
}

export async function sendTestMessage(peerId: string, text: string): Promise<TestMessageEvent> {
  if (!isDesktopRuntime) {
    const trusted = previewTrustedDevices.find((device) => device.id === peerId);
    if (!trusted) throw new Error("请先完成设备配对");
    await new Promise((resolve) => window.setTimeout(resolve, 360));
    trusted.lastVerifiedMs = Date.now();
    return {
      id: crypto.randomUUID(),
      peerId,
      peerName: trusted.name,
      text,
      direction: "sent",
      atMs: Date.now(),
    };
  }
  return invoke<TestMessageEvent>("send_test_message", { peerId, text });
}

export async function revokeTrustedDevice(peerId: string): Promise<boolean> {
  if (!isDesktopRuntime) {
    const before = previewTrustedDevices.length;
    previewTrustedDevices = previewTrustedDevices.filter((device) => device.id !== peerId);
    emitPreview("trusted-devices-changed", [...previewTrustedDevices]);
    return before !== previewTrustedDevices.length;
  }
  return invoke<boolean>("revoke_trusted_device", { peerId });
}

export async function startFileTransfer(peerId: string, path: string): Promise<string> {
  if (!isDesktopRuntime) {
    const trusted = previewTrustedDevices.find((device) => device.id === peerId);
    if (!trusted) throw new Error("请先完成设备配对");
    const transferId = crypto.randomUUID();
    const size = path.endsWith("产品交付说明.pdf") ? 2_416_640 : 18_874_368;
    const base: Omit<FileTransferProgress, "stage" | "transferred" | "bytesPerSecond"> = {
      transferId,
      peerId,
      peerName: trusted.name,
      name: path.split(/[\\/]/).pop() || "Neloa Demo.zip",
      direction: "sent",
      size,
    };
    const timers = [
      window.setTimeout(() => emitPreview<FileTransferProgress>("file-transfer-progress", { ...base, stage: "hashing", transferred: size * 0.7, bytesPerSecond: 84_000_000 }), 80),
      window.setTimeout(() => emitPreview<FileTransferProgress>("file-transfer-progress", { ...base, stage: "waiting", transferred: 0, bytesPerSecond: 0 }), 260),
      window.setTimeout(() => emitPreview<FileTransferProgress>("file-transfer-progress", { ...base, stage: "transferring", transferred: size * 0.28, bytesPerSecond: 21_400_000 }), 520),
      window.setTimeout(() => emitPreview<FileTransferProgress>("file-transfer-progress", { ...base, stage: "transferring", transferred: size * 0.72, bytesPerSecond: 24_800_000 }), 900),
      window.setTimeout(() => emitPreview<FileTransferProgress>("file-transfer-progress", { ...base, stage: "verifying", transferred: size, bytesPerSecond: 23_100_000 }), 1250),
      window.setTimeout(() => {
        previewTransferTimers.delete(transferId);
        previewTransferDetails.delete(transferId);
        emitPreview<FileTransferResult>("file-transfer-result", {
          transferId,
          peerId,
          peerName: trusted.name,
          name: base.name,
          direction: "sent",
          status: "completed",
          message: "文件已加密发送并由对端校验",
          path,
          sha256: "b4e7d65f3a94c8f7c83f02aa602ff18b6ae5b20f1bfe17c2f8d31b5472eac918",
          size,
          atMs: Date.now(),
        });
      }, 1540),
    ];
    previewTransferTimers.set(transferId, timers);
    previewTransferDetails.set(transferId, {
      transferId,
      peerId,
      peerName: trusted.name,
      name: base.name,
      direction: "sent",
      path,
      size,
    });
    return transferId;
  }
  return invoke<string>("start_file_transfer", { peerId, path });
}

export async function decideFileOffer(transferId: string, accepted: boolean): Promise<void> {
  if (!isDesktopRuntime) return;
  await invoke("decide_file_offer", { transferId, accepted });
}

export async function cancelFileTransfer(transferId: string): Promise<boolean> {
  if (!isDesktopRuntime) {
    const timers = previewTransferTimers.get(transferId);
    const details = previewTransferDetails.get(transferId);
    if (!timers || !details) return false;
    timers.forEach((timer) => window.clearTimeout(timer));
    previewTransferTimers.delete(transferId);
    previewTransferDetails.delete(transferId);
    emitPreview<FileTransferResult>("file-transfer-result", {
      ...details,
      status: "cancelled",
      message: "文件发送已取消",
      sha256: null,
      atMs: Date.now(),
    });
    return true;
  }
  return invoke<boolean>("cancel_file_transfer", { transferId });
}

export const onPairingRequest = (callback: (request: PairingRequest) => void) =>
  onAppEvent("pairing-request", callback);

export const onPairingResult = (callback: (result: PairingResult) => void) =>
  onAppEvent("pairing-result", callback);

export const onTrustedDevicesChanged = (callback: (devices: TrustedDevice[]) => void) =>
  onAppEvent("trusted-devices-changed", callback);

export const onTestMessageReceived = (callback: (message: TestMessageEvent) => void) =>
  onAppEvent("test-message-received", callback);

export const onNetworkError = (callback: (message: string) => void) =>
  onAppEvent("network-error", callback);

export const onClipboardSettingsChanged = (callback: (snapshot: ClipboardSnapshot) => void) =>
  onAppEvent("clipboard-settings-changed", callback);

export const onClipboardSyncEvent = (callback: (event: ClipboardSyncEvent) => void) =>
  onAppEvent("clipboard-sync-event", callback);

export async function onFileOffer(callback: (offer: FileOffer) => void): Promise<UnlistenFn> {
  const unlisten = await onAppEvent("file-offer", callback);
  if (isDesktopRuntime || query.get("demo") !== "incoming") return unlisten;

  const timer = window.setTimeout(() => callback({
    transferId: crypto.randomUUID(),
    peerId: previewPlatform() === "windows" ? "preview-mac" : "preview-windows",
    peerName: previewPlatform() === "windows" ? "MacBook Pro M3" : "Surface Laptop",
    name: "品牌素材与交付说明.zip",
    size: 42_781_696,
    sha256: "1f99bb7c2a4e7745d518f912f8103ae91b5d4d680741116207c940695a1017cb",
  }), 320);
  return () => {
    window.clearTimeout(timer);
    unlisten();
  };
}

export const onFileTransferProgress = (callback: (progress: FileTransferProgress) => void) =>
  onAppEvent("file-transfer-progress", callback);

export const onFileTransferResult = (callback: (result: FileTransferResult) => void) =>
  onAppEvent("file-transfer-result", callback);

export interface FileInspectionResult {
  files: SelectedFile[];
  rejected: number;
  omitted: number;
}

const MAX_INSPECTED_PATHS = 100;

/** Validates native paths through Rust before they enter the send queue. */
export async function inspectFilePaths(paths: string[]): Promise<FileInspectionResult> {
  const candidates = paths.slice(0, MAX_INSPECTED_PATHS);
  const omitted = Math.max(0, paths.length - candidates.length);
  const inspected = await Promise.allSettled(
    candidates.map((path) => invoke<SelectedFile>("inspect_file", { path })),
  );
  return {
    files: inspected.flatMap((result) => (result.status === "fulfilled" ? [result.value] : [])),
    rejected: inspected.filter((result) => result.status === "rejected").length,
    omitted,
  };
}

export async function chooseFiles(): Promise<FileInspectionResult | null> {
  if (!isDesktopRuntime) {
    return {
      files: [
        {
          path: "/Users/yuki/Downloads/Neloa Demo.zip",
          name: "Neloa Demo.zip",
          size: 18_874_368,
        },
        {
          path: "/Users/yuki/Downloads/产品交付说明.pdf",
          name: "产品交付说明.pdf",
          size: 2_416_640,
        },
      ],
      rejected: 0,
      omitted: 0,
    };
  }
  const selection = await open({
    multiple: true,
    directory: false,
    title: "选择要发送的文件（可多选）",
    pickerMode: "document",
    fileAccessMode: "copy",
  });
  if (!selection) return null;
  return inspectFilePaths(Array.isArray(selection) ? selection : [selection]);
}

export async function onFileDragDrop(
  callback: (event: DragDropEvent) => void,
): Promise<UnlistenFn> {
  if (!isDesktopRuntime) return () => {};
  return getCurrentWebview().onDragDropEvent((event) => callback(event.payload));
}

export async function performWindowAction(
  action: "minimize" | "maximize" | "hide" | "close",
): Promise<void> {
  if (!isDesktopRuntime) return;
  await invoke("window_action", { action });
}

export async function startWindowDragging(): Promise<void> {
  if (!isDesktopRuntime) return;
  await getCurrentWindow().startDragging();
}
