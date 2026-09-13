import type { ReactNode } from "react";

import { formatBytes, formatTime } from "../lib/format";
import type { NeloaState } from "../lib/useNeloa";
import type { DiagnosticState } from "../types";
import { Icon, deviceIcon, type IconName } from "../ui/icons";
import {
  Badge,
  Button,
  Card,
  DeviceAvatar,
  SectionTitle,
  StatusDot,
  Switch,
  cx,
  type Tone,
} from "../ui/kit";

const CHECK_TONE: Record<DiagnosticState, Tone> = {
  ok: "ok",
  idle: "neutral",
  warning: "warn",
  error: "danger",
};

const CHECK_LABEL: Record<DiagnosticState, string> = {
  ok: "正常",
  idle: "待命",
  warning: "注意",
  error: "异常",
};

function Row({
  title,
  description,
  children,
}: {
  title: string;
  description: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="row">
      <div className="row-body">
        <strong>{title}</strong>
        <span>{description}</span>
      </div>
      {children}
    </div>
  );
}

function PolicyTile({ icon, title, detail }: { icon: IconName; title: string; detail: string }) {
  return (
    <div className="policy-tile">
      <span className="policy-icon">
        <Icon name={icon} size={16} />
      </span>
      <strong className="truncate">{title}</strong>
      <small className="truncate">{detail}</small>
    </div>
  );
}

