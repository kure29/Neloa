import { formatBytes, transferPercent, transferStageLabel } from "../lib/format";
import type { NeloaState, ToastTone } from "../lib/useNeloa";
import { Icon, deviceIcon } from "./icons";
import { Button, IconButton, Sheet, cx, useShell } from "./kit";

/**
 * Six-digit short-authentication-string confirmation. Both sides must read the
 * same number aloud, so the code is the largest thing on the surface and the
 * decline button is a peer of the accept button rather than a quiet link.
 */
export function PairingSheet({ app }: { app: NeloaState }) {
  const request = app.pairing;
  if (!request) return null;
  const busy = app.busyAction === "pair";

  return (
    <Sheet labelledBy="pairing-title" className="sheet-pairing">
      <div className="sheet-head">
        <span className="sheet-icon">
          <Icon name={deviceIcon(request.peer.platform)} size={20} />
        </span>
        <div>
          <h2 id="pairing-title">与 {request.peer.name} 配对</h2>
          <p>请确认两台设备显示同一组数字。不一致说明连接被中间人篡改，请立即取消。</p>
        </div>
      </div>

      <div className="sas-code selectable" aria-label={`验证码 ${request.code}`}>
        <span>{request.code.slice(0, 3)}</span>
        <span>{request.code.slice(3, 6)}</span>
      </div>

      <div className="sheet-detail">
        <span>对端密钥指纹</span>
        <code className="selectable">{request.peer.fingerprint}</code>
      </div>

      <div className="sheet-actions">
        <Button disabled={busy} onClick={() => void app.submitPairing(false)}>
          数字不同
        </Button>
        <Button variant="primary" disabled={busy} onClick={() => void app.submitPairing(true)}>
          {busy ? "正在确认…" : "一致，建立信任"}
        </Button>
      </div>
    </Sheet>
  );
}

/** Incoming file consent. Accept is not auto-focused — see `Sheet`. */
export function FileOfferSheet({ app }: { app: NeloaState }) {
  const offer = app.fileOffers[0];
  if (!offer) return null;

  return (
    <Sheet labelledBy="offer-title" className="sheet-offer">
      <div className="sheet-head">
        <span className="sheet-icon">
          <Icon name="download" size={20} />
        </span>
        <div>
          <h2 id="offer-title">{offer.peerName} 想发送文件</h2>
          <p>接受后保存到下载目录的 Neloa 文件夹；校验通过前不会出现最终文件。</p>
        </div>
      </div>

      <div className="offer-file">
        <span className="offer-file-icon">
          <Icon name="file" size={18} />
        </span>
        <div className="row-body">
          <strong className="truncate">{offer.name}</strong>
          <span>{formatBytes(offer.size)}</span>
        </div>
      </div>

      <div className="sheet-detail">
        <span>SHA-256</span>
        <code className="selectable" title={offer.sha256}>
          {offer.sha256.slice(0, 18)}…{offer.sha256.slice(-8)}
        </code>
      </div>

      <div className="sheet-actions">
        <Button onClick={() => void app.submitFileOffer(false)}>拒绝</Button>
        <Button variant="primary" onClick={() => void app.submitFileOffer(true)}>
          接受并保存
        </Button>
      </div>
    </Sheet>
  );
}

/**
 * Live transfers. Floating panel on desktop, a strip above the tab bar on
 * mobile — same rows, positioned by CSS.
 */
export function TransferTray({ app }: { app: NeloaState }) {
  const shell = useShell();
  if (app.transfers.length === 0) return null;
  const visible = app.transfers.slice(0, shell === "mobile" ? 2 : 3);

  return (
    <aside className="transfer-tray" aria-label="进行中的文件传输">
      {visible.map((transfer) => {
        const percent = transferPercent(transfer);
        const stage = transferStageLabel(transfer);

        return (
          <article className="transfer" key={transfer.transferId}>
            <div className="transfer-head">
              <span className={cx("transfer-direction", transfer.direction)}>
                <Icon name={transfer.direction === "sent" ? "sent" : "received"} size={15} />
              </span>
              <div className="row-body">
                <strong className="truncate">{transfer.name}</strong>
                <span className="truncate">
                  {stage} · {transfer.peerName}
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

            <div className="progress-meta mono">
              <span>
                {percent}% · {formatBytes(transfer.transferred)} / {formatBytes(transfer.size)}
              </span>
              <span>
                {transfer.bytesPerSecond > 0
                  ? `${formatBytes(transfer.bytesPerSecond)}/s`
                  : "端到端加密"}
              </span>
            </div>
          </article>
        );
      })}
    </aside>
  );
}

export function Toast({ message, tone }: { message: string; tone: ToastTone }) {
  const icon = tone === "ok" ? "check" : tone === "neutral" ? "pulse" : "alert";
  return (
    <div
      className={cx("toast", `tone-${tone}`, message && "visible")}
      role={tone === "danger" ? "alert" : "status"}
      aria-live={tone === "danger" ? "assertive" : "polite"}
    >
      <span className="toast-icon" aria-hidden="true">
        <Icon name={icon} size={14} />
      </span>
      <span>{message}</span>
    </div>
  );
}

export function FileDropOverlay({ app }: { app: NeloaState }) {
  if (!app.fileDrop.active) return null;
  const count = Math.max(1, app.fileDrop.count);

  return (
    <div className="file-drop-overlay" role="status" aria-live="polite" aria-atomic="true">
      <div className="file-drop-message">
        <span className="file-drop-icon" aria-hidden="true">
          <Icon name="download" size={24} />
        </span>
        <strong>释放以添加 {count} 个文件</strong>
        <span>文件会进入待发送列表，不会立即发送</span>
      </div>
    </div>
  );
}

/** Every overlay layer, so both shells mount the same set in the same order. */
export function Overlays({ app }: { app: NeloaState }) {
  return (
    <>
      <FileDropOverlay app={app} />
      <TransferTray app={app} />
      <PairingSheet app={app} />
      <FileOfferSheet app={app} />
      <Toast message={app.toast} tone={app.toastTone} />
    </>
  );
}
