---
title: Pairing and device trust
---

# Pairing and device trust

Neloa never establishes trust in the cloud. Two devices must each confirm once, and only then do they pin each other's identity keys.

## What happens while pairing

1. The two devices perform a Noise XX handshake and negotiate protocol versions and capabilities.
2. Both sides derive a **six-digit number** from the same handshake transcript.
3. The numbers must match. You confirm on both devices separately.
4. Only when both sides confirm is the other device's public key written into the local trust store.

This number is not a password and does not need to be kept secret. Its job is to make a substituted connection visible: if the two devices show different numbers, you are talking to a different device than you think.

The pairing screen also shows the peer's **key fingerprint**. The code matching and the fingerprint matching is what earns trust.

::: warning When the numbers differ
Do not confirm. Cancel the pairing, check that you selected the device you meant to, and start again. A mismatch usually means the wrong device was selected rather than that someone is attacking you — but the outcome is the same: this connection should not be trusted.
:::

## After pairing

- Later sessions must match both the device ID and the stored public key, or the handshake is rejected.
- Paired devices show a route marker on the devices screen: **local network** or **relay**.
- Pairing survives a restart; you do not compare the code again.
- The relay takes no part in pairing: crossing networks requires a pairing that was completed over a local network first.

## Device aliases

You can give any device a local alias, which is useful when two machines share a name such as "MacBook".

- On the devices screen, open the more button on a paired device to set the alias.
- Aliases are limited to 32 characters and may not contain control characters.
- **An alias is stored only on this device** and never renames the other machine.

The device's own name is edited under Settings → This device, and that is the name other devices see.

## Revoking trust

Under Settings → Paired devices, press Remove on a device to revoke trust. Afterwards:

- The device disappears from your trusted list and can no longer send you files or clipboard content.
- Re-establishing trust requires both devices to go through the six-digit flow again.

::: tip Deciding whether to revoke
A device that changed hands, was reinstalled, or that you are no longer sure about should be revoked and paired again. **Pairing is the entire basis of trust**, and revoking it empties that relationship.
:::
