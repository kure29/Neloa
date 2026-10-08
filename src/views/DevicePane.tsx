import { useEffect } from "react";

import {
  TRANSFER_STATUS_LABELS,
  formatBytes,
  peerIsCompatible,
  platformLabel,
  transferPercent,
  transferStageLabel,
} from "../lib/format";
import { formatRecordMoment } from "../lib/historyGroups";
import type { NeloaState } from "../lib/useNeloa";
import type { FileTransferProgress, PeerDevice, TransferRecord } from "../types";
import { Icon } from "../ui/icons";
import { Badge, Button, DeviceAvatar, IconButton, cx } from "../ui/kit";
import { TRANSPORT_LABELS, TransportSelect } from "../ui/transport";

const detailSelectId = (peerId: string) => `transport-detail-${peerId}`;

function peerDisplayName(app: NeloaState, peer: PeerDevice) {
  return app.trustedDevicesById.get(peer.id)?.alias?.trim() || peer.name;
}

function recordTone(record: TransferRecord) {
  if (record.kind !== "file" || !record.status || record.status === "completed") return "ok" as const;
  return record.status === "failed" ? "danger" as const : "warn" as const;
}

/** Selects an unpaired device and pairs it, or points at the route picker first. */
function startPairingFlow(app: NeloaState, peerId: string) {
  app.setView("radar");
  app.setSelectedPeerId(peerId);
  if (app.transportPreferenceFor(peerId) === "ask") {
    requestAnimationFrame(() => document.getElementById(detailSelectId(peerId))?.focus());
    return;
  }
  void app.pairPeer(peerId);
}

/** The sidebar's device section: discovery state and the grouped device rows. */
export function DeviceList({ app }: { app: NeloaState }) {
  const peers = app.discovery.peers;
  const trustedPeers = peers.filter((peer) => app.trustedIds.has(peer.id));
  const newPeers = peers.filter((peer) => !app.trustedIds.has(peer.id));
  const discoveryLive = app.discovery.active && !app.discovery.error;
  const discoveryTone = app.discovery.error ? "danger" : discoveryLive ? "ok" : "neutral";
  const discoveryLabel = app.discovery.error ? "发现异常" : discoveryLive ? "搜索中" : "启动中";

  const renderRow = (peer: PeerDevice) => {
    const trusted = app.trustedIds.has(peer.id);
    const compatible = peerIsCompatible(peer);
    const selected = app.view === "radar" && peer.id === app.selectedPeerId;
    const displayName = peerDisplayName(app, peer);
    const transfer = app.transfers.find((item) => item.peerId === peer.id);
    const subtitle = !compatible
      ? "版本不兼容"
      : transfer
        ? `${transfer.direction === "sent" ? "正在发送" : "正在接收"} ${transferPercent(transfer)}%`
        : trusted
          ? `${TRANSPORT_LABELS[app.transportPreferenceFor(peer.id)]} · 在线`
          : `${platformLabel(peer.platform)} · 未配对`;

    return (
      <li key={peer.id} className={cx("drow", selected && "selected", !compatible && "incompatible")}>
        <button
          type="button"
          className="drow-main"
          aria-current={selected ? "page" : undefined}
          aria-label={`${displayName}，${platformLabel(peer.platform)}，${subtitle}`}
          onClick={() => {
            app.setView("radar");
            app.setSelectedPeerId(peer.id);
          }}
        >
          <DeviceAvatar
            platform={peer.platform}
            size={34}
            online
            trusted={trusted}
            incompatible={!compatible}
          />
          <span className="drow-copy">
            <strong className="truncate" title={displayName}>{displayName}</strong>
            <span className={cx("drow-sub truncate", !compatible && "warn")}>{subtitle}</span>
          </span>
        </button>
        {!trusted && compatible && !selected && (
          <button
            type="button"
            className="drow-pair"
            disabled={app.busyAction === "pair"}
            onClick={() => startPairingFlow(app, peer.id)}
          >
            配对
          </button>
        )}
      </li>
    );
  };

  return (
    <section className="sidebar-devices" aria-labelledby="devices-title">
      <header className="sidebar-devices-head">
        <h2 id="devices-title">设备</h2>
        <span
          className={cx("discovery-state", `tone-${discoveryTone}`, discoveryLive && "live")}
          aria-hidden="true"
        >
          <span className="discovery-state-dot" />
          {discoveryLabel}
        </span>
        <IconButton
          className="sidebar-refresh"
          icon="scan"
          label="重新查找设备"
          onClick={() => void app.refreshDiscovery(true)}
        />
      </header>

      <div className="sidebar-scroll">
        {peers.length === 0 ? (
          <p className="sidebar-empty">
            {app.discovery.error
              ? "设备发现暂不可用"
              : "还没有发现其他设备。请在另一台设备上打开 Neloa。"}
          </p>
        ) : (
          <>
            {trustedPeers.length > 0 && (
              <>
                <h3 className="sidebar-group">已配对</h3>
                <ul className="sidebar-rows">{trustedPeers.map(renderRow)}</ul>
              </>
            )}
            {newPeers.length > 0 && (
              <>
                <h3 className="sidebar-group">附近的新设备</h3>
                <ul className="sidebar-rows">{newPeers.map(renderRow)}</ul>
              </>
            )}
          </>
        )}
      </div>
    </section>
  );
}

