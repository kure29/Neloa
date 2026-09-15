import type {
  FileTransferProgress,
  FileTransferResult,
  PeerDevice,
  TestMessageEvent,
  TransferRecord,
} from "../types";

export function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = size / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value >= 10 ? value.toFixed(1) : value.toFixed(2)} ${unit}`;
}

export function formatTime(timestamp: number): string {
  return new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit" })
    .format(new Date(timestamp));
}

export function peerIsCompatible(peer: PeerDevice): boolean {
  return peer.minProtocolVersion > 0
    && peer.minProtocolVersion <= peer.protocolVersion
    && peer.protocolVersion >= 1
    && peer.minProtocolVersion <= 1;
}

export function platformLabel(platform: string): string {
  const normalized = platform.toLowerCase();
  if (normalized === "macos") return "macOS";
  if (normalized === "ios") return "iOS";
  if (normalized === "windows") return "Windows";
  if (normalized === "android") return "Android";
  if (normalized === "debian") return "Debian";
  if (normalized === "ubuntu") return "Ubuntu";
  if (normalized === "fedora") return "Fedora";
  if (normalized === "arch") return "Arch Linux";
  if (normalized === "manjaro") return "Manjaro";
  if (normalized === "opensuse") return "openSUSE";
  if (normalized === "linuxmint") return "Linux Mint";
  if (normalized === "redhat") return "Red Hat Enterprise Linux";
  if (normalized === "linux") return "Linux";
  return platform || "未知系统";
}

export function transferRecord(message: TestMessageEvent): TransferRecord {
  const condensed = message.text.replace(/\s+/g, " ").trim();
  return {
    id: message.id,
    name: condensed.length > 28 ? `“${condensed.slice(0, 28)}…”` : `“${condensed}”`,
    detail: "加密文本",
    peer: message.peerName,
    time: formatTime(message.atMs),
    direction: message.direction,
    kind: "text",
    status: "completed",
  };
}

export function fileTransferRecord(result: FileTransferResult): TransferRecord {
  return {
    id: result.transferId,
    name: result.name,
    detail: `${formatBytes(result.size)} · ${result.message}`,
    peer: result.peerName,
    time: formatTime(result.atMs),
    direction: result.direction,
    kind: "file",
    status: result.status,
    path: result.path ?? undefined,
    peerId: result.peerId,
    size: result.size,
  };
}

export function transferStageLabel(progress: FileTransferProgress): string {
  if (progress.stage === "hashing") return "计算校验值";
  if (progress.stage === "waiting") return progress.direction === "sent" ? "等待对方接受" : "准备接收";
  if (progress.stage === "verifying") return "校验并写入";
  return progress.direction === "sent" ? "加密发送中" : "接收并解密";
}

export function transferPercent(progress: FileTransferProgress): number {
  if (progress.size === 0) return progress.stage === "verifying" ? 100 : 0;
  return Math.min(100, Math.round((progress.transferred / progress.size) * 100));
}

export const TRANSFER_STATUS_LABELS = {
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
  rejected: "被拒绝",
} as const;
