import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  beginPairing,
  cancelFileTransfer,
  chooseFiles,
  copyDiagnosticReport,
  decideFileOffer,
  decidePairing,
  detectShell,
  getClipboardSnapshot,
  getDiagnosticsSnapshot,
  getDiscoverySnapshot,
  getLocalDevice,
  getRelaySnapshot,
  getSecuritySnapshot,
  inspectFilePaths,
  isDesktopRuntime,
  onClipboardSettingsChanged,
  onClipboardSyncEvent,
  onFileDragDrop,
  onFileOffer,
  onFileTransferProgress,
  onFileTransferResult,
  onLocalDeviceChanged,
  onNetworkError,
  onPairingRequest,
  onPairingResult,
  onPeersChanged,
  onRelayStatusChanged,
  onShellChange,
  onTestMessageReceived,
  onTrustedDevicesChanged,
  previewPlatform,
  revokeTrustedDevice,
  setClipboardEnabled,
  setDeviceName,
  setRelayConfig,
  startFileTransfer,
} from "../bridge";
import type {
  ClipboardSnapshot,
  ClipboardSyncEvent,
  DiagnosticsSnapshot,
  DiscoverySnapshot,
  FileOffer,
  FileTransferProgress,
  RelaySnapshot,
  SecuritySnapshot,
  SelectedFile,
  TransferRecord,
  TrustedDevice,
} from "../types";
import { fileTransferRecord, transferRecord } from "./format";
import { resolvePrimaryAction } from "./primaryAction";

export type ViewName = "radar" | "history" | "settings";
export type BusyAction = "pair" | "file" | "clipboard" | "deviceName" | "relay" | "diagnostics" | null;

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
const EMPTY_RELAY: RelaySnapshot = {
  enabled: false,
  url: "",
  hasToken: false,
  connected: false,
  onlineDevices: 0,
  error: null,
};
const EMPTY_DIAGNOSTICS: DiagnosticsSnapshot = {
  generatedAtMs: 0,
  appVersion: "0.1.10",
  platform: "macos",
  protocolVersion: 1,
  minProtocolVersion: 1,
  deviceIdPrefix: "读取中",
  checks: [],
  peers: [],
  firewallGuidance: "正在读取本机网络建议…",
  reportPrivacy: "报告不含 IP、完整设备 ID、公钥、文件路径或剪贴板正文",
};
const MAX_QUEUED_FILES = 20;

