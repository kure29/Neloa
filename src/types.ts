export type Platform = "macos" | "windows" | "linux" | "ios" | "android";

/** Which shell renders the interface. Independent of the operating system. */
export type Shell = "desktop" | "mobile";

export interface LocalDevice {
  id: string;
  name: string;
  platform: Platform;
  version: string;
}

export interface PeerDevice {
  id: string;
  name: string;
  platform: string;
  version: string;
  protocolVersion: number;
  minProtocolVersion: number;
  capabilities: string[];
  addresses: string[];
  port: number;
  lastSeenMs: number;
  relayAvailable: boolean;
}

export interface DiscoverySnapshot {
  active: boolean;
  error: string | null;
  peers: PeerDevice[];
}

export interface SelectedFile {
  path: string;
  name: string;
  size: number;
}

export interface TransferRecord {
  id: string;
  name: string;
  detail: string;
  peer: string;
  time: string;
  direction: "sent" | "received";
  kind?: "text" | "file";
  status?: FileTransferStatus;
  path?: string;
  peerId?: string;
  size?: number;
}

export type FileTransferStatus = "completed" | "failed" | "cancelled" | "rejected";
export type FileTransferStage = "hashing" | "waiting" | "transferring" | "verifying";

export interface FileOffer {
  transferId: string;
  peerId: string;
  peerName: string;
  name: string;
  size: number;
  sha256: string;
}

export interface FileTransferProgress {
  transferId: string;
  peerId: string;
  peerName: string;
  name: string;
  direction: "sent" | "received";
  stage: FileTransferStage;
  transferred: number;
  size: number;
  bytesPerSecond: number;
}

export interface FileTransferResult {
  transferId: string;
  peerId: string;
  peerName: string;
  name: string;
  direction: "sent" | "received";
  status: FileTransferStatus;
  message: string;
  path: string | null;
  sha256: string | null;
  size: number;
  atMs: number;
}

export interface TrustedDevice {
  id: string;
  name: string;
  platform: string;
  publicKey: string;
  fingerprint: string;
  pairedAtMs: number;
  lastVerifiedMs: number;
}

export interface NetworkStatus {
  active: boolean;
  error: string | null;
  port: number;
  identityFingerprint: string;
}

export interface SecuritySnapshot {
  network: NetworkStatus;
  trustedDevices: TrustedDevice[];
}

export interface RelaySnapshot {
  enabled: boolean;
  url: string;
  hasToken: boolean;
  connected: boolean;
  onlineDevices: number;
  error: string | null;
}

export interface PairingPeer {
  id: string;
  name: string;
  platform: string;
  fingerprint: string;
}

export interface PairingRequest {
  sessionId: string;
  peer: PairingPeer;
  code: string;
  direction: "incoming" | "outgoing";
}

export interface PairingResult {
  sessionId: string;
  peerId: string;
  peerName: string;
  accepted: boolean;
  message: string;
}

export interface TestMessageEvent {
  id: string;
  peerId: string;
  peerName: string;
  text: string;
  direction: "sent" | "received";
  atMs: number;
}

export interface ClipboardSnapshot {
  enabled: boolean;
  maxBytes: number;
  protectSensitive: boolean;
  pollIntervalMs: number;
}

export type ClipboardSyncStatus = "synced" | "blocked" | "failed" | "waiting";

export interface ClipboardSyncEvent {
  id: string;
  peerId: string;
  peerName: string;
  direction: "sent" | "received";
  status: ClipboardSyncStatus;
  bytes: number;
  message: string;
  atMs: number;
}

export type DiagnosticState = "ok" | "warning" | "error" | "idle";

export interface DiagnosticCheck {
  id: string;
  label: string;
  state: DiagnosticState;
  detail: string;
  guidance: string | null;
}

export interface DiagnosticPeer {
  idPrefix: string;
  name: string;
  platform: string;
  appVersion: string;
  protocolVersion: number;
  minProtocolVersion: number;
  compatible: boolean;
  capabilities: string[];
  lastSeenMs: number;
}

export interface DiagnosticsSnapshot {
  generatedAtMs: number;
  appVersion: string;
  platform: string;
  protocolVersion: number;
  minProtocolVersion: number;
  deviceIdPrefix: string;
  checks: DiagnosticCheck[];
  peers: DiagnosticPeer[];
  firewallGuidance: string;
  reportPrivacy: string;
}
