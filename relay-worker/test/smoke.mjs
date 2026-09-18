import assert from "node:assert/strict";

import WebSocket from "ws";

const url = process.env.NELOA_RELAY_TEST_URL ?? "ws://127.0.0.1:8788/v1/ws";
const token = process.env.NELOA_RELAY_TEST_TOKEN ?? "test-only-relay-token-32-characters";
const tunnelId = "00112233-4455-6677-8899-aabbccddeeff";

function device(id, name) {
  return {
    id,
    name,
    platform: "smoke-test",
    appVersion: "0.1.13",
    protocolVersion: 1,
    minProtocolVersion: 1,
    capabilities: ["pairing", "noise-xx"],
  };
}

function tunnelFrame(id, payload) {
  const hex = id.replaceAll("-", "");
  const frame = Buffer.alloc(16 + payload.byteLength);
  for (let index = 0; index < 16; index += 1) {
    frame[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  Buffer.from(payload).copy(frame, 16);
  return frame;
}

class Client {
  constructor(id, name) {
    this.id = id;
    this.name = name;
    this.inbox = [];
    this.waiters = [];
  }

  async connect() {
    this.socket = new WebSocket(url, { headers: { authorization: `Bearer ${token}` } });
    this.socket.on("message", (data, isBinary) => {
      const value = isBinary ? Buffer.from(data) : JSON.parse(data.toString());
      const waiterIndex = this.waiters.findIndex(({ predicate }) => predicate(value));
      if (waiterIndex >= 0) {
        const [{ resolve, timer }] = this.waiters.splice(waiterIndex, 1);
        clearTimeout(timer);
        resolve(value);
      } else {
        this.inbox.push(value);
      }
    });
    await new Promise((resolve, reject) => {
      this.socket.once("open", resolve);
      this.socket.once("error", reject);
    });
    this.send({
      type: "register",
      relayProtocolVersion: 1,
      device: device(this.id, this.name),
    });
    await this.waitFor((message) => message.type === "registered");
  }

  send(value) {
    this.socket.send(Buffer.isBuffer(value) ? value : JSON.stringify(value));
  }

  waitFor(predicate, timeoutMs = 5_000) {
    const index = this.inbox.findIndex(predicate);
    if (index >= 0) return Promise.resolve(this.inbox.splice(index, 1)[0]);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const waiterIndex = this.waiters.findIndex((waiter) => waiter.timer === timer);
        if (waiterIndex >= 0) this.waiters.splice(waiterIndex, 1);
        reject(new Error(`timed out waiting for relay message on ${this.id}`));
      }, timeoutMs);
      this.waiters.push({ predicate, resolve, timer });
    });
  }

  close() {
    this.socket.close();
  }
}

const first = new Client("smoke-device-a", "Phone");
const second = new Client("smoke-device-b", "Laptop");

try {
  await first.connect();
  await second.connect();
  await second.waitFor((message) => (
    message.type === "presence" && message.devices.some(({ id }) => id === first.id)
  ));

  first.send({ type: "openTunnel", tunnelId, targetId: second.id });
  const [opened, incoming] = await Promise.all([
    first.waitFor((message) => message.type === "tunnelOpened" && message.tunnelId === tunnelId),
    second.waitFor((message) => message.type === "incomingTunnel" && message.tunnelId === tunnelId),
  ]);
  assert.equal(opened.target.id, second.id);
  assert.equal(incoming.source.id, first.id);

  const opaque = Buffer.from("opaque Noise ciphertext");
  const frame = tunnelFrame(tunnelId, opaque);
  first.send(frame);
  const forwarded = await second.waitFor(Buffer.isBuffer);
  assert.deepEqual(forwarded, frame);

  first.send({ type: "closeTunnel", tunnelId });
  await Promise.all([
    first.waitFor((message) => message.type === "tunnelClosed" && message.tunnelId === tunnelId),
    second.waitFor((message) => message.type === "tunnelClosed" && message.tunnelId === tunnelId),
  ]);
  console.log("Cloudflare relay smoke test passed");
} finally {
  first.close();
  second.close();
}