interface QueuedFileResult {
  added: number;
  duplicates: number;
  overflow: number;
}

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
  const [relay, setRelay] = useState(EMPTY_RELAY);
  const [diagnostics, setDiagnostics] = useState(EMPTY_DIAGNOSTICS);
  const [lastClipboardEvent, setLastClipboardEvent] = useState<ClipboardSyncEvent | null>(null);
  const [view, setView] = useState<ViewName>("radar");
  const [selectedPeerId, setSelectedPeerId] = useState<string | null>(null);
  const [selectedFiles, setSelectedFiles] = useState<SelectedFile[]>([]);
  const selectedFilesRef = useRef<SelectedFile[]>([]);
  const [fileDrop, setFileDrop] = useState({ active: false, count: 0 });
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

  const replaceSelectedFiles = useCallback((files: SelectedFile[]) => {
    selectedFilesRef.current = files;
    setSelectedFiles(files);
  }, []);

  const queueSelectedFiles = useCallback((files: SelectedFile[]): QueuedFileResult => {
    const next = [...selectedFilesRef.current];
    const knownPaths = new Set(next.map((file) => file.path));
    let duplicates = 0;
    let overflow = 0;

    for (const file of files) {
      if (knownPaths.has(file.path)) {
        duplicates += 1;
      } else if (next.length >= MAX_QUEUED_FILES) {
        overflow += 1;
      } else {
        knownPaths.add(file.path);
        next.push(file);
      }
    }

    const added = next.length - selectedFilesRef.current.length;
    if (added > 0) replaceSelectedFiles(next);
    return { added, duplicates, overflow };
  }, [replaceSelectedFiles]);

  const reportQueuedFiles = useCallback((
    queued: QueuedFileResult,
    rejected: number,
    omitted: number,
  ) => {
    const unreadable = rejected + omitted;
    const skipped = queued.duplicates + queued.overflow + unreadable;
    if (queued.added > 0) {
      showToast(skipped > 0
        ? `已添加 ${queued.added} 个文件，${skipped} 个未添加`
        : `已添加 ${queued.added} 个文件`);
      return;
    }
    if (selectedFilesRef.current.length >= MAX_QUEUED_FILES || queued.overflow > 0) {
      showToast(`发送列表已满，最多 ${MAX_QUEUED_FILES} 个文件`);
    } else if (unreadable > 0) {
      showToast("未添加：文件夹或文件不可读取");
    } else if (queued.duplicates > 0) {
      showToast("所选文件已在发送列表中");
    } else {
      showToast("没有可添加的文件");
    }
  }, [showToast]);

  useEffect(() => onShellChange(setShell), []);

  useEffect(() => {
    if (shell !== "desktop" || !isDesktopRuntime) {
      setFileDrop({ active: false, count: 0 });
      return;
    }

    let disposed = false;
    let stop = () => {};
    void onFileDragDrop((event) => {
      if (disposed) return;
      if (event.type === "enter") {
        setFileDrop({ active: true, count: event.paths.length });
      } else if (event.type === "over") {
        setFileDrop((current) => current.active ? current : { active: true, count: 1 });
      } else if (event.type === "leave") {
        setFileDrop({ active: false, count: 0 });
      } else {
        setFileDrop({ active: false, count: 0 });
        setView("radar");
        void inspectFilePaths(event.paths)
          .then((result) => {
            if (disposed) return;
            reportQueuedFiles(queueSelectedFiles(result.files), result.rejected, result.omitted);
          })
          .catch((error) => {
            if (!disposed) showToast(`无法读取拖入的文件：${String(error)}`);
          });
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    }).catch((error) => {
      if (!disposed) showToast(`无法启用文件拖放：${String(error)}`);
    });

    return () => {
      disposed = true;
      stop();
    };
  }, [queueSelectedFiles, reportQueuedFiles, shell, showToast]);

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
      getRelaySnapshot(),
      getDiagnosticsSnapshot(),
    ])
      .then(([
        device,
        snapshot,
        securitySnapshot,
        clipboardSnapshot,
        relaySnapshot,
        diagnosticsSnapshot,
      ]) => {
        if (disposed) return;
        setLocal(device);
        setPlatform(device.platform);
        setDiscovery(snapshot);
        setSecurity(securitySnapshot);
        setClipboard(clipboardSnapshot);
        setRelay(relaySnapshot);
        setDiagnostics(diagnosticsSnapshot);
      })
      .catch((error) => {
        if (!disposed) showToast(String(error));
      });

    const listeners = [
      onLocalDeviceChanged((device) => {
        if (!disposed) setLocal(device);
      }),
      onPeersChanged((snapshot) => {
        if (!disposed) setDiscovery(snapshot);
      }),
      onRelayStatusChanged((snapshot) => {
        if (disposed) return;
        setRelay(snapshot);
        void refreshDiscovery();
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
  }, [refreshDiscovery, refreshSecurity, showToast]);

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
      files: selectedFiles,
      busyPairing: busyAction === "pair",
      busyFile: busyAction === "file",
    }),
    [selectedPeer, selectedPeerTrusted, selectedFiles, busyAction],
  );

  const pickFiles = useCallback(async () => {
    const selection = await chooseFiles();
    if (!selection) {
      if (!isDesktopRuntime) showToast("桌面应用中会打开系统文件选择器");
      return;
    }
    reportQueuedFiles(
      queueSelectedFiles(selection.files),
      selection.rejected,
      selection.omitted,
    );
  }, [queueSelectedFiles, reportQueuedFiles, showToast]);

  const removeSelectedFile = useCallback((path: string) => {
    replaceSelectedFiles(selectedFilesRef.current.filter((file) => file.path !== path));
  }, [replaceSelectedFiles]);

  const clearSelectedFiles = useCallback(() => {
    replaceSelectedFiles([]);
  }, [replaceSelectedFiles]);

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

  const beginFileTransfers = useCallback(async (
    peerId: string,
    files: SelectedFile[],
    removeStartedFromQueue = true,
  ) => {
    const peer = discovery.peers.find((item) => item.id === peerId);
    if (!peer) {
      showToast("目标设备已离线，请重新扫描");
      return;
    }
    setBusyAction("file");
    const startedPaths = new Set<string>();
    let lastError = "";
    try {
      for (const file of files) {
        try {
          const transferId = await startFileTransfer(peerId, file.path);
          startedPaths.add(file.path);
          setFileTransfers((current) => ({
            ...current,
            [transferId]: {
              transferId,
              peerId,
              peerName: peer.name,
              name: file.name,
              direction: "sent",
              stage: "hashing",
              transferred: 0,
              size: file.size,
              bytesPerSecond: 0,
            },
          }));
        } catch (error) {
          lastError = String(error);
        }
      }

      if (removeStartedFromQueue && startedPaths.size > 0) {
        replaceSelectedFiles(
          selectedFilesRef.current.filter((file) => !startedPaths.has(file.path)),
        );
      }
      if (startedPaths.size === files.length) {
        showToast(files.length === 1 ? "文件已加入发送队列" : `${files.length} 个文件已加入发送队列`);
      } else if (startedPaths.size > 0) {
        showToast(`已开始 ${startedPaths.size} 个文件，${files.length - startedPaths.size} 个启动失败`);
      } else {
        showToast(lastError || "无法开始文件传输");
      }
    } finally {
      setBusyAction(null);
    }
  }, [discovery.peers, replaceSelectedFiles, showToast]);

  const runPrimaryAction = useCallback(() => {
    if (primaryAction.disabled) {
      showToast(primaryAction.hint);
      return;
    }
    if (!selectedPeer) return;
    if (!selectedPeerTrusted) {
      void startPairing(selectedPeer.id);
      return;
    }
    if (selectedFiles.length === 0) {
      void pickFiles();
      return;
    }
    void beginFileTransfers(selectedPeer.id, [...selectedFiles]);
  }, [
    primaryAction,
    selectedPeer,
    selectedFiles,
    selectedPeerTrusted,
    startPairing,
    pickFiles,
    beginFileTransfers,
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
    void beginFileTransfers(record.peerId, [{
      path: record.path,
      name: record.name,
      size: record.size ?? 0,
    }], false);
  }, [beginFileTransfers, showToast]);

  const removeHistoryRecord = useCallback((recordId: string) => {
    setHistory((current) => current.filter((record) => record.id !== recordId));
    showToast("已删除记录");
  }, [showToast]);

  const clearHistory = useCallback(() => {
    setHistory([]);
    showToast("已清空传输记录");
  }, [showToast]);

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

  const configureRelay = useCallback(async (enabled: boolean, url: string, token: string) => {
    setBusyAction("relay");
    try {
      const snapshot = await setRelayConfig(enabled, url, token);
      setRelay(snapshot);
      showToast(enabled ? "中继设置已保存，正在连接…" : "中继已关闭，继续使用局域网直连");
      void refreshDiagnostics();
    } catch (error) {
      showToast(String(error));
      throw error;
    } finally {
      setBusyAction(null);
    }
  }, [refreshDiagnostics, showToast]);

  const configureDeviceName = useCallback(async (name: string) => {
    setBusyAction("deviceName");
    try {
      const device = await setDeviceName(name);
      setLocal(device);
      showToast("设备名称已更新");
      void refreshDiagnostics();
      return device;
    } catch (error) {
      showToast(String(error));
      throw error;
    } finally {
      setBusyAction(null);
    }
  }, [refreshDiagnostics, showToast]);

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
  const statusLabel = relay.connected
    ? `${discovery.peers.length} 台设备在线 · 中继已连接`
    : discovery.error
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
    relay,
    diagnostics,
    lastClipboardEvent,
    view,
    setView,
    selectedPeerId,
    setSelectedPeerId,
    selectedPeer,
    selectedPeerTrusted,
    trustedIds,
    selectedFiles,
    pickFiles,
    removeSelectedFile,
    clearSelectedFiles,
    fileDrop,
    pairing,
    submitPairing,
    fileOffers,
    submitFileOffer,
    transfers: Object.values(fileTransfers),
    cancelTransfer,
    history,
    retryTransfer,
    removeHistoryRecord,
    clearHistory,
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
    configureDeviceName,
    configureRelay,
  };
}

export type NeloaState = ReturnType<typeof useNeloa>;
