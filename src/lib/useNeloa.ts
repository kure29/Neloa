import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  beginPairing,
  cancelFileTransfer,
  chooseFile,
  copyDiagnosticReport,
  decideFileOffer,
  decidePairing,
  detectShell,
  getClipboardSnapshot,
  getDiagnosticsSnapshot,
  getDiscoverySnapshot,
  getLocalDevice,
  getSecuritySnapshot,
  isDesktopRuntime,
  onClipboardSettingsChanged,
  onClipboardSyncEvent,
  onFileOffer,
  onFileTransferProgress,
  onFileTransferResult,
  onNetworkError,
  onPairingRequest,
  onPairingResult,
  onPeersChanged,
  onShellChange,
  onTestMessageReceived,
  onTrustedDevicesChanged,
  previewPlatform,
  revokeTrustedDevice,
  setClipboardEnabled,
  startFileTransfer,
} from "../bridge";
import type {
  ClipboardSnapshot,
  ClipboardSyncEvent,
  DiagnosticsSnapshot,
  DiscoverySnapshot,
  FileOffer,
  FileTransferProgress,
  SecuritySnapshot,
  TransferRecord,
  TrustedDevice,
} from "../types";
import { fileTransferRecord, transferRecord } from "./format";
import { resolvePrimaryAction } from "./primaryAction";

export type ViewName = "radar" | "history" | "settings";
export type BusyAction = "pair" | "file" | "clipboard" | "diagnostics" | null;

const EMPTY_DISCOVERY: DiscoverySnapshot = { active: false, error: null, peers: [] };
const EMPTY_SECURITY: SecuritySnapshot = {
  network: { active: false, error: null, port: 48631, identityFingerprint: "读取中…" },
  trustedDevices: [],
};
const EMPTY_CLIPBOARD: ClipboardSnapshot = {
  enabled: false,
  maxBytes: 8 * 1024,
  protectSensitive: true,
  pollIntervalMs: 450,
};
const EMPTY_DIAGNOSTICS: DiagnosticsSnapshot = {
  generatedAtMs: 0,
  appVersion: "0.1.1",
  platform: "macos",
  protocolVersion: 1,
  minProtocolVersion: 1,
  deviceIdPrefix: "读取中",
  checks: [],
  peers: [],
  firewallGuidance: "正在读取本机网络建议…",
  reportPrivacy: "报告不含 IP、完整设备 ID、公钥、文件路径或剪贴板正文",
};

/**
 * Every piece of application state, so the desktop and mobile shells render the
 * same data and share every action. The shells differ only in layout.
 */
