---
title: Clipboard sync
---

# Clipboard sync

Clipboard sync is **off** by default. That is a deliberate default: clipboard content tends to be more sensitive than people expect.

## Turning it on

Open Settings → Clipboard sync and switch on "Automatically sync newly copied text".

Enabling it records the current clipboard as a local baseline, but does **not** send that existing content anywhere. Only text copied after enabling is synced.

## The limits

| Constraint | Value |
| --- | --- |
| Content type | Plain text only |
| Per-item limit | 8 KB |
| Sensitive content | Blocked on common key and credential markers such as `-----BEGIN PRIVATE KEY-----` |
| Persistence | The body is never written to history or to a diagnostics report |
| Targets | Only currently online, already paired devices |

Text that exceeds the size limit or matches a sensitive-content marker **stays on this device** and is not sent.

## Loops and duplicates

Received content is not immediately sent back: once the remote write succeeds, this device treats it as the new local baseline. To compare across platforms, CRLF, CR, and LF are normalised before comparison.

## Mobile limits

Android 10 and later restrict reading the clipboard in the background, and iOS may show a system paste permission prompt. The product definition for mobile is therefore: **sync only while the app is in the foreground**. Mobile systems will not grant desktop-style unlimited background polling.

## The most recent event

Settings shows the status, peer, byte count, and time of the last clipboard event directly beneath the switch. If sync is not behaving as expected, start there — it distinguishes "nothing new" from "it went out and failed".
