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
  if (platform === "macos") return "laptop";
  if (platform === "windows") return "desktop";
  if (platform === "ios" || platform === "android") return "phone";
  return "device";
}
