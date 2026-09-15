import type { ReactNode } from "react";

/**
 * The whole icon set, drawn on a 20x20 grid with a 1.6 stroke so weights stay
 * even next to 13px text. Everything inherits currentColor.
 *
 * This replaces the text glyphs the interface used to lean on (▦ ⌘ ◇ ↗ ⌾ ⌁ ∅ ＋ ↻),
 * which never aligned to the baseline and rendered differently on every platform.
 */
export type IconName =
  | "laptop"
  | "desktop"
  | "phone"
  | "device"
  | "apple"
  | "windows"
  | "android"
  | "linux"
  | "debian"
  | "ubuntu"
  | "fedora"
  | "arch"
  | "manjaro"
  | "opensuse"
  | "linuxmint"
  | "redhat"
  | "scan"
  | "plus"
  | "close"
  | "check"
  | "sent"
  | "received"
  | "shield"
  | "key"
  | "clipboard"
  | "pulse"
  | "radio"
  | "clock"
  | "sliders"
  | "chevron"
  | "alert"
  | "trash"
  | "download"
  | "file"
  | "link"
  | "lock"
  | "text"
  | "noTrace"
  | "more"
  | "info"
  | "minimize"
  | "maximize";

const PATHS: Record<IconName, ReactNode> = {
  laptop: (
    <>
      <rect x="3.5" y="4.5" width="13" height="9" rx="1.6" />
      <path d="M2 16.5h16" />
    </>
  ),
  desktop: (
    <>
      <rect x="2.5" y="3.5" width="15" height="10" rx="1.6" />
      <path d="M10 13.5V17M7 17h6" />
    </>
  ),
  phone: (
    <>
      <rect x="5.5" y="2.5" width="9" height="15" rx="2.2" />
      <path d="M8.75 14.9h2.5" />
    </>
  ),
  device: (
    <>
      <rect x="3.5" y="3.5" width="13" height="13" rx="2.6" />
      <circle cx="10" cy="10" r="2.3" />
    </>
  ),
  apple: (
    <g transform="scale(.8333)">
      <path
        fill="currentColor"
        stroke="none"
        d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.32.03-1.75-.79-3.27-.79-1.53 0-2 .77-3.25.82-1.3.05-2.3-1.32-3.14-2.53-1.71-2.48-3.03-7-1.26-10.05.87-1.52 2.43-2.48 4.1-2.51 1.28-.03 2.49.87 3.27.87.78 0 2.26-1.07 3.81-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.8M13 3.5C13.73 2.67 14.94 2.04 15.94 2c.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z"
      />
    </g>
  ),
  windows: (
    <>
      <path fill="currentColor" stroke="none" d="M2.5 3.6l6.5-.9v6.6H2.5zM10.1 2.55l7.4-1.05v7.8h-7.4zM2.5 10.5H9v6.65l-6.5-.9zM10.1 10.5h7.4v7.95l-7.4-1.1z" />
    </>
  ),
  android: (
    <>
      <path d="M5.2 8.1h9.6v6.4a1.4 1.4 0 0 1-1.4 1.4H6.6a1.4 1.4 0 0 1-1.4-1.4z" />
      <path d="M6 8.1a4 4 0 0 1 8 0M7 3.4 5.8 1.9M13 3.4l1.2-1.5M3.2 8.9v4.6M16.8 8.9v4.6M7.4 15.9v2.2M12.6 15.9v2.2" />
      <circle cx="7.8" cy="5.8" r=".55" fill="currentColor" stroke="none" />
      <circle cx="12.2" cy="5.8" r=".55" fill="currentColor" stroke="none" />
    </>
  ),
  linux: (
    <>
      <path d="M10 2.4c-2 0-3.3 1.7-3.3 4 0 1.2-.4 2.2-1.1 3.4-.9 1.5-1.4 3.3-.7 4.7.5 1 1.7 1.2 2.9.8.6 1 1.3 1.6 2.2 1.6s1.6-.6 2.2-1.6c1.2.4 2.4.2 2.9-.8.7-1.4.2-3.2-.7-4.7-.7-1.2-1.1-2.2-1.1-3.4 0-2.3-1.3-4-3.3-4z" />
      <circle cx="8.5" cy="6.3" r=".55" fill="currentColor" stroke="none" />
      <circle cx="11.5" cy="6.3" r=".55" fill="currentColor" stroke="none" />
      <path d="m8.7 8.1 1.3.8 1.3-.8M7.8 15.3l-2 2M12.2 15.3l2 2" />
    </>
  ),
  debian: (
    <>
      <path strokeWidth="1.8" d="M16.3 7.8c.1-3.2-2.5-5.7-5.8-5.6-4 .1-6.9 3.2-6.7 7 .2 4.3 4.2 7.1 8.2 6.5 3.1-.5 4.9-3 4.3-5.5-.5-2.1-2.5-3.4-4.5-2.9-1.7.4-2.7 1.9-2.3 3.3.3 1.1 1.4 1.8 2.4 1.5.8-.2 1.2-.9 1-1.5-.1-.5-.6-.8-1.1-.6" />
      <path d="M5.2 14.6c1.8 2.2 4.9 3.2 7.9 2.5" />
    </>
  ),
  ubuntu: (
    <>
      <path strokeWidth="2" d="M10 5.6a4.4 4.4 0 0 1 3.8 2.2M13.8 12.2A4.4 4.4 0 0 1 10 14.4M6.2 12.2A4.4 4.4 0 0 1 6.2 7.8" />
      <circle cx="10" cy="2.7" r="1.45" fill="currentColor" stroke="none" />
      <circle cx="3.5" cy="13.75" r="1.45" fill="currentColor" stroke="none" />
      <circle cx="16.5" cy="13.75" r="1.45" fill="currentColor" stroke="none" />
    </>
  ),
  fedora: (
    <>
      <circle cx="10" cy="10" r="7.4" />
      <path strokeWidth="1.9" d="M12.8 5.3h-1.4a2.8 2.8 0 0 0-2.8 2.8v5.4a1.8 1.8 0 0 1-1.8 1.8H5.7M6.2 10h6.1M12.3 7.4v5.2" />
    </>
  ),
  arch: (
    <path
      fill="currentColor"
      stroke="none"
      d="M10 1.8 2.1 17.7c1.7-1 3.6-1.8 5.7-2.1L10 10l2.2 5.6c2.1.3 4 1.1 5.7 2.1zM10 6.2l1.2 2.9-1.2-.7-1.2.7z"
    />
  ),
  manjaro: (
    <path
      fill="currentColor"
      stroke="none"
      d="M2.4 2.4h15.2v4.2H8.3v11H2.4zM9.8 8.2h3.1v9.4H9.8zM14.5 8.2h3.1v9.4h-3.1z"
    />
  ),
  opensuse: (
    <>
      <path d="M17.4 8.3c-.5-3.2-3.4-5.5-6.8-5.3-4 .2-7.2 3.2-7.5 7.1-.2 2.7 1.3 5.2 3.6 6.4.8.4 1.9.2 2.4-.5.7-.8.4-2-.5-2.4-.8-.4-1.8 0-2.2.8" />
      <path d="M7 9.4c1.2-1.9 3.8-2.8 6-1.8 1.3.6 2.2 1.7 2.5 3" />
      <circle cx="13.9" cy="7.7" r=".7" fill="currentColor" stroke="none" />
    </>
  ),
  linuxmint: (
    <>
      <rect x="2.5" y="2.5" width="15" height="15" rx="3.2" />
      <path strokeWidth="1.8" d="M6 5.8v7.1c0 .8.6 1.4 1.4 1.4h5.2c.8 0 1.4-.6 1.4-1.4V8.5M9.9 8.3v6M9.9 9.6c.6-.8 1.4-1.2 2.3-1.2.7 0 1.3.3 1.8.8" />
    </>
  ),
  redhat: (
    <>
      <path fill="currentColor" stroke="none" d="M6.1 10.9c.4-2 1.2-4.6 2-6.1.5-1 1.5-1.6 2.6-1.3l2.6.7c.7.2 1.2.8 1.2 1.5l.2 3.5c1.1.4 2 .9 2.7 1.5-2 1.1-4.8 1.7-7.8 1.7-3 0-5.7-.5-7.4-1.5.8-.3 2.1-.3 3.9 0z" />
      <path d="M2.6 12.6c.5 2.3 3.6 4 7.4 4s6.9-1.7 7.4-4" />
    </>
  ),
  scan: (
    <>
      <path d="M17.5 10a7.5 7.5 0 1 1-2.2-5.3l2.2 2" />
      <path d="M17.5 2.5v4.2h-4.2" />
    </>
  ),
  plus: <path d="M10 4.5v11M4.5 10h11" />,
  close: <path d="M5.2 5.2l9.6 9.6M14.8 5.2l-9.6 9.6" />,
  check: <path d="M4.5 10.4l3.6 3.6 7.4-8" />,
  sent: (
    <>
      <path d="M6 14L14 6" />
      <path d="M7.6 6H14v6.4" />
    </>
  ),
  received: (
    <>
      <path d="M14 6L6 14" />
      <path d="M12.4 14H6V7.6" />
    </>
  ),
  shield: <path d="M10 2.4l6.2 2.3v4.6c0 3.9-2.5 6.8-6.2 8.4-3.7-1.6-6.2-4.5-6.2-8.4V4.7z" />,
  key: (
    <>
      <circle cx="7.3" cy="7.3" r="3.8" />
      <path d="M10 10l6 6" />
      <path d="M13.5 12.1l-1.7 1.7" />
    </>
  ),
  clipboard: (
    <>
      <path d="M7.6 3.6H6.4a2 2 0 0 0-2 2v9.9a2 2 0 0 0 2 2h7.2a2 2 0 0 0 2-2V5.6a2 2 0 0 0-2-2h-1.2" />
      <rect x="7.6" y="2" width="4.8" height="3.2" rx="1.1" />
    </>
  ),
  pulse: <path d="M2 10h3.6l2.4-5.8 4 11.6 2.4-5.8H18" />,
  radio: (
    <>
      <circle cx="10" cy="10" r="1.8" />
      <path d="M6.6 13.4a4.8 4.8 0 0 1 0-6.8M13.4 6.6a4.8 4.8 0 0 1 0 6.8" />
      <path d="M4.2 15.8a8.2 8.2 0 0 1 0-11.6M15.8 4.2a8.2 8.2 0 0 1 0 11.6" />
    </>
  ),
  clock: (
    <>
      <circle cx="10" cy="10" r="7.3" />
      <path d="M10 5.8V10l2.8 1.8" />
    </>
  ),
  sliders: (
    <>
      <path d="M3.5 6.5h8M15.5 6.5h1M3.5 13.5h1M8.5 13.5h8" />
      <circle cx="13.5" cy="6.5" r="2" />
      <circle cx="6.5" cy="13.5" r="2" />
    </>
  ),
  chevron: <path d="M8 5.2l4.8 4.8L8 14.8" />,
  alert: (
    <>
      <path d="M8.7 3.6a1.5 1.5 0 0 1 2.6 0l5.9 10.3a1.5 1.5 0 0 1-1.3 2.25H4.1a1.5 1.5 0 0 1-1.3-2.25z" />
      <path d="M10 7.8v3.4" />
      <circle cx="10" cy="13.7" r="0.85" fill="currentColor" stroke="none" />
    </>
  ),
  trash: (
    <>
      <path d="M3.6 5.4h12.8" />
      <path d="M8 5.4V4.1a1.5 1.5 0 0 1 1.5-1.5h1a1.5 1.5 0 0 1 1.5 1.5v1.3" />
      <path d="M5.6 5.4l.7 10a1.5 1.5 0 0 0 1.5 1.4h4.4a1.5 1.5 0 0 0 1.5-1.4l.7-10" />
    </>
  ),
  download: (
    <>
      <path d="M10 2.6v9.6" />
      <path d="M6.2 8.6L10 12.4l3.8-3.8" />
      <path d="M3.6 14.4v1.5a1.5 1.5 0 0 0 1.5 1.5h9.8a1.5 1.5 0 0 0 1.5-1.5v-1.5" />
    </>
  ),
  file: (
    <>
      <path d="M11.6 2.6H6.5a2 2 0 0 0-2 2v10.8a2 2 0 0 0 2 2h7a2 2 0 0 0 2-2V6.6z" />
      <path d="M11.6 2.6v4h4" />
    </>
  ),
  link: (
    <>
      <path d="M8.4 11.6a3.6 3.6 0 0 0 5.1 0l2.2-2.2a3.6 3.6 0 1 0-5.1-5.1l-1.2 1.2" />
      <path d="M11.6 8.4a3.6 3.6 0 0 0-5.1 0l-2.2 2.2a3.6 3.6 0 0 0 5.1 5.1l1.2-1.2" />
    </>
  ),
  lock: (
    <>
      <rect x="4.4" y="8.4" width="11.2" height="8.6" rx="2.1" />
      <path d="M6.9 8.4V6.1a3.1 3.1 0 0 1 6.2 0v2.3" />
    </>
  ),
  text: (
    <>
      <path d="M4 6.2V4.6h12v1.6" />
      <path d="M10 4.6v10.8" />
      <path d="M7.4 15.4h5.2" />
    </>
  ),
  noTrace: (
    <>
      <circle cx="10" cy="10" r="7.2" />
      <path d="M5.1 14.9L14.9 5.1" />
    </>
  ),
  more: (
    <>
      <circle cx="4.2" cy="10" r="1.15" fill="currentColor" stroke="none" />
      <circle cx="10" cy="10" r="1.15" fill="currentColor" stroke="none" />
      <circle cx="15.8" cy="10" r="1.15" fill="currentColor" stroke="none" />
    </>
  ),
  info: (
    <>
      <circle cx="10" cy="10" r="7.2" />
      <path d="M10 8.8v5M10 6.2h.01" />
    </>
  ),
  minimize: <path d="M4.5 10h11" />,
  maximize: <rect x="5" y="5" width="10" height="10" rx="1.6" />,
};

export function Icon({
  name,
  size = 18,
  className,
}: {
  name: IconName;
  size?: number;
  className?: string;
}) {
  return (
    <svg
      className={className}
      viewBox="0 0 20 20"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {PATHS[name]}
    </svg>
  );
}

/** Peer platforms arrive as free-form strings from mDNS, so this must not throw. */
export function deviceIcon(platform: string): IconName {
  const normalized = platform.toLowerCase();
  if (normalized === "macos" || normalized === "ios") return "apple";
  if (normalized === "windows") return "windows";
  if (normalized === "android") return "android";
  if (normalized === "debian") return "debian";
  if (normalized === "ubuntu") return "ubuntu";
  if (normalized === "fedora") return "fedora";
  if (normalized === "arch") return "arch";
  if (normalized === "manjaro") return "manjaro";
  if (normalized === "opensuse") return "opensuse";
  if (normalized === "linuxmint") return "linuxmint";
  if (normalized === "redhat") return "redhat";
  if (normalized === "linux") return "linux";
  return "device";
}