export function useNeloa() {
  const [platform, setPlatform] = useState(previewPlatform());
  const [shell, setShell] = useState(detectShell());
  const [local, setLocal] = useState<Awaited<ReturnType<typeof getLocalDevice>> | null>(null);
  const [discovery, setDiscovery] = useState(EMPTY_DISCOVERY);
  const [security, setSecurity] = useState(EMPTY_SECURITY);
  const [clipboard, setClipboard] = useState(EMPTY_CLIPBOARD);
  const [diagnostics, setDiagnostics] = useState(EMPTY_DIAGNOSTICS);
  const [lastClipboardEvent, setLastClipboardEvent] = useState<ClipboardSyncEvent | null>(null);
  const [view, setView] = useState<ViewName>("radar");
  const [selectedPeerId, setSelectedPeerId] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<Awaited<ReturnType<typeof chooseFile>>>(null);
  const [pairing, setPairing] = useState<Awaited<ReturnType<typeof beginPairing>> | null>(null);
  const [fileOffers, setFileOffers] = useState<FileOffer[]>([]);
  const [fileTransfers, setFileTransfers] = useState<Record<string, FileTransferProgress>>({});
  const [busyAction, setBusyAction] = useState<BusyAction>(null);
  const [toast, setToast] = useState("");
  const [history, setHistory] = useState<TransferRecord[]>([]);
  const toastTimer = useRef<number | undefined>(undefined);

  const showToast = useCallback((message: string) => {
    window.clearTimeout(toastTimer.current);
    setToast(message);
    toastTimer.current = window.setTimeout(() => setToast(""), 3000);
  }, []);

  useEffect(() => onShellChange(setShell), []);

  const refreshDiscovery = useCallback(async (announce = false) => {
    if (announce) showToast("正在重新扫描…");
    try {
      setDiscovery(await getDiscoverySnapshot());
    } catch (error) {
      setDiscovery({ active: false, peers: [], error: String(error) });
    }
  }, [showToast]);

  const refreshSecurity = useCallback(async () => {
    try {
      setSecurity(await getSecuritySnapshot());
    } catch (error) {
      setSecurity((current) => ({
        ...current,
        network: { ...current.network, active: false, error: String(error) },
      }));
    }
  }, []);

  const refreshDiagnostics = useCallback(async (announce = false) => {
    if (announce) setBusyAction("diagnostics");
    try {
      const snapshot = await getDiagnosticsSnapshot();
      setDiagnostics(snapshot);
      if (announce) showToast("诊断已刷新");
    } catch (error) {
      if (announce) showToast(String(error));
    } finally {
      if (announce) setBusyAction(null);
    }
  }, [showToast]);

  useEffect(() => {
    let disposed = false;
    const cleanups: Array<() => void> = [];

    Promise.all([
      getLocalDevice(),
      getDiscoverySnapshot(),
      getSecuritySnapshot(),
      getClipboardSnapshot(),
      getDiagnosticsSnapshot(),
    ])
      .then(([device, snapshot, securitySnapshot, clipboardSnapshot, diagnosticsSnapshot]) => {
        if (disposed) return;
        setLocal(device);
        setPlatform(device.platform);
        setDiscovery(snapshot);
        setSecurity(securitySnapshot);
        setClipboard(clipboardSnapshot);
        setDiagnostics(diagnosticsSnapshot);
      })
      .catch((error) => {
        if (!disposed) showToast(String(error));
      });

    const listeners = [
      onPeersChanged((snapshot) => {
        if (!disposed) setDiscovery(snapshot);
      }),
      onPairingRequest((request) => {
        if (!disposed) {
          setBusyAction(null);
          setPairing(request);
        }
      }),
      onPairingResult((result) => {
        if (disposed) return;
        setBusyAction(null);
        setPairing((current) => (current?.sessionId === result.sessionId ? null : current));
        showToast(result.message);
        if (result.accepted) void refreshSecurity();
      }),
      onTrustedDevicesChanged((trustedDevices) => {
        if (!disposed) setSecurity((current) => ({ ...current, trustedDevices }));
      }),
      onTestMessageReceived((message) => {
        if (disposed) return;
        setHistory((current) => [transferRecord(message), ...current]);
        showToast(`收到 ${message.peerName} 的加密文本`);
      }),
      onNetworkError((message) => {
        if (!disposed) showToast(message);
      }),
      onClipboardSettingsChanged((snapshot) => {
        if (!disposed) setClipboard(snapshot);
      }),
      onClipboardSyncEvent((event) => {
        if (disposed) return;
        setLastClipboardEvent(event);
        if (event.status === "waiting") return;
        if (event.status === "synced") {
          showToast(event.direction === "received"
            ? `已从 ${event.peerName} 同步剪贴板`
            : `剪贴板已同步至 ${event.peerName}`);
        } else {
          showToast(event.message);
        }
      }),
      onFileOffer((offer) => {
        if (!disposed) setFileOffers((current) => [...current, offer]);
      }),
      onFileTransferProgress((progress) => {
        if (!disposed) {
          setFileTransfers((current) => ({ ...current, [progress.transferId]: progress }));
        }
      }),
      onFileTransferResult((result) => {
        if (disposed) return;
        setFileTransfers((current) => {
          const next = { ...current };
          delete next[result.transferId];
          return next;
        });
        setFileOffers((current) => current.filter((offer) => offer.transferId !== result.transferId));
        setHistory((current) => [fileTransferRecord(result), ...current]);
        showToast(result.message);
      }),
    ];

    Promise.all(listeners).then((unlisteners) => {
      if (disposed) unlisteners.forEach((stop) => stop());
      else cleanups.push(...unlisteners);
    });

    return () => {
      disposed = true;
      cleanups.forEach((stop) => stop());
      window.clearTimeout(toastTimer.current);
    };
  }, [refreshSecurity, showToast]);

  useEffect(() => {
    if (view === "settings") void refreshDiagnostics();
  }, [view, refreshDiagnostics]);

  useEffect(() => {
    if (!selectedPeerId && discovery.peers[0]) setSelectedPeerId(discovery.peers[0].id);
    if (selectedPeerId && !discovery.peers.some((peer) => peer.id === selectedPeerId)) {
      setSelectedPeerId(discovery.peers[0]?.id ?? null);
    }
  }, [discovery.peers, selectedPeerId]);

  const selectedPeer = useMemo(
    () => discovery.peers.find((peer) => peer.id === selectedPeerId) ?? null,
    [discovery.peers, selectedPeerId],
  );
  const trustedIds = useMemo(
    () => new Set(security.trustedDevices.map((device) => device.id)),
    [security.trustedDevices],
  );
  const selectedPeerTrusted = selectedPeer ? trustedIds.has(selectedPeer.id) : false;

  const primaryAction = useMemo(
    () => resolvePrimaryAction({
      peer: selectedPeer,
      trusted: selectedPeerTrusted,
      file: selectedFile,
      busyPairing: busyAction === "pair",
      busyFile: busyAction === "file",
    }),
    [selectedPeer, selectedPeerTrusted, selectedFile, busyAction],
  );

  const pickFile = useCallback(async () => {
    const file = await chooseFile();
    if (!file) {
      if (!isDesktopRuntime) showToast("桌面应用中会打开系统文件选择器");
      return;
    }
    setSelectedFile(file);
  }, [showToast]);

  const startPairing = useCallback(async (peerId: string) => {
    if (!security.network.active) {
      showToast(security.network.error ?? "加密网络服务正在启动，请稍后再试");
      return;
    }
    setBusyAction("pair");
    try {
      setPairing(await beginPairing(peerId));
    } catch (error) {
      showToast(String(error));
    } finally {
      setBusyAction(null);
    }
  }, [security.network, showToast]);

  const beginFileTransfer = useCallback(async (
    peerId: string,
    path: string,
    name: string,
    size: number,
  ) => {
    const peer = discovery.peers.find((item) => item.id === peerId);
    if (!peer) {
      showToast("目标设备已离线，请重新扫描");
      return;
    }
    setBusyAction("file");
    try {
      const transferId = await startFileTransfer(peerId, path);
      setFileTransfers((current) => ({
        ...current,
        [transferId]: {
          transferId,
          peerId,
          peerName: peer.name,
          name,
          direction: "sent",
          stage: "hashing",
          transferred: 0,
          size,
          bytesPerSecond: 0,
        },
      }));
    } catch (error) {
      showToast(String(error));
    } finally {
      setBusyAction(null);
    }
  }, [discovery.peers, showToast]);

  const runPrimaryAction = useCallback(() => {
    if (primaryAction.disabled) {
      showToast(primaryAction.hint);
      return;
    }
    if (!selectedPeer || !selectedFile) return;
    if (!selectedPeerTrusted) {
      void startPairing(selectedPeer.id);
      return;
    }
    void beginFileTransfer(selectedPeer.id, selectedFile.path, selectedFile.name, selectedFile.size);
  }, [
    primaryAction,
    selectedPeer,
    selectedFile,
    selectedPeerTrusted,
    startPairing,
    beginFileTransfer,
    showToast,
  ]);

  const submitPairing = useCallback(async (accepted: boolean) => {
    if (!pairing) return;
    const request = pairing;
    setBusyAction("pair");
    try {
      await decidePairing(request.sessionId, accepted);
      setPairing(null);
      showToast(accepted ? "本机已确认，等待对方…" : "已取消配对");
    } catch (error) {
      showToast(String(error));
    } finally {
      setBusyAction(null);
    }
  }, [pairing, showToast]);

  const submitFileOffer = useCallback(async (accepted: boolean) => {
    const offer = fileOffers[0];
    if (!offer) return;
    try {
      await decideFileOffer(offer.transferId, accepted);
      setFileOffers((current) => current.slice(1));
      if (accepted) {
        setFileTransfers((current) => ({
          ...current,
          [offer.transferId]: {
            transferId: offer.transferId,
            peerId: offer.peerId,
            peerName: offer.peerName,
            name: offer.name,
            direction: "received",
            stage: "waiting",
            transferred: 0,
            size: offer.size,
            bytesPerSecond: 0,
          },
        }));
      } else {
        showToast("已拒绝接收");
      }
    } catch (error) {
      showToast(String(error));
    }
  }, [fileOffers, showToast]);

  const cancelTransfer = useCallback(async (transferId: string) => {
    try {
      const cancelled = await cancelFileTransfer(transferId);
      showToast(cancelled ? "正在取消…" : "该传输已结束");
    } catch (error) {
      showToast(String(error));
    }
  }, [showToast]);

  const retryTransfer = useCallback((record: TransferRecord) => {
    if (!record.peerId || !record.path) {
      showToast("缺少重试所需的文件信息");
      return;
    }
    void beginFileTransfer(record.peerId, record.path, record.name, record.size ?? 0);
  }, [beginFileTransfer, showToast]);

  const revoke = useCallback(async (device: TrustedDevice) => {
    try {
      const removed = await revokeTrustedDevice(device.id);
      if (removed) {
        setSecurity((current) => ({
          ...current,
          trustedDevices: current.trustedDevices.filter((item) => item.id !== device.id),
        }));
        showToast(`已撤销对 ${device.name} 的信任`);
      }
    } catch (error) {
      showToast(String(error));
    }
  }, [showToast]);

  const toggleClipboard = useCallback(async () => {
    setBusyAction("clipboard");
    try {
      const snapshot = await setClipboardEnabled(!clipboard.enabled);
      setClipboard(snapshot);
      showToast(snapshot.enabled
        ? "已开启；当前剪贴板内容不会发送，等待下一次复制"
        : "剪贴板同步已暂停");
    } catch (error) {
      showToast(String(error));
    } finally {
      setBusyAction(null);
    }
  }, [clipboard.enabled, showToast]);

  const copyDiagnostics = useCallback(async () => {
    setBusyAction("diagnostics");
    try {
      showToast(await copyDiagnosticReport());
      setDiagnostics(await getDiagnosticsSnapshot());
    } catch (error) {
      showToast(String(error));
    } finally {
      setBusyAction(null);
    }
  }, [showToast]);

  const deviceName = local?.name ?? "读取设备…";
  const statusLabel = discovery.error
    ? "发现服务异常"
    : discovery.active
      ? `${discovery.peers.length} 台设备在线`
      : "正在启动";

  return {
    platform,
    shell,
    local,
    deviceName,
    statusLabel,
    discovery,
    security,
    clipboard,
    diagnostics,
    lastClipboardEvent,
    view,
    setView,
    selectedPeerId,
    setSelectedPeerId,
    selectedPeer,
    selectedPeerTrusted,
    trustedIds,
    selectedFile,
    pickFile,
    clearFile: () => setSelectedFile(null),
    pairing,
    submitPairing,
    fileOffers,
    submitFileOffer,
    transfers: Object.values(fileTransfers),
    cancelTransfer,
    history,
    retryTransfer,
    busyAction,
    toast,
    showToast,
    primaryAction,
    runPrimaryAction,
    refreshDiscovery,
    refreshDiagnostics,
    copyDiagnostics,
    revoke,
    toggleClipboard,
  };
}

export type NeloaState = ReturnType<typeof useNeloa>;
