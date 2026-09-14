import { useEffect, useRef, useState, type RefObject } from "react";

import { formatBytes, peerIsCompatible } from "../lib/format";
import type { NeloaState } from "../lib/useNeloa";
import { Button, DeviceAvatar, IconButton, cx } from "../ui/kit";
import { Icon } from "../ui/icons";

const TAU = Math.PI * 2;
const PEERS_PER_RING = 6;

/**
 * The radar sizes itself from the stage instead of a fixed radius, so the same
 * component works in a 720px desktop window and on a 360px phone without
 * scaling the labels into a blur.
 */
function useStageRadius(ref: RefObject<HTMLDivElement | null>): number {
  const [radius, setRadius] = useState(150);

  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      const room = Math.min(width, height) / 2 - 68;
      // The floor keeps peer tiles clear of the local device in the centre.
      setRadius(Math.max(120, Math.min(room, 190)));
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [ref]);

  return radius;
}

export function RadarView({ app }: { app: NeloaState }) {
  const stageRef = useRef<HTMLDivElement>(null);
  const radius = useStageRadius(stageRef);
  const peers = app.discovery.peers;
  const ringCount = Math.max(1, Math.ceil(peers.length / PEERS_PER_RING));

  return (
    <div className="radar-view">
      <div className="radar-stage" ref={stageRef}>
        {[0.55, 0.8, 1.05].map((scale) => (
          <span
            key={scale}
            className="radar-ring"
            style={{ width: radius * 2 * scale, height: radius * 2 * scale }}
          />
        ))}
        <span
          className="radar-sweep"
          style={{ width: radius * 2 * 1.05, height: radius * 2 * 1.05 }}
        />

        <div className="local-device">
          <DeviceAvatar platform={app.platform} size={58} />
          <strong className="truncate">{app.deviceName}</strong>
          <span>本机 · {app.statusLabel}</span>
        </div>

        <div className="peer-layer">
          {peers.map((peer, index) => {
            const ring = Math.floor(index / PEERS_PER_RING);
            const seat = index % PEERS_PER_RING;
            const seats = Math.min(PEERS_PER_RING, peers.length - ring * PEERS_PER_RING);
            const stagger = ring % 2 === 1 ? Math.PI / seats : 0;
            const angle = (seat / seats) * TAU - Math.PI / 2 + stagger;
            const distance = ringCount === 1
              ? radius
              : radius * (0.58 + (0.42 * ring) / (ringCount - 1));

            const active = peer.id === app.selectedPeerId;
            const trusted = app.trustedIds.has(peer.id);
            const compatible = peerIsCompatible(peer);
            const relayOnly = peer.relayAvailable && peer.addresses.length === 0;
            const status = !compatible
              ? "版本不兼容"
              : trusted
                ? relayOnly
                  ? "已配对 · 中继"
                  : "已配对"
                : "未配对";

            return (
              <button
                key={peer.id}
                className={cx("peer-node", active && "selected")}
                style={{
                  left: `calc(50% + ${Math.round(distance * Math.cos(angle))}px)`,
                  top: `calc(50% + ${Math.round(distance * Math.sin(angle))}px)`,
                }}
                aria-pressed={active}
                onClick={() => app.setSelectedPeerId(peer.id)}
              >
                <DeviceAvatar
                  platform={peer.platform}
                  online
                  trusted={trusted && compatible}
                  incompatible={!compatible}
                />
                <strong className="truncate">{peer.name}</strong>
                <small className={cx(!compatible && "warn", trusted && compatible && "ok")}>
                  {status}
                </small>
              </button>
            );
          })}
        </div>

        {peers.length === 0 && (
          <div className="radar-empty">
            <strong>{app.discovery.error ? "无法启动局域网发现" : "正在寻找附近设备"}</strong>
            <span>
              {app.discovery.error ?? "请确认另一台设备已打开 Neloa，并连接同一网络"}
            </span>
          </div>
        )}
      </div>

      <div className="send-dock">
        <div className="dock-file">
          <button
            className={cx("file-picker", app.selectedFile && "has-file")}
            onClick={() => void app.pickFile()}
          >
            <Icon name={app.selectedFile ? "file" : "plus"} />
            <span className="truncate">
              {app.selectedFile
                ? `${app.selectedFile.name} · ${formatBytes(app.selectedFile.size)}`
                : "选择要发送的文件"}
            </span>
          </button>
          {app.selectedFile && (
            <IconButton
              className="file-clear"
              icon="close"
              label={`移除 ${app.selectedFile.name}`}
              onClick={app.clearFile}
            />
          )}
        </div>

        <Button
          variant="primary"
          className="dock-send"
          disabled={app.primaryAction.disabled}
          onClick={app.runPrimaryAction}
        >
          {app.primaryAction.label}
        </Button>

        <p className={cx("dock-hint", `tone-${app.primaryAction.tone}`)}>
          {app.primaryAction.hint}
        </p>
      </div>
    </div>
  );
}
