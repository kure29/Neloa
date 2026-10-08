import type { NeloaState } from "../lib/useNeloa";
import { Icon } from "../ui/icons";
import { IconButton, StatusDot, cx } from "../ui/kit";
import { Overlays } from "../ui/overlays";
import { CurrentView, TABS } from "../views";
import { peerDisplayName } from "../views/DevicePane";

/**
 * Same state as the desktop, framed for a phone: a safe-area header and a
 * bottom tab bar. On the device tab the header doubles as the navigation bar,
 * with a way back to the list while a device's screen is open.
 */
export function MobileShell({ app }: { app: NeloaState }) {
  const detailPeer = app.view === "radar" ? app.selectedPeer : null;
  // The device page already counts online peers, so the header describes this
  // device's own reachability instead of repeating that number.
  const presence = app.discovery.error
    ? "发现服务异常"
    : !app.discovery.active
      ? "正在启动"
      : app.relay.connected
        ? "可被附近设备发现 · 中继已连接"
        : "可被附近设备发现";

  return (
    <div className={cx("app", "shell-mobile", `platform-${app.platform}`)}>
      <a className="skip-link" href="#main">
        跳到主要内容
      </a>

      <header className="mobile-header">
        {detailPeer ? (
          <>
            <button
              type="button"
              className="mobile-back"
              onClick={() => app.setSelectedPeerId(null)}
            >
              <Icon name="chevron" size={18} />
              设备
            </button>
            <strong className="mobile-header-title truncate">{peerDisplayName(app, detailPeer)}</strong>
            <span className="mobile-header-spacer" aria-hidden="true" />
          </>
        ) : (
          <>
            <div className="row-body">
              <strong className="truncate">{app.deviceName}</strong>
              <span className="truncate">
                <StatusDot tone={app.discovery.error ? "danger" : app.discovery.active ? "ok" : "warn"} />
                {presence}
              </span>
            </div>
            {app.view === "radar" && (
              <IconButton
                icon="scan"
                label="重新查找设备"
                onClick={() => void app.refreshDiscovery(true)}
              />
            )}
          </>
        )}
      </header>

      <main className="content" id="main" tabIndex={-1}>
        <CurrentView app={app} />
      </main>

      <nav className="tabbar" aria-label="主导航">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            className={cx("tabbar-item", app.view === tab.id && "active")}
            aria-current={app.view === tab.id ? "page" : undefined}
            onClick={() => {
              // Tapping the current device tab pops back to the list.
              if (tab.id === "radar" && app.view === "radar") app.setSelectedPeerId(null);
              app.setView(tab.id);
            }}
          >
            <Icon name={tab.icon} size={21} />
            <span>{tab.short}</span>
          </button>
        ))}
      </nav>

      <Overlays app={app} />
    </div>
  );
}
