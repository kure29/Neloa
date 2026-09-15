import { StrictMode, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";

import { isDesktopRuntime } from "./bridge";
import { useNeloa } from "./lib/useNeloa";
import { DesktopShell } from "./shell/DesktopShell";
import { MobileShell } from "./shell/MobileShell";
import { ShellProvider } from "./ui/kit";

import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/components.css";
import "./styles/desktop.css";
import "./styles/mobile.css";

function App() {
  const app = useNeloa();

  useEffect(() => {
    if (!isDesktopRuntime || app.shell !== "desktop") return;
    const suppressPageMenu = (event: MouseEvent) => {
      const target = event.target;
      const nativeMenuTarget = target instanceof Element
        && target.closest("input, textarea, select, [contenteditable='true'], .selectable");
      if (!nativeMenuTarget) event.preventDefault();
    };
    document.addEventListener("contextmenu", suppressPageMenu);
    return () => document.removeEventListener("contextmenu", suppressPageMenu);
  }, [app.shell]);

  return (
    <ShellProvider value={app.shell}>
      {app.shell === "mobile" ? <MobileShell app={app} /> : <DesktopShell app={app} />}
    </ShellProvider>
  );
}

const rootElement = document.getElementById("root") as HTMLElement & { neloaRoot?: Root };
const root = rootElement.neloaRoot ?? createRoot(rootElement);
rootElement.neloaRoot = root;
root.render(<StrictMode><App /></StrictMode>);
