import {
  createContext,
  useContext,
  useEffect,
  useRef,
  type ButtonHTMLAttributes,
  type ReactNode,
} from "react";

import type { Shell } from "../types";
import { Icon, deviceIcon, type IconName } from "./icons";

const ShellContext = createContext<Shell>("desktop");
export const ShellProvider = ShellContext.Provider;
export const useShell = () => useContext(ShellContext);

export type Tone = "neutral" | "ok" | "warn" | "danger" | "relay";

/* ---------- buttons ---------- */

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "danger" | "ghost";
  size?: "md" | "sm";
}

export function Button({ variant = "secondary", size = "md", className, ...rest }: ButtonProps) {
  return <button className={cx("btn", `btn-${variant}`, `btn-${size}`, className)} {...rest} />;
}

export function IconButton({
  icon,
  label,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { icon: IconName; label: string }) {
  return (
    <button className={cx("icon-btn", className)} aria-label={label} title={label} {...rest}>
      <Icon name={icon} />
    </button>
  );
}

/* ---------- state indicators ---------- */

export function Badge({ tone = "neutral", children }: { tone?: Tone; children: ReactNode }) {
  return <span className={cx("badge", `tone-${tone}`)}>{children}</span>;
}

export function StatusDot({ tone = "neutral", className }: { tone?: Tone; className?: string }) {
  return <span className={cx("status-dot", `tone-${tone}`, className)} aria-hidden="true" />;
}

export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: () => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className={cx("switch", checked && "on")}
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={onChange}
    >
      <span className="switch-knob" />
    </button>
  );
}

/* ---------- structure ---------- */

export function SectionTitle({
  title,
  meta,
  action,
}: {
  title: string;
  meta?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="section-title">
      <h2>{title}</h2>
      {meta !== undefined && <span className="section-meta">{meta}</span>}
      {action}
    </div>
  );
}

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cx("card", className)}>{children}</div>;
}

export function EmptyState({
  icon,
  title,
  description,
}: {
  icon: IconName;
  title: string;
  description: string;
}) {
  return (
    <div className="empty-state">
      <span className="empty-state-icon">
        <Icon name={icon} size={22} />
      </span>
      <strong>{title}</strong>
      <p>{description}</p>
    </div>
  );
}

/**
 * A rounded device tile. `online`, `trusted` and `incompatible` are drawn as
 * small marks on the corner rather than by recolouring the whole tile, so the
 * device stays recognisable in every state.
 */
export function DeviceAvatar({
  platform,
  size = 44,
  online,
  trusted,
  incompatible,
}: {
  platform: string;
  size?: number;
  online?: boolean;
  trusted?: boolean;
  incompatible?: boolean;
}) {
  const normalizedPlatform = platform.toLowerCase();
  const platformClass = [
    "macos",
    "ios",
    "windows",
    "android",
    "linux",
    "debian",
    "ubuntu",
    "fedora",
    "arch",
    "manjaro",
    "opensuse",
    "linuxmint",
    "redhat",
  ].includes(normalizedPlatform)
    ? `platform-${normalizedPlatform}`
    : "platform-other";

  return (
    <span
      className={cx(
        "device-avatar",
        platformClass,
        incompatible && "incompatible",
        trusted && "trusted",
      )}
      style={{ width: size, height: size }}
    >
      <Icon name={deviceIcon(platform)} size={Math.round(size * 0.48)} />
      {online && <i className="device-online" aria-hidden="true" />}
      {trusted && (
        <i className="device-trusted" aria-hidden="true">
          <Icon name="check" size={10} />
        </i>
      )}
      {incompatible && (
        <i className="device-incompatible" aria-hidden="true">
          <Icon name="alert" size={10} />
        </i>
      )}
    </span>
  );
}

/**
 * Modal surface. Centred dialog on desktop, bottom sheet on mobile — same
 * markup, same children, so the pairing and file-offer flows have one
 * implementation across all three platforms.
 *
 * There is deliberately no dismiss-on-backdrop: both flows are security
 * decisions that must be answered explicitly.
 */
export function Sheet({
  labelledBy,
  className,
  children,
}: {
  labelledBy: string;
  className?: string;
  children: ReactNode;
}) {
  const shell = useShell();
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    cardRef.current?.focus();
  }, []);

  return (
    <div className={cx("scrim", shell === "mobile" && "scrim-bottom")}>
      <div
        ref={cardRef}
        className={cx("sheet", className)}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        tabIndex={-1}
      >
        {shell === "mobile" && <span className="sheet-grip" aria-hidden="true" />}
        {children}
      </div>
    </div>
  );
}

export function cx(...values: Array<string | false | null | undefined>): string {
  return values.filter(Boolean).join(" ");
}
