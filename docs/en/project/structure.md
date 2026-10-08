---
title: Layout and commands
---

# Layout and commands

```text
src/                         React interface and the Tauri bridge
src-tauri/                   Rust client core and native projects
relay/                       Self-hosted relay service and Docker Compose
relay-worker/                Cloudflare Workers / Durable Objects relay
crates/neloa-relay-protocol/ Protocol shared by the client and the relay
docs/                        Documentation site (VitePress)
.github/workflows/           Continuous integration and installer builds
```

## The interface layer

```text
src/
  bridge.ts        Typed Tauri commands/events, plus the browser preview simulation
  lib/useNeloa.ts  All application state and actions; both shells consume it
  ui/              Icons, primitives, overlays, the route picker
  views/           DevicePane (device list and detail) | MobileDevicesView (phone device page) | HistoryView | SettingsView
  shell/           DesktopShell (full-height sidebar: devices and navigation) | MobileShell (tab bar, safe areas)
  styles/          tokens | base | components | desktop | mobile
```

The device page differs by shell in layout, not content: on desktop the sidebar lists devices and `DevicePane` shows the selected one; on phones `MobileDevicesView` shows the same list and pushes the same device detail as its own screen. `views/index.tsx` picks between them. History and settings are shared. Beyond that, nothing in `views/` or `ui/` branches on platform, and `useShell()` decides whether a modal renders as a centred dialog on desktop or a bottom sheet on mobile. The shell is chosen in the front end from `?platform=`, pointer type, viewport width, and user agent; the Rust backend reports `macos`, `windows`, `linux`, `ios`, or `android`, and protocol behaviour stays shell-independent.

Interface colour comes only from the custom properties in `src/styles/tokens.css`. Every variable has a light and a dark value, so component rules never branch on the colour scheme. The documentation site reuses the same tokens.

## Where the source of truth lives

Each topic has exactly one original, and the manual does not copy it:

| Topic | Source of truth |
| --- | --- |
| User documentation (install, pairing, transfer, clipboard, relay, troubleshooting) | `docs/guide/` |
| Architecture and threat model | `docs/en/reference/architecture.md` (English source) |
| Rust relay deployment and wire protocol | `relay/README.md` |
| Cloudflare Workers relay | `relay-worker/README.md` and `docs/en/guide/relay-cloudflare.md` |
| Mobile builds and acceptance | `docs/build/mobile.md` (Chinese source) |
| Windows build | `docs/build/windows.md` (Chinese source) |

Architecture, mobile, and Windows build sources now live directly at their VitePress routes instead of being pulled through wrapper files. The relay reference still includes `relay/README.md`, avoiding a second copy of its protocol documentation.

## Previewing the documentation locally

```bash
npm run docs:dev      # http://localhost:5173/Neloa/ and /Neloa/en/
npm run docs:build    # static output in docs/.vitepress/dist
npm run docs:preview  # preview the built output
```
