import type { NeloaState } from "../lib/useNeloa";
import { Icon } from "../ui/icons";
import { IconButton, StatusDot, cx } from "../ui/kit";
import { Overlays } from "../ui/overlays";
import { CurrentView, TABS } from "../views";

/**
 * Same views, same state — only the frame changes: a safe-area header instead
 * of a title bar, and a bottom tab bar instead of a tab strip. There are no
 * window controls, because there is no window.
 */
export function MobileShell({ app }: { app: NeloaState }) {
  return (
    <div className={cx("app", "shell-mobile", `platform-${app.platform}`)}>
      <a className="skip-link" href="#main">
        跳到主要内容
      </a>

      <header className="mobile-header">
        <div className="row-body">
          <strong className="truncate">{app.deviceName}</strong>
          <span className="truncate">
            <StatusDot tone={app.discovery.error ? "danger" : app.discovery.active ? "ok" : "warn"} />
            {app.statusLabel}
          </span>
        </div>
        {app.view === "radar" && (
          <IconButton
            icon="scan"
            label="重新查找设备"
            onClick={() => void app.refreshDiscovery(true)}
          />
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
            onClick={() => app.setView(tab.id)}
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
