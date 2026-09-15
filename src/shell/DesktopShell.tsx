import type { MouseEvent as ReactMouseEvent } from "react";

import { performWindowAction, startWindowDragging } from "../bridge";
import type { NeloaState } from "../lib/useNeloa";
import { Icon } from "../ui/icons";
import { IconButton, StatusDot, cx } from "../ui/kit";
import { Overlays } from "../ui/overlays";
import { CurrentView, TABS } from "../views";

/**
 * The window is drawn by us (`decorations: false`), so the title bar is also
 * the drag handle. Everything inside it except the interactive controls moves
 * the window.
 */
function beginDrag(event: ReactMouseEvent<HTMLElement>) {
  if (event.button !== 0) return;
  if ((event.target as HTMLElement).closest("button, nav, [data-no-drag]")) return;
  void startWindowDragging();
}

/**
 * macOS and Windows share every pixel inside the frame. Only the window
 * controls, the system font stack and the frame radius differ — those belong
 * to the OS, not to Neloa.
 */
export function DesktopShell({ app }: { app: NeloaState }) {
  const isMac = app.platform !== "windows";

  return (
    <div className={cx("app", "shell-desktop", `platform-${app.platform}`)}>
      <a className="skip-link" href="#main">
        跳到主要内容
      </a>

      <header className="titlebar" onMouseDown={beginDrag}>
        <div className="titlebar-left">
          {isMac ? (
            <div className="traffic-lights" aria-label="窗口控制">
              <button
                className="traffic close"
                aria-label="关闭窗口"
                onClick={() => void performWindowAction("close")}
              />
              <button
                className="traffic minimize"
                aria-label="最小化窗口"
                onClick={() => void performWindowAction("minimize")}
              />
              <button
                className="traffic maximize"
                aria-label="最大化窗口"
                onClick={() => void performWindowAction("maximize")}
              />
            </div>
          ) : (
            <span className="app-mark" aria-hidden="true">
              <Icon name="radio" size={15} />
            </span>
          )}
          <strong className="brand">Neloa</strong>
        </div>

        <nav className="tabs" aria-label="主导航">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              className={cx("tab", app.view === tab.id && "active")}
              aria-current={app.view === tab.id ? "page" : undefined}
              onClick={() => app.setView(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </nav>

        <div className="titlebar-right">
          <span className="device-chip" title={app.statusLabel}>
            <StatusDot tone={app.discovery.error ? "danger" : app.discovery.active ? "ok" : "warn"} />
            <span className="truncate">{app.deviceName}</span>
          </span>
          {app.view === "radar" && (
            <IconButton
              icon="scan"
              label="重新查找设备"
              onClick={() => void app.refreshDiscovery(true)}
            />
          )}
          {!isMac && (
            <div className="window-controls">
              <button aria-label="最小化窗口" onClick={() => void performWindowAction("minimize")}>
                <Icon name="minimize" size={16} />
              </button>
              <button
                aria-label="最大化或还原窗口"
                onClick={() => void performWindowAction("maximize")}
              >
                <Icon name="maximize" size={16} />
              </button>
              <button
                className="window-close"
                aria-label="隐藏到后台"
                onClick={() => void performWindowAction("hide")}
              >
                <Icon name="close" size={16} />
              </button>
            </div>
          )}
        </div>
      </header>

      <main className="content" id="main" tabIndex={-1}>
        <CurrentView app={app} />
      </main>

      <Overlays app={app} />
    </div>
  );
}
