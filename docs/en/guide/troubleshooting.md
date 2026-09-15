---
title: Troubleshooting
---

# Troubleshooting

Start with the summary at the top of Settings → Connection. It reports either "connection is healthy" or "N items need attention", lists the items that need attention, and gives a suggested action for each.

## Diagnostics

Expand Settings → Connection → Technical details to see six checks:

| Check | What it covers |
| --- | --- |
| Local network | Local interfaces and the QUIC listener |
| Device discovery | Whether mDNS/Bonjour is working |
| Device identity protection | Whether the long-term device key can be read from the system credential store |
| Client version | Current version and protocol capabilities |
| Clipboard sync | Whether it is enabled and usable |
| Self-hosted relay | Whether it is enabled and connected |

Copy diagnostics at the bottom generates a report that **contains no clipboard bodies**, suitable for pasting into an issue.

## No devices found

1. Confirm both devices are on the same network and both have Neloa open.
2. Look at the devices screen hint. "Searching for nearby and relay devices" means discovery is still running; "device discovery is unavailable" means it is not, so check Device discovery in the diagnostics.
3. On Windows, check whether the firewall allows Neloa on private and public networks.
4. iOS asks for Local Network permission on first run; if it was declined, re-enable it in system settings.
5. Android needs working Wi-Fi. Neloa holds the mDNS multicast lock while the app is alive, but local discovery cannot work when the device is only on cellular.

## A device shows "incompatible version"

The two protocol ranges do not overlap, or the peer lacks a capability this operation needs. Updating the other device's Neloa resolves it. Older builds are treated as protocol 0 and receive an actionable notice instead of entering a pairing or transfer flow.

## A transfer fails or stalls

- Cancel it in the transfer panel and send again.
- Failed sends keep a Retry button in the history screen.
- For large files, confirm neither device went to sleep and the network did not change mid-transfer.
- If the sender stays on "waiting for the peer to accept", the receiver has not confirmed that file — the receiving side never saves automatically.

## The relay will not connect

- Confirm the address starts with `wss://` and ends with `/v1/ws`.
- Confirm both devices use exactly the same token.
- Confirm the reverse proxy forwards HTTPS to `127.0.0.1:8787` with a valid certificate.
- Check the Self-hosted relay entry under Technical details. Even when the relay fails, local transfers keep working.

## Clipboard sync does nothing

- Confirm the switch is on and that the most recent event below it shows a record.
- Text above 8 KB, or matching a sensitive-content marker, **is not sent by design**.
- On mobile, sync only runs while the app is in the foreground.
- Content already on the clipboard when you enable the feature is not sent; only newly copied text is.
