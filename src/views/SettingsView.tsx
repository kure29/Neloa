import type { ReactNode } from "react";

import { formatBytes, formatTime } from "../lib/format";
import type { NeloaState } from "../lib/useNeloa";
import type { DiagnosticState } from "../types";
import { Icon } from "../ui/icons";
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

export function SettingsView({ app }: { app: NeloaState }) {
  const { security, clipboard, diagnostics, discovery, lastClipboardEvent } = app;
  const problems = diagnostics.checks.filter(
    (check) => check.state === "warning" || check.state === "error",
  ).length;

  return (
    <div className="page">
      <SectionTitle title="设置" />

      <SectionTitle
        title="剪贴板同步"
        meta={clipboard.enabled ? "已开启" : "已关闭"}
      />
      <Card>
        <Row
          title="自动同步新复制的文字"
          description={clipboard.enabled
            ? "在已配对设备之间同步"
            : "内容只保留在这台设备"}
        >
          <Switch
            checked={clipboard.enabled}
            disabled={app.busyAction === "clipboard"}
            label={clipboard.enabled ? "暂停剪贴板同步" : "开启剪贴板同步"}
            onChange={() => void app.toggleClipboard()}
          />
        </Row>
        <div className="setting-note">
          <Icon name="shield" size={16} />
          <span>
            只同步开启后新复制的纯文本；敏感内容会被拦截，也不会写入记录。
          </span>
        </div>
        {lastClipboardEvent && (
          <div className="clipboard-event">
            <StatusDot
              tone={lastClipboardEvent.status === "synced"
                ? "ok"
                : lastClipboardEvent.status === "failed"
                  ? "danger"
                  : "warn"}
            />
            <div className="row-body">
              <strong className="truncate">{lastClipboardEvent.message}</strong>
              <span className="truncate">
                {lastClipboardEvent.peerName} · {formatBytes(lastClipboardEvent.bytes)} ·{" "}
                {formatTime(lastClipboardEvent.atMs)}
              </span>
            </div>
          </div>
        )}
      </Card>

      <SectionTitle title="已配对设备" meta={`${security.trustedDevices.length} 台`} />
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
                <span>最近连接 {formatTime(device.lastVerifiedMs)}</span>
              </div>
              <Button variant="danger" size="sm" onClick={() => void app.revoke(device)}>
                <Icon name="trash" size={14} />
                移除
              </Button>
            </div>
          ))
        )}
      </Card>

      <SectionTitle title="高级" />
      <details className="card advanced-settings">
        <summary>
          <span className="advanced-summary-icon">
            <Icon name="sliders" size={18} />
          </span>
          <span className="advanced-summary-copy">
            <strong>连接与诊断</strong>
            <small>{problems === 0 ? "一切正常" : `${problems} 项需要处理`}</small>
          </span>
          <Icon className="advanced-summary-chevron" name="chevron" size={16} />
        </summary>

        <div className="advanced-panel">
          <Row title="设备名称" description="附近设备会看到这个名称">
            <span className="row-value">{app.local?.name ?? "读取中…"}</span>
          </Row>
          <Row
            title="设备身份"
            description={<code className="selectable">{security.network.identityFingerprint}</code>}
          >
            <Badge tone={security.network.active ? "ok" : "warn"}>
              {security.network.active ? "已就绪" : "不可用"}
            </Badge>
          </Row>
          <Row title="附近设备发现" description="在同一局域网内自动寻找设备">
            <Badge tone={discovery.active ? "ok" : "warn"}>
              {discovery.active ? "正常" : "启动中"}
            </Badge>
          </Row>

          <div className="advanced-actions">
            <div className="row-body">
              <strong>网络诊断</strong>
              <span>{problems === 0 ? "未发现需要处理的问题" : `${problems} 项需要处理`}</span>
            </div>
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
          </div>

          <div className="check-grid" aria-live="polite">
            {diagnostics.checks.length === 0 ? (
              <p className="check-loading">正在检查连接状态…</p>
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

          {problems > 0 && (
            <div className="note">
              <span className="note-icon">
                <Icon name="shield" size={16} />
              </span>
              <div className="row-body">
                <strong>连接建议</strong>
                <span>{diagnostics.firewallGuidance}</span>
              </div>
            </div>
          )}

          <div className="card-footnote">
            <span>{diagnostics.reportPrivacy}</span>
            <time
              className="mono"
              dateTime={diagnostics.generatedAtMs
                ? new Date(diagnostics.generatedAtMs).toISOString()
                : undefined}
            >
              {diagnostics.generatedAtMs
                ? `检查于 ${formatTime(diagnostics.generatedAtMs)}`
                : "等待检查"}
            </time>
          </div>
        </div>
      </details>

      {security.network.error && (
        <p className="page-error">加密网络服务：{security.network.error}</p>
      )}
      <p className="page-footnote">Neloa {app.local?.version ?? "0.1.8"}</p>
    </div>
  );
}
