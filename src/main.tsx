import { StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";

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
