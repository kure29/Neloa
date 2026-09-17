---
title: Layout and commands
---

# Layout and commands

```text
src/                         React interface and the Tauri bridge
src-tauri/                   Rust client core and native projects
relay/                       Self-hosted relay service and Docker Compose
crates/neloa-relay-protocol/ Protocol shared by the client and the relay
docs/                        Documentation site (VitePress)
.github/workflows/           Continuous integration and installer builds
```

## The interface layer

```text
src/
  bridge.ts        Typed Tauri commands/events, plus the browser preview simulation
  lib/useNeloa.ts  All application state and actions; both shells consume it
  ui/              Icons, primitives, overlays
  views/           RadarView | HistoryView | SettingsView — shell-agnostic
  shell/           DesktopShell (title bar) | MobileShell (tab bar, safe areas)
  styles/          tokens | base | components | desktop | mobile
```

Nothing in `views/` or `ui/` branches on platform, with one exception: `useShell()` decides whether a modal renders as a centred dialog on desktop or a bottom sheet on mobile. The shell is chosen in the front end from `?platform=`, pointer type, viewport width, and user agent; the Rust backend reports `macos`, `windows`, `linux`, `ios`, or `android`, and protocol behaviour stays shell-independent.

Interface colour comes only from the custom properties in `src/styles/tokens.css`. Every variable has a light and a dark value, so component rules never branch on the colour scheme. The documentation site reuses the same tokens.

## Where the source of truth lives

Each topic has exactly one original, and the manual does not copy it:

| Topic | Source of truth |
| --- | --- |
| User documentation (install, pairing, transfer, clipboard, relay, troubleshooting) | `docs/guide/` |
| Architecture and threat model | `docs/en/reference/architecture.md` (English source) |
| Relay deployment and wire protocol | `relay/README.md` |
| Mobile builds and acceptance | `docs/build/mobile.md` (Chinese source) |
| Windows build | `docs/build/windows.md` (Chinese source) |

Architecture, mobile, and Windows build sources now live directly at their VitePress routes instead of being pulled through wrapper files. The relay reference still includes `relay/README.md`, avoiding a second copy of its protocol documentation.

## Previewing the documentation locally

```bash
npm run docs:dev      # http://localhost:5173/Neloa/ and /Neloa/en/
npm run docs:build    # static output in docs/.vitepress/dist
npm run docs:preview  # preview the built output
```
