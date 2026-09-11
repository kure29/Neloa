import type { NeloaState, ViewName } from "../lib/useNeloa";
import type { IconName } from "../ui/icons";
import { HistoryView } from "./HistoryView";
import { RadarView } from "./RadarView";
import { SettingsView } from "./SettingsView";

export interface Tab {
  id: ViewName;
  /** Desktop tab strip. */
  label: string;
  /** Bottom tab bar, where horizontal room is scarce. */
  short: string;
  icon: IconName;
}

export const TABS: Tab[] = [
  { id: "radar", label: "雷达", short: "雷达", icon: "radio" },
  { id: "history", label: "传输记录", short: "记录", icon: "clock" },
  { id: "settings", label: "设置", short: "设置", icon: "sliders" },
];

export function CurrentView({ app }: { app: NeloaState }) {
  if (app.view === "history") return <HistoryView app={app} />;
  if (app.view === "settings") return <SettingsView app={app} />;
  return <RadarView app={app} />;
}
