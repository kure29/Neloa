import { useEffect, useState, type FormEvent, type ReactNode } from "react";

import { formatBytes, formatTime, platformLabel } from "../lib/format";
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

const CHECK_DISPLAY_LABEL: Record<string, string> = {
  network: "局域网连接",
  discovery: "附近设备发现",
  identity: "设备身份保护",
  protocol: "客户端版本",
  clipboard: "剪贴板同步",
  relay: "自建中继",
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
  const { security, clipboard, relay, diagnostics, lastClipboardEvent } = app;
  const [deviceName, setDeviceName] = useState(app.local?.name ?? "");
  const [deviceNameError, setDeviceNameError] = useState("");
  const [deviceNameSaved, setDeviceNameSaved] = useState(false);
  const [relayEnabled, setRelayEnabled] = useState(relay.enabled);
  const [relayUrl, setRelayUrl] = useState(relay.url);
  const [relayToken, setRelayToken] = useState("");
  const [relayFormError, setRelayFormError] = useState("");
  const [relaySaved, setRelaySaved] = useState(false);
  const [relayEditing, setRelayEditing] = useState(!(relay.hasToken && relay.url));
  const problemChecks = diagnostics.checks.filter(
    (check) => check.state === "warning" || check.state === "error",
  );
  const problems = problemChecks.length;
  const checking = app.busyAction === "diagnostics" || diagnostics.checks.length === 0;
  const connectionTone: Tone = checking
    ? "neutral"
    : problemChecks.some((check) => check.state === "error")
      ? "danger"
      : problems > 0
        ? "warn"
        : "ok";

  useEffect(() => {
    setDeviceName(app.local?.name ?? "");
  }, [app.local?.name]);

  useEffect(() => {
    setRelayEnabled(relay.enabled);
    setRelayUrl(relay.url);
    setRelayEditing(!(relay.hasToken && relay.url));
  }, [relay.enabled, relay.hasToken, relay.url]);

  useEffect(() => {
    if (!deviceNameSaved) return;
    const timer = window.setTimeout(() => setDeviceNameSaved(false), 1800);
    return () => window.clearTimeout(timer);
  }, [deviceNameSaved]);

  useEffect(() => {
    if (!relaySaved) return;
    const timer = window.setTimeout(() => setRelaySaved(false), 1800);
    return () => window.clearTimeout(timer);
  }, [relaySaved]);

  const submitDeviceName = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const normalized = deviceName.trim();
    setDeviceNameError("");
    if (!normalized) {
      setDeviceNameError("设备名称不能为空");
      return;
    }
    if ([...normalized].length > 32) {
      setDeviceNameError("设备名称最多 32 个字符");
      return;
    }
    try {
      const device = await app.configureDeviceName(normalized);
      setDeviceName(device.name);
      setDeviceNameSaved(true);
    } catch (error) {
      setDeviceNameError(String(error).replace(/^Error:\s*/, ""));
    }
  };

  const submitRelay = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setRelayFormError("");
    try {
      await app.configureRelay(relayEnabled, relayUrl, relayToken);
      setRelayToken("");
      setRelaySaved(true);
      setRelayEditing(false);
    } catch (error) {
      setRelayFormError(String(error).replace(/^Error:\s*/, ""));
    }
  };

  const relayTone: Tone = relay.connected
    ? "relay"
    : relay.enabled && relay.error
      ? "danger"
      : relay.enabled
        ? "warn"
        : "neutral";
  const relayLabel = relay.connected
    ? "已连接"
    : relay.enabled && relay.error
      ? "连接失败"
      : relay.enabled
        ? "连接中"
        : "未启用";
  const relayConfigured = Boolean(relay.url && relay.hasToken);
  const relayDirty = relayEnabled !== relay.enabled
    || relayUrl.trim() !== relay.url
    || relayToken.length > 0;
  const connectionReadyDescription = relay.connected
    ? "局域网直连与自建中继均可用"
    : relay.enabled && relay.error
      ? "局域网仍可用；自建中继需要处理"
      : relay.enabled
        ? "局域网可用；正在连接自建中继"
        : "局域网连接已就绪";

  const cancelRelayEdit = () => {
    setRelayEnabled(relay.enabled);
    setRelayUrl(relay.url);
    setRelayToken("");
    setRelayFormError("");
    setRelaySaved(false);
    setRelayEditing(!relayConfigured);
  };

  return (
    <div className="page">
      <SectionTitle title="本机" />
      <Card>
        <form
          className="device-name-config"
          aria-busy={app.busyAction === "deviceName"}
          onSubmit={(event) => void submitDeviceName(event)}
        >
          <label className="setting-field device-name-field">
            <span>设备名称</span>
            <input
              value={deviceName}
              autoComplete="off"
              maxLength={32}
              placeholder="例如：Yuki 的 iPhone"
              required
              aria-describedby="device-name-hint"
              aria-invalid={Boolean(deviceNameError)}
              onChange={(event) => {
                setDeviceName(event.target.value);
                setDeviceNameError("");
                setDeviceNameSaved(false);
              }}
            />
            <small id="device-name-hint">其他设备会看到这个名称，最多 32 个字符。</small>
            {deviceNameError && (
              <small className="setting-field-error" role="alert">{deviceNameError}</small>
            )}
          </label>
          <Button
            type="submit"
            size="sm"
            className={cx(deviceNameSaved && "btn-confirmed")}
            disabled={
              app.busyAction === "deviceName"
              || !app.local
              || deviceName.trim() === app.local.name
            }
          >
            {app.busyAction === "deviceName" ? (
              <><Icon className="spin" name="scan" size={14} />保存中…</>
            ) : deviceNameSaved ? (
              <><Icon name="check" size={14} />已保存</>
            ) : "保存名称"}
          </Button>
        </form>
      </Card>

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

      <SectionTitle title="连接" />
      <Card>
          <form
            className="relay-config"
            aria-busy={app.busyAction === "relay"}
            onSubmit={(event) => void submitRelay(event)}
          >
            <div className="row relay-heading">
              <div className="row-body">
                <strong>自建中继</strong>
                <span>
                  {relay.connected
                    ? `${relay.onlineDevices} 台已配对设备通过中继在线`
                    : relay.enabled
                      ? "正在保持与自建中继的连接"
                      : "局域网不可达时自动回退，不经中继配对"}
                </span>
              </div>
              <Badge tone={relayTone}>{relayLabel}</Badge>
              <Switch
                checked={relayEnabled}
                disabled={app.busyAction === "relay"}
                label={relayEnabled ? "关闭自建中继" : "启用自建中继"}
                onChange={() => {
                  setRelayEnabled((enabled) => !enabled);
                  setRelayEditing(true);
                  setRelayFormError("");
                  setRelaySaved(false);
                }}
              />
            </div>

            {relayEnabled && relayEditing && (
              <div className="relay-fields">
                <label className="setting-field">
                  <span>WebSocket 地址</span>
                  <input
                    value={relayUrl}
                    inputMode="url"
                    maxLength={2048}
                    placeholder="wss://relay.example.com/v1/ws"
                    spellCheck={false}
                    required
                    aria-describedby="relay-url-hint"
                    onChange={(event) => {
                      setRelayUrl(event.target.value);
                      setRelaySaved(false);
                    }}
                  />
                  <small id="relay-url-hint">公网必须使用 wss://；省略路径时自动补全。</small>
                </label>
                <label className="setting-field">
                  <span>访问令牌</span>
                  <input
                    type="password"
                    value={relayToken}
                    autoComplete="off"
                    maxLength={512}
                    placeholder={relay.hasToken ? "已安全保存；留空表示不更换" : "至少 32 个字符"}
                    spellCheck={false}
                    required={!relay.hasToken}
                    aria-describedby="relay-token-hint"
                    aria-invalid={Boolean(relayFormError)}
                    onChange={(event) => {
                      setRelayToken(event.target.value);
                      setRelaySaved(false);
                    }}
                  />
                  <small id="relay-token-hint">令牌只保存在系统凭据库，不会在连接状态中显示。</small>
                </label>
              </div>
            )}

            {(relayFormError || (relay.enabled && relay.error)) && (
              <p className="relay-error" role="alert">
                {relayFormError || relay.error}
              </p>
            )}

            {relayEditing || relayDirty ? (
              <div className="relay-actions">
                <span>关闭中继不会影响局域网直连。</span>
                <div className="relay-action-buttons">
                  {relayConfigured && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      disabled={app.busyAction === "relay"}
                      onClick={cancelRelayEdit}
                    >
                      取消
                    </Button>
                  )}
                  <Button
                    type="submit"
                    size="sm"
                    disabled={app.busyAction === "relay" || !relayDirty}
                  >
                    {app.busyAction === "relay" ? (
                      <><Icon className="spin" name="scan" size={14} />保存中…</>
                    ) : "保存"}
                  </Button>
                </div>
              </div>
            ) : (
              <button
                type="button"
                className="relay-summary-action"
                onClick={() => {
                  setRelayEditing(true);
                  setRelaySaved(false);
                }}
              >
                <span className="truncate">
                  {relay.url || "尚未配置中继地址"}
                </span>
                <strong>
                  {relaySaved ? (
                    <><Icon name="check" size={14} />已保存</>
                  ) : (
                    <>修改配置<Icon name="chevron" size={14} /></>
                  )}
                </strong>
              </button>
            )}
          </form>
      </Card>

      <Card className={cx("connection-health", `tone-${connectionTone}`)}>
        <div className="connection-health-summary" aria-live="polite">
          <span className="connection-health-icon" aria-hidden="true">
            <Icon
              className={checking ? "spin" : undefined}
              name={checking ? "scan" : problems > 0 ? "alert" : "check"}
              size={18}
            />
          </span>
          <div className="row-body">
            <strong>
              {checking
                ? "正在检查连接"
                : problems === 0
                  ? "连接正常"
                  : `${problems} 项需要处理`}
            </strong>
            <span>
              {checking
                ? "正在确认设备发现、传输与中继状态…"
                : problems === 0
                  ? connectionReadyDescription
                  : "按下面的建议处理后，再重新检查"}
            </span>
          </div>
          <Button
            variant="ghost"
            size="sm"
            disabled={app.busyAction === "diagnostics"}
            aria-label={checking ? "正在重新检查连接" : "重新检查连接"}
            onClick={() => void app.refreshDiagnostics(true)}
          >
            <Icon className={checking ? "spin" : undefined} name="scan" size={14} />
            {checking ? "检查中…" : "重新检查"}
          </Button>
        </div>

        {problemChecks.length > 0 && (
          <div className="connection-issues">
            {problemChecks.map((check) => (
              <section className="connection-issue" key={check.id}>
                <StatusDot tone={CHECK_TONE[check.state]} />
                <div className="row-body">
                  <strong>{CHECK_DISPLAY_LABEL[check.id] ?? check.label}</strong>
                  <span>{check.guidance ?? check.detail}</span>
                </div>
                <Badge tone={CHECK_TONE[check.state]}>
                  {check.state === "error" ? "暂不可用" : "需留意"}
                </Badge>
              </section>
            ))}
          </div>
        )}

        <details className="diagnostic-details">
          <summary>
            <span className="diagnostic-summary-copy">
              <strong>连接状态</strong>
              <small>
                {diagnostics.generatedAtMs
                  ? `上次检查 ${formatTime(diagnostics.generatedAtMs)}`
                  : "等待首次检查"}
              </small>
            </span>
            <Icon className="diagnostic-summary-chevron" name="chevron" size={16} />
          </summary>

          <div className="diagnostic-panel">
            <div className="check-grid">
              {diagnostics.checks.length === 0 ? (
                <p className="check-loading">正在读取技术信息…</p>
              ) : (
                diagnostics.checks.map((check) => (
                  <section className="check" key={check.id}>
                    <StatusDot tone={CHECK_TONE[check.state]} />
                    <div className="row-body">
                      <strong>{CHECK_DISPLAY_LABEL[check.id] ?? check.label}</strong>
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
                  <strong>网络与防火墙说明</strong>
                  <span>{diagnostics.firewallGuidance}</span>
                </div>
              </div>
            )}

          </div>
        </details>
      </Card>

      <SectionTitle
        title="设备信任"
        meta={security.trustedDevices.length > 0
          ? `${security.trustedDevices.length} 台已信任`
          : "暂无"}
      />
      <Card>
        <div className="setting-note">
          <Icon name="shield" size={16} />
          <span>这里也会保留当前离线的设备；移除后，需要重新核对六位数字才能连接。</span>
        </div>
        {security.trustedDevices.length === 0 ? (
          <p className="card-empty">
            回到设备页选择设备，通过六位数字完成首次配对。
          </p>
        ) : (
          security.trustedDevices.map((device) => {
            const online = app.discovery.peers.some((peer) => peer.id === device.id);
            return (
              <div className="row" key={device.id}>
                <DeviceAvatar platform={device.platform} size={38} online={online} trusted />
                <div className="row-body">
                  <strong className="truncate">{device.alias?.trim() || device.name}</strong>
                  <span className="truncate">
                    {device.alias?.trim() && `${device.name} · `}
                    {platformLabel(device.platform)} · {online ? "在线" : "离线"} · 最近连接{" "}
                    {formatTime(device.lastVerifiedMs)}
                  </span>
                </div>
                <Button variant="danger" size="sm" onClick={() => void app.revoke(device)}>
                  <Icon name="trash" size={14} />
                  移除
                </Button>
              </div>
            );
          })
        )}
      </Card>

      {security.network.error && !problemChecks.some((check) => check.id === "network") && (
        <p className="page-error">局域网连接暂不可用：{security.network.error}</p>
      )}
      <p className="page-footnote">Neloa {app.local?.version ?? "0.1.11"}</p>
    </div>
  );
}
