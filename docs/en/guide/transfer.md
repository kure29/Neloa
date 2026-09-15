---
title: Sending and receiving files
---

# Sending and receiving files

## Sending

1. On the devices screen, select a **paired** device. The selected device gets a check mark.
2. Press Select files, or drag files straight into the window (macOS and Windows accept multiple files).
3. The pending list shows each file's name and size, and any file can be removed individually.
4. Press send.

The line above the send button tells you what this step will actually do, for example "sending 24.6 MB to Surface Laptop, end-to-end encrypted". When the button is unavailable, the same line explains why.

## Receiving

The receiving side never saves anything automatically. Every incoming file shows a confirmation surface with:

- the sending device's name
- the file name and size
- the file digest (SHA-256)

The transfer only starts once you choose Accept and save. Choosing Reject ends this transfer immediately and tells the sender.

## Where files land

An incoming file is first written to a temporary file and only published to the `Neloa` folder inside your system Downloads folder **after verification passes**. Publication never overwrites an existing file — a name collision produces another name instead.

If a transfer is interrupted, cancelled, or rejected, its temporary file is removed, so a half-written file never stays on disk.

## Queue, cancel, retry

- You can select many files at once; they enter a send queue.
- Active transfers appear in a floating panel with direction, stage, percentage, transferred bytes, and current speed.
- Every transfer can be cancelled on its own.
- A send that failed or was interrupted keeps a Retry button in [transfer history](/en/guide/transfer#transfer-history).

The transfer stage names what is happening right now: computing a checksum, waiting for the peer to accept, encrypting and sending, receiving and decrypting, verifying and writing.

## Integrity verification

Before sending, the sender computes a SHA-256 for each file and offers the name, byte size, and digest together. The receiver verifies the digest again after writing, and only then publishes the final file.

Which means: **"completed" means completed after verification.**

## Transfer history

The history screen keeps recent results with direction, file name, peer device, size, time, and status (completed / failed / cancelled / rejected). Plain-text clipboard events are recorded there too, but only the peer, direction, byte count, and time are kept — never the body.