export function SettingsView({ app }: { app: NeloaState }) {
  const { security, clipboard, diagnostics, discovery, lastClipboardEvent } = app;
  const problems = diagnostics.checks.filter(
    (check) => check.state === "warning" || check.state === "error",
  ).length;

  return (
    <div className="page">
      <SectionTitle title="设置" />

      <Card>
        <Row title="本机加密身份" description={<code className="selectable">{security.network.identityFingerprint}</code>}>
          <Badge tone={security.network.active ? "ok" : "warn"}>
            {security.network.active ? "已就绪" : "不可用"}
          </Badge>
        </Row>
        <Row title="设备名称" description="局域网内广播显示的名称">
          <button
            className="row-action"
            onClick={() => app.showToast("设备重命名将随持久化设置一起接入")}
          >
            {app.local?.name ?? "读取中…"}
            <Icon name="chevron" size={14} />
          </button>
        </Row>
        <Row title="局域网自动发现" description="保持可被同一网络中的 Neloa 发现">
          <Badge tone={discovery.active ? "ok" : "warn"}>
            {discovery.active ? "已开启" : "启动中"}
          </Badge>
        </Row>
      </Card>

      <SectionTitle
        title="剪贴板同步"
        meta={clipboard.enabled ? "监听新复制的内容" : "已暂停，内容留在本机"}
      />
      <Card>
        <Row
          title={clipboard.enabled ? "已开启" : "已关闭"}
          description={clipboard.enabled
            ? "只同步开启之后新复制的纯文本"
            : "开启后当前剪贴板内容不会被发送"}
        >
          <Switch
            checked={clipboard.enabled}
            disabled={app.busyAction === "clipboard"}
            label={clipboard.enabled ? "暂停剪贴板同步" : "开启剪贴板同步"}
            onChange={() => void app.toggleClipboard()}
          />
        </Row>
        <div className="policy-grid">
          <PolicyTile icon="text" title="仅纯文本" detail={`上限 ${formatBytes(clipboard.maxBytes)}`} />
          <PolicyTile
            icon="lock"
            title="凭据保护"
            detail={clipboard.protectSensitive ? "疑似密钥本地拦截" : "未开启"}
          />
          <PolicyTile icon="noTrace" title="不留记录" detail="不保存剪贴板正文" />
        </div>
        <div className="event-line">
          <StatusDot tone={lastClipboardEvent
            ? lastClipboardEvent.status === "synced"
              ? "ok"
              : lastClipboardEvent.status === "failed"
                ? "danger"
                : "warn"
            : "neutral"}
          />
          <div className="row-body">
            <strong className="truncate">
              {lastClipboardEvent
                ? lastClipboardEvent.message
                : clipboard.enabled
                  ? "等待下一次复制"
                  : "开启后不会发送已有内容"}
            </strong>
            <span className="truncate">
              {lastClipboardEvent
                ? `${lastClipboardEvent.peerName} · ${formatBytes(lastClipboardEvent.bytes)} · ${formatTime(lastClipboardEvent.atMs)}`
                : `每 ${clipboard.pollIntervalMs} ms 检查一次变化`}
            </span>
          </div>
        </div>
      </Card>

      <SectionTitle
        title="网络诊断"
        meta={problems === 0 ? "关键服务正常" : `${problems} 项需要处理`}
        action={
          <div className="section-buttons">
            <Button
              size="sm"
              disabled={app.busyAction === "diagnostics"}
              onClick={() => void app.refreshDiagnostics(true)}
            >
              {app.busyAction === "diagnostics" ? "检查中…" : "重新检查"}
            </Button>
            <Button
              size="sm"
              disabled={app.busyAction === "diagnostics"}
              onClick={() => void app.copyDiagnostics()}
            >
              复制报告
            </Button>
          </div>
        }
      />
      <Card>
        <div className="check-grid" aria-live="polite">
          {diagnostics.checks.length === 0 ? (
            <p className="check-loading">正在检查端口、发现服务与设备身份…</p>
          ) : (
            diagnostics.checks.map((check) => (
              <section className="check" key={check.id}>
                <StatusDot tone={CHECK_TONE[check.state]} />
                <div className="row-body">
                  <strong>{check.label}</strong>
                  <span>{check.detail}</span>
                  {check.guidance && <small className="tone-warn">{check.guidance}</small>}
                </div>
                <em className={cx("check-state", `tone-${CHECK_TONE[check.state]}`)}>
                  {CHECK_LABEL[check.state]}
                </em>
              </section>
            ))
          )}
        </div>

        <div className="peer-audit">
          <div className="peer-audit-head">
            <strong>发现的设备</strong>
            <span className="mono">
              协议 v{diagnostics.minProtocolVersion}–v{diagnostics.protocolVersion} ·{" "}
              {diagnostics.peers.length} 台
            </span>
          </div>
          {diagnostics.peers.length === 0 ? (
            <p className="peer-audit-empty">
              暂未发现对端。请在另一台设备打开 Neloa，并保持在同一局域网。
            </p>
          ) : (
            diagnostics.peers.map((peer) => (
              <section className="peer-audit-row" key={`${peer.idPrefix}-${peer.name}`}>
                <span className={cx("peer-audit-icon", !peer.compatible && "warn")}>
                  <Icon name={deviceIcon(peer.platform)} size={16} />
                </span>
                <div className="row-body">
                  <strong className="truncate">{peer.name}</strong>
                  <span className="truncate mono">
                    {peer.appVersion} · v{peer.minProtocolVersion}–v{peer.protocolVersion} ·{" "}
                    {formatTime(peer.lastSeenMs)}
                  </span>
                  <small className="truncate">
                    {peer.capabilities.length > 0 ? peer.capabilities.join(" · ") : "未声明能力"}
                  </small>
                </div>
                <Badge tone={peer.compatible ? "ok" : "warn"}>
                  {peer.compatible ? "可连接" : "需更新"}
                </Badge>
              </section>
            ))
          )}
        </div>

        <div className="note">
          <span className="note-icon">
            <Icon name="shield" size={16} />
          </span>
          <div className="row-body">
            <strong>防火墙</strong>
            <span>{diagnostics.firewallGuidance}</span>
          </div>
        </div>

        <div className="card-footnote">
          <span>{diagnostics.reportPrivacy}</span>
          <time
            className="mono"
            dateTime={diagnostics.generatedAtMs
              ? new Date(diagnostics.generatedAtMs).toISOString()
              : undefined}
          >
            {diagnostics.generatedAtMs ? `检查于 ${formatTime(diagnostics.generatedAtMs)}` : "等待检查"}
          </time>
        </div>
      </Card>

      <SectionTitle title="可信设备" meta={`${security.trustedDevices.length} 台`} />
      <Card>
        {security.trustedDevices.length === 0 ? (
          <p className="card-empty">
            回到雷达页选择设备，通过六位数字完成首次配对。
          </p>
        ) : (
          security.trustedDevices.map((device) => (
            <div className="row" key={device.id}>
              <DeviceAvatar platform={device.platform} size={38} />
              <div className="row-body">
                <strong className="truncate">{device.name}</strong>
                <span className="truncate mono selectable">{device.fingerprint}</span>
                <small>最近验证 {formatTime(device.lastVerifiedMs)}</small>
              </div>
              <Button variant="danger" size="sm" onClick={() => void app.revoke(device)}>
                <Icon name="trash" size={14} />
                撤销
              </Button>
            </div>
          ))
        )}
      </Card>

      {security.network.error && (
        <p className="page-error">加密网络服务：{security.network.error}</p>
      )}
      <p className="page-footnote">Neloa {app.local?.version ?? "0.1.6"}</p>
    </div>
  );
}