/**
 * The desktop device page: everything about the device chosen in the sidebar
 * — its route, the drop target, the send queue, its live transfers and its
 * recent history — so choosing a device and sending to it happen in one place.
 * Phones keep the single-column RadarView.
 */
export function DevicePane({ app }: { app: NeloaState }) {
  const { setSelectedPeerId } = app;

  useEffect(() => {
    const deselect = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || document.querySelector('[aria-modal="true"]')) return;
      const target = event.target;
      if (target instanceof Element && target.closest("input, textarea, select")) return;
      setSelectedPeerId(null);
    };
    window.addEventListener("keydown", deselect);
    return () => window.removeEventListener("keydown", deselect);
  }, [setSelectedPeerId]);

  return (
    <section className="split-detail" aria-label="设备详情">
      {app.selectedPeer ? (
        <PeerDetail
          key={app.selectedPeer.id}
          app={app}
          peer={app.selectedPeer}
          onPair={() => startPairingFlow(app, app.selectedPeer!.id)}
        />
      ) : (
        <NoSelection app={app} />
      )}
    </section>
  );
}

function PeerDetail({
  app,
  peer,
  onPair,
}: {
  app: NeloaState;
  peer: PeerDevice;
  onPair: () => void;
}) {
  const trusted = app.trustedIds.has(peer.id);
  const compatible = peerIsCompatible(peer);
  const displayName = peerDisplayName(app, peer);
  const preference = app.transportPreferenceFor(peer.id);
  const transfers = app.transfers.filter((transfer) => transfer.peerId === peer.id);
  const records = app.history
    .filter((record) => record.peerId ? record.peerId === peer.id : record.peer === peer.name)
    .slice(0, 4);
  const pairBusy = app.busyAction === "pair";

  return (
    <>
      <header className="detail-head">
        <DeviceAvatar
          platform={peer.platform}
          size={52}
          online
          trusted={trusted}
          incompatible={!compatible}
        />
        <div className="detail-title">
          <h2 className="truncate" title={displayName}>{displayName}</h2>
          <p>
            {displayName !== peer.name && <>{peer.name} · </>}
            {platformLabel(peer.platform)} · {!compatible ? "版本不兼容" : trusted ? "已配对" : "未配对"}
            {trusted && compatible && (
              <>
                {" · 端到端加密 "}
                <Icon name="lock" size={13} />
              </>
            )}
          </p>
        </div>
        <div className="detail-actions">
          {trusted && compatible && (
            <TransportSelect
              app={app}
              peer={peer}
              displayName={displayName}
              id={detailSelectId(peer.id)}
            />
          )}
          <IconButton icon="close" label="取消选择这台设备" onClick={() => app.setSelectedPeerId(null)} />
        </div>
      </header>

      {!compatible ? (
        <div className="detail-note tone-warn" role="status">
          <Icon name="alert" size={18} />
          <div>
            <strong>版本不兼容</strong>
            <p>{displayName} 的 Neloa 版本（{peer.version}）与本机不兼容，请更新对方后再连接。</p>
          </div>
        </div>
      ) : !trusted ? (
        <div className="card pair-card">
          <div className="pair-card-body">
            <span className="pair-card-icon" aria-hidden="true"><Icon name="shield" size={20} /></span>
            <div>
              <strong>先与这台设备配对</strong>
              <p>选择连接方式后开始配对。两台设备会显示同一组六位数字，核对一致后才会建立信任，之后就可以直接发送文件。</p>
            </div>
          </div>
          <div className="pair-card-actions">
            <TransportSelect
              app={app}
              peer={peer}
              displayName={displayName}
              id={detailSelectId(peer.id)}
            />
            <Button variant="primary" disabled={preference === "ask" || pairBusy} onClick={onPair}>
              {pairBusy ? <><Icon className="spin" name="scan" size={14} />配对中…</> : "开始配对"}
            </Button>
          </div>
          {preference === "ask" && <p className="pair-card-hint">请先选择连接方式</p>}
        </div>
      ) : (
        <DropTarget app={app} routeMissing={preference === "ask"} />
      )}

      {app.selectedFiles.length > 0 && (
        <SendQueue
          app={app}
          footnote={!trusted || !compatible
            ? "配对完成后才能发送"
            : preference === "ask"
              ? "请先选择连接方式"
              : `将通过${TRANSPORT_LABELS[preference]}发送，端到端加密`}
          canSend={trusted && compatible}
        />
      )}

      {transfers.length > 0 && (
        <>
          <h3 className="detail-label">进行中</h3>
          <TransferList app={app} transfers={transfers} />
        </>
      )}

      {trusted && (
        <>
          <div className="detail-label-row">
            <h3 className="detail-label">与这台设备的最近记录</h3>
            {app.history.length > 0 && (
              <button type="button" className="row-action" onClick={() => app.setView("history")}>
                全部记录<Icon name="chevron" size={14} />
              </button>
            )}
          </div>
          {records.length === 0 ? (
            <p className="detail-empty">还没有和这台设备的传输记录。</p>
          ) : (
            <ul className="record-list">
              {records.map((record) => (
                <li className="record" key={record.id}>
                  <span className={cx("record-direction", record.direction)}>
                    <Icon name={record.direction === "sent" ? "sent" : "received"} />
                  </span>
                  <div className="record-body">
                    <strong className="truncate">{record.name}</strong>
                    <span className="truncate">{formatRecordMoment(record.atMs)} · {record.detail}</span>
                  </div>
                  <Badge tone={recordTone(record)}>
                    {record.kind === "file" && record.status ? TRANSFER_STATUS_LABELS[record.status] : "已加密"}
                  </Badge>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </>
  );
}

function DropTarget({ app, routeMissing }: { app: NeloaState; routeMissing: boolean }) {
  return (
    <div className={cx("drop-target", app.fileDrop.active && "active")}>
      <span className="drop-target-icon" aria-hidden="true"><Icon name="download" size={20} /></span>
      <div className="drop-target-copy">
        <strong>把文件拖到这里</strong>
        <span>{routeMissing ? "发送前请先在右上角选择连接方式" : "文件会先进入待发送列表，确认后再发送"}</span>
      </div>
      <Button size="sm" disabled={app.busyAction === "file"} onClick={() => void app.pickFiles()}>
        <Icon name="plus" size={14} />
        选择文件
      </Button>
    </div>
  );
}

function SendQueue({
  app,
  footnote,
  canSend,
}: {
  app: NeloaState;
  footnote: string;
  canSend: boolean;
}) {
  const total = app.selectedFiles.reduce((sum, file) => sum + file.size, 0);
  const busy = app.busyAction === "file";

  return (
    <section className="send-queue" aria-label="待发送文件">
      <ul>
        {app.selectedFiles.map((file) => (
          <li key={file.path}>
            <Icon name="file" size={16} />
            <span className="truncate" title={file.name}>{file.name}</span>
            <small>{formatBytes(file.size)}</small>
            <IconButton
              icon="close"
              label={`移除 ${file.name}`}
              disabled={busy}
              onClick={() => app.removeSelectedFile(file.path)}
            />
          </li>
        ))}
      </ul>
      <footer>
        <span>
          <Icon name="lock" size={13} />
          共 {app.selectedFiles.length} 个文件，{formatBytes(total)}；{footnote}
        </span>
        <Button variant="ghost" size="sm" disabled={busy} onClick={app.clearSelectedFiles}>
          清空
        </Button>
        {canSend && (
          <Button variant="primary" disabled={app.primaryAction.disabled} onClick={app.runPrimaryAction}>
            {app.primaryAction.label}
          </Button>
        )}
      </footer>
      <span className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        待发送列表共 {app.selectedFiles.length} 个文件
      </span>
    </section>
  );
}

function TransferList({ app, transfers }: { app: NeloaState; transfers: FileTransferProgress[] }) {
  return (
    <ul className="detail-transfers">
      {transfers.map((transfer) => {
        const percent = transferPercent(transfer);
        const stage = transferStageLabel(transfer);
        return (
          <li key={transfer.transferId}>
            <div className="detail-transfer-head">
              <span className={cx("transfer-direction", transfer.direction)}>
                <Icon name={transfer.direction === "sent" ? "sent" : "received"} size={15} />
              </span>
              <div className="row-body">
                <strong className="truncate">{transfer.name}</strong>
                <span className="truncate">
                  {stage} · {percent}% · {formatBytes(transfer.transferred)} / {formatBytes(transfer.size)}
                  {transfer.bytesPerSecond > 0 && ` · ${formatBytes(transfer.bytesPerSecond)}/s`}
                </span>
              </div>
              <IconButton
                icon="close"
                label={`取消传输 ${transfer.name}`}
                onClick={() => void app.cancelTransfer(transfer.transferId)}
              />
            </div>
            <div
              className="progress-track"
              role="progressbar"
              aria-label={`${transfer.name} ${stage}`}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={percent}
            >
              <span style={{ transform: `scaleX(${percent / 100})` }} />
            </div>
          </li>
        );
      })}
    </ul>
  );
}

function NoSelection({ app }: { app: NeloaState }) {
  const peers = app.discovery.peers;
  const hasTrusted = peers.some((peer) => app.trustedIds.has(peer.id));
  const live = app.discovery.active && !app.discovery.error;

  return (
    <>
      <div className="detail-empty-state">
        <span className={cx("device-empty-icon", live && "live")} aria-hidden="true">
          <Icon name={app.discovery.error ? "alert" : "radio"} size={22} />
        </span>
        <strong>
          {app.discovery.error
            ? "无法发现设备"
            : peers.length === 0
              ? "正在查找设备"
              : "选择左侧的一台设备"}
        </strong>
        <p>
          {app.discovery.error
            ?? (peers.length === 0
              ? "请在另一台设备上打开 Neloa；跨网络时请确认中继已连接。"
              : hasTrusted
                ? "已配对的设备可以直接发送；新设备需要先选择连接方式，再核对六位数字完成配对。"
                : "新设备需要先选择连接方式，再核对六位数字完成配对。")}
        </p>
        <ol className="dock-steps" aria-label="发送步骤">
          <li className="current">
            <span className="dock-step-index" aria-hidden="true">1</span>
            选择设备
          </li>
          <li>
            <span className="dock-step-index" aria-hidden="true">2</span>
            添加文件
          </li>
          <li>
            <span className="dock-step-index" aria-hidden="true">3</span>
            加密发送
          </li>
        </ol>
        {app.selectedFiles.length === 0 && (
          <Button size="sm" onClick={() => void app.pickFiles()}>
            <Icon name="plus" size={14} />
            先选择文件
          </Button>
        )}
      </div>
      {app.selectedFiles.length > 0 && (
        <SendQueue app={app} footnote="选择左侧的设备后发送" canSend={false} />
      )}
    </>
  );
}
