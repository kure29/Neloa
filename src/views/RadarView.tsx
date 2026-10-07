import type { MouseEvent } from "react";

import { formatBytes, peerIsCompatible, platformLabel } from "../lib/format";
import type { NeloaState } from "../lib/useNeloa";
import { Badge, Button, DeviceAvatar, IconButton, cx } from "../ui/kit";
import { Icon } from "../ui/icons";

const PEER_TO_PEER_CAPABILITY = "transport-peer-to-peer-wifi";
const BLUETOOTH_CAPABILITY = "transport-bluetooth";

function routePresentation(preference: ReturnType<NeloaState["transportPreferenceFor"]>) {
  switch (preference) {
    case "lan":
      return { label: "局域网", className: "route-lan" };
    case "peerToPeer":
      return { label: "点对点 Wi-Fi", className: "route-peer" };
    case "bluetooth":
      return { label: "蓝牙", className: "route-bluetooth" };
    case "relay":
      return { label: "中继", className: "route-relay" };
    default:
      return { label: "未选择", className: "route-unselected" };
  }
}

function transportSelectId(peerId: string) {
  return `transport-${peerId}`;
}

export function RadarView({ app }: { app: NeloaState }) {
  const peers = app.discovery.peers;
  const selectedSize = app.selectedFiles.reduce((total, file) => total + file.size, 0);
  const fileSummary = app.selectedFiles.length === 1
    ? `${app.selectedFiles[0].name} · ${formatBytes(selectedSize)}`
    : `${app.selectedFiles.length} 个文件 · ${formatBytes(selectedSize)}`;
  const showFileSelection = app.selectedFiles.length > 0;
  const trustedPeersOnline = peers.filter((peer) => app.trustedIds.has(peer.id)).length;
  const discoveryLive = app.discovery.active && !app.discovery.error;
  const discoveryTone = app.discovery.error ? "danger" : discoveryLive ? "ok" : "neutral";
  const discoveryLabel = app.discovery.error ? "发现异常" : discoveryLive ? "搜索中" : "启动中";
  const showGuidance = !showFileSelection && !app.selectedPeer;

  /** Pairing needs an explicit route first; send the user straight to the picker. */
  const pairOrChooseRoute = (peerId: string) => {
    if (app.transportPreferenceFor(peerId) === "ask") {
      document.getElementById(transportSelectId(peerId))?.focus();
    } else {
      app.setSelectedPeerId(peerId);
    }
    void app.pairPeer(peerId);
  };

  const clearPeerFromBlankArea = (event: MouseEvent<HTMLElement>) => {
    if (!app.selectedPeerId) return;
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;
    if (target.closest(".peer-row, .device-browser-heading, button, a, input, select")) return;
    app.setSelectedPeerId(null);
  };

  return (
    <div className="radar-view">
      <section
        className="device-browser"
        aria-labelledby="available-devices-title"
        onClick={clearPeerFromBlankArea}
      >
        <div className="device-browser-heading">
          <div>
            <h2 id="available-devices-title">可用设备</h2>
            <p>
              {peers.length > 0
                ? `${peers.length} 台设备在线${trustedPeersOnline > 0 ? "，选择已配对设备开始传输" : ""}`
                : app.discovery.error
                  ? "设备发现暂不可用"
                  : app.discovery.active
                    ? "正在查找附近和中继设备"
                    : "正在启动设备发现"}
            </p>
          </div>
          <span
            className={cx("discovery-state", `tone-${discoveryTone}`, discoveryLive && "live")}
            aria-hidden="true"
          >
            <span className="discovery-state-dot" />
            {discoveryLabel}
          </span>
        </div>

        {peers.length > 0 ? (
          <div className="peer-list">
            {peers.map((peer) => {
              const active = peer.id === app.selectedPeerId && app.selectedPeerTrusted;
              const trustedDevice = app.trustedDevicesById.get(peer.id);
              const trusted = Boolean(trustedDevice);
              const compatible = peerIsCompatible(peer);
              const alias = trustedDevice?.alias?.trim() ?? "";
              const displayName = alias || peer.name;
              const pairBusy = app.busyAction === "pair" && peer.id === app.selectedPeerId;
              const transportPreference = app.transportPreferenceFor(peer.id);
              const route = routePresentation(transportPreference);
              const peerToPeerAvailable = peer.capabilities.includes(PEER_TO_PEER_CAPABILITY)
                && peer.peerToPeerAvailable;
              const bluetoothAvailable = peer.capabilities.includes(BLUETOOTH_CAPABILITY)
                && peer.bluetoothAvailable;

              return (
                <div
                  key={peer.id}
                  className={cx("peer-row", active && "selected", !compatible && "incompatible")}
                >
                  <button
                    type="button"
                    className="peer-row-main"
                    aria-pressed={active}
                    aria-label={trusted
                      ? `${displayName}，${platformLabel(peer.platform)}，${active ? "已选择，再次点击取消选择" : "已配对，选择设备"}`
                      : `${displayName}，${platformLabel(peer.platform)}，配对设备`}
                    disabled={!compatible || app.busyAction === "pair"}
                    onClick={() => {
                      if (trusted) {
                        app.setSelectedPeerId(active ? null : peer.id);
                      } else {
                        pairOrChooseRoute(peer.id);
                      }
                    }}
                  >
                    <DeviceAvatar
                      platform={peer.platform}
                      size={44}
                      online
                      incompatible={!compatible}
                    />
                    <span className="peer-row-copy">
                      <span className="peer-row-name">
                        <strong className="truncate" title={displayName}>{displayName}</strong>
                        {trusted && compatible && (
                          <span className={cx("peer-route", route.className)}>
                            {route.label}
                          </span>
                        )}
                      </span>
                      <small className={cx(!compatible && "warn")}>
                        {alias && <>{peer.name} · </>}
                        {platformLabel(peer.platform)} · {compatible
                          ? trusted ? "已配对" : "尚未配对"
                          : "版本不兼容"}
                      </small>
                    </span>
                  </button>

                  <span className="peer-row-actions">
                    {!compatible ? (
                      <Badge tone="warn">需更新</Badge>
                    ) : (
                      <>
                        <label
                          className={cx(
                            "peer-transport-control",
                            transportPreference === "ask" && "needs-choice",
                          )}
                          title="选择这台设备的连接方式"
                        >
                          <span className="sr-only">{displayName} 的连接方式</span>
                          <select
                            id={transportSelectId(peer.id)}
                            value={transportPreference}
                            aria-label={`${displayName} 的连接方式`}
                            disabled={app.busyAction !== null}
                            onClick={(event) => event.stopPropagation()}
                            onChange={(event) => {
                              event.stopPropagation();
                              void app.configureTransportPreference(
                                peer.id,
                                event.target.value as Parameters<typeof app.configureTransportPreference>[1],
                              );
                            }}
                          >
                            <option value="ask" disabled>选择连接方式</option>
                            <option value="lan" disabled={peer.addresses.length === 0}>局域网</option>
                            <option value="peerToPeer" disabled={!peerToPeerAvailable}>
                              {peerToPeerAvailable ? "点对点 Wi-Fi" : "点对点 Wi-Fi（不可用）"}
                            </option>
                            <option value="bluetooth" disabled={!bluetoothAvailable}>
                              {bluetoothAvailable ? "蓝牙" : "蓝牙（不可用）"}
                            </option>
                            <option value="relay" disabled={!peer.relayAvailable}>中继</option>
                          </select>
                          <Icon name="chevron" size={13} />
                        </label>
                        {trusted ? (
                          active && (
                            <IconButton
                              className="peer-selected-mark"
                              icon="check"
                              label={`取消选择 ${displayName}`}
                              onClick={() => app.setSelectedPeerId(null)}
                            />
                          )
                        ) : (
                          <Button
                            className={cx("peer-pair", transportPreference === "ask" && "awaiting-route")}
                            size="sm"
                            disabled={app.busyAction === "pair"}
                            onClick={() => pairOrChooseRoute(peer.id)}
                          >
                            {pairBusy ? <Icon className="spin" name="scan" size={14} /> : null}
                            {pairBusy ? "配对中…" : "配对"}
                          </Button>
                        )}
                      </>
                    )}
                  </span>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="device-empty">
            <span className={cx("device-empty-icon", discoveryLive && "live")} aria-hidden="true">
              <Icon name={app.discovery.error ? "alert" : "radio"} size={20} />
            </span>
            <strong>
              {app.discovery.error
                ? "无法发现设备"
                : app.discovery.active
                  ? "正在查找设备"
                  : "设备发现正在启动"}
            </strong>
            <span>
              {app.discovery.error
                ?? (app.discovery.active
                  ? "请在另一台设备上打开 Neloa；跨网络时请确认中继已连接。"
                  : "请稍候，Neloa 正在启动局域网与中继发现服务。")}
            </span>
          </div>
        )}
      </section>

      <div className={cx("send-dock", !showFileSelection && "single-action")}>
        {showFileSelection && (
          <div className="dock-selection">
            <div className="dock-file">
              <button
                className="file-picker has-file"
                disabled={app.busyAction === "file"}
                aria-describedby="file-picker-hint"
                onClick={() => void app.pickFiles()}
              >
                <Icon name="file" />
                <span className="truncate">{fileSummary}</span>
              </button>
              <span className="sr-only" id="file-picker-hint">
                打开文件选择器，继续向待发送列表添加文件
              </span>
              <IconButton
                className="file-clear"
                icon="trash"
                label="清空待发送文件"
                disabled={app.busyAction === "file"}
                onClick={app.clearSelectedFiles}
              />
            </div>

            <ul className="selected-file-list" aria-label="待发送文件">
              {app.selectedFiles.map((file) => (
                <li key={file.path}>
                  <Icon name="file" size={15} />
                  <span className="truncate" title={file.name}>{file.name}</span>
                  <small>{formatBytes(file.size)}</small>
                  <IconButton
                    className="selected-file-remove"
                    icon="close"
                    label={`移除 ${file.name}`}
                    disabled={app.busyAction === "file"}
                    onClick={() => app.removeSelectedFile(file.path)}
                  />
                </li>
              ))}
            </ul>
          </div>
        )}

        <span className="sr-only" role="status" aria-live="polite" aria-atomic="true">
          {showFileSelection
            ? `待发送列表共 ${app.selectedFiles.length} 个文件`
            : "待发送列表为空"}
        </span>

        {showGuidance ? (
          <div className="dock-guidance">
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
            <p className="dock-guidance-copy">
              {peers.length === 0
                ? "等待其他设备打开 Neloa"
                : trustedPeersOnline > 0
                  ? "点按上方已配对的设备即可开始"
                  : "新设备需要先选择连接方式，再核对六位数字完成配对"}
              {app.shell === "desktop" && "；也可以先把文件拖进窗口"}
            </p>
            <Button
              className="dock-add-files"
              size="sm"
              onClick={() => void app.pickFiles()}
            >
              <Icon name="plus" size={14} />
              添加文件
            </Button>
          </div>
        ) : (
          <>
            <Button
              variant="primary"
              className="dock-send"
              disabled={app.primaryAction.disabled}
              onClick={app.runPrimaryAction}
            >
              {!showFileSelection && app.selectedPeerTrusted && <Icon name="plus" size={16} />}
              {app.primaryAction.label}
            </Button>

            <p id="send-file-hint" className={cx("dock-hint", `tone-${app.primaryAction.tone}`)}>
              {app.primaryAction.hint}
            </p>
          </>
        )}
      </div>

    </div>
  );
}
