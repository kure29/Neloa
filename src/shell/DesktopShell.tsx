import { useEffect, useState, type MouseEvent as ReactMouseEvent } from "react";

import { onWindowMaximizedChange, performWindowAction, startWindowDragging } from "../bridge";
import type { NeloaState } from "../lib/useNeloa";
import { Icon } from "../ui/icons";
import { BrandMark, StatusDot, cx } from "../ui/kit";
import { Overlays } from "../ui/overlays";
import { DeviceList } from "../views/DevicePane";
import { CurrentView, TABS } from "../views";

/**
 * The window is drawn by us (`decorations: false`). There is no title bar: the
 * top strips of the sidebar and of the main pane are the drag handles, and
 * everything in them except the interactive controls moves the window.
 */
function beginDrag(event: ReactMouseEvent<HTMLElement>) {
  if (event.button !== 0) return;
  if ((event.target as HTMLElement).closest("button, nav, [data-no-drag]")) return;
  void startWindowDragging();
}

/**
 * A full-height sidebar holds the devices, the other two destinations and
 * this device's own status; the main pane shows the chosen device, the
 * history or the settings. macOS and Windows share every pixel inside the
 * frame except the window controls (traffic lights in the sidebar on macOS,
 * caption buttons top-right on Windows, which also keeps its icon and name
 * top-left as Windows apps do), the font stack and the frame radius.
 */
export function DesktopShell({ app }: { app: NeloaState }) {
  const isMac = app.platform !== "windows";
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    document.documentElement.classList.add("desktop-window");
    let disposed = false;
    let stop = () => {};
    void onWindowMaximizedChange((value) => {
      if (!disposed) setMaximized(value);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    return () => {
      disposed = true;
      stop();
      document.documentElement.classList.remove("desktop-window");
    };
  }, []);

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
      if (document.querySelector('[aria-modal="true"]')) return;
      const target = event.target;
      if (target instanceof Element && target.closest("input, textarea, select, [contenteditable='true']")) {
        return;
      }

      const view = event.key === "1"
        ? "radar"
        : event.key === "2"
          ? "history"
          : event.key === "3" || event.key === ","
            ? "settings"
            : null;
      if (!view) return;
      event.preventDefault();
      app.setView(view);
    };

    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [app.setView]);

  const activeTransfers = app.transfers.length;
  const presence = app.discovery.error
    ? "发现服务异常"
    : !app.discovery.active
      ? "正在启动"
      : app.relay.connected
        ? "可被发现 · 中继已连接"
        : "可被附近设备发现";

  return (
    <div className={cx("app", "shell-desktop", maximized && "window-maximized", `platform-${app.platform}`)}>
      <a className="skip-link" href="#main">
        跳到主要内容
      </a>

      <aside className="sidebar" aria-label="设备与导航">
        {/* The sidebar's top strip is the window's drag handle on its side. */}
        <div className="sidebar-top" onMouseDown={beginDrag}>
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
            <span className="brand-lockup">
              <BrandMark size={18} />
              <strong className="brand">Neloa</strong>
            </span>
          )}
        </div>

        <DeviceList app={app} />

        <nav className="sidebar-nav" aria-label="主导航">
          {TABS.filter((tab) => tab.id !== "radar").map((tab) => {
            const index = TABS.indexOf(tab);
            return (
              <button
                key={tab.id}
                type="button"
                className={cx("sidebar-nav-item", app.view === tab.id && "selected")}
                aria-current={app.view === tab.id ? "page" : undefined}
                aria-keyshortcuts={`Meta+${index + 1} Control+${index + 1}${tab.id === "settings" ? " Meta+, Control+," : ""}`}
                onClick={() => app.setView(tab.id)}
              >
                <Icon name={tab.icon} size={17} />
                {tab.label}
                {tab.id === "history" && activeTransfers > 0 && (
                  <span className="sidebar-nav-badge" aria-label={`${activeTransfers} 个传输进行中`}>
                    {activeTransfers}
                  </span>
                )}
              </button>
            );
          })}
        </nav>

        <button
          type="button"
          className="sidebar-me"
          title="在设置中修改本机名称"
          onClick={() => app.setView("settings")}
        >
          <StatusDot tone={app.discovery.error ? "danger" : app.discovery.active ? "ok" : "warn"} />
          <span className="sidebar-me-copy">
            <strong className="truncate">{app.deviceName}</strong>
            <span className="truncate">{presence}</span>
          </span>
        </button>
      </aside>

      <div className="desktop-main">
        <div className="main-top" onMouseDown={beginDrag}>
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

        <main className="content" id="main" tabIndex={-1}>
          <CurrentView app={app} />
        </main>
      </div>

      <Overlays app={app} />
    </div>
  );
}
