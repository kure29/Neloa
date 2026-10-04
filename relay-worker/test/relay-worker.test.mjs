import assert from "node:assert/strict";
import test from "node:test";

import { RelayObject, requestIsAuthorized } from "../src/index.mjs";
import {
  MAX_RELAY_PAYLOAD,
  parseRegistration,
  tunnelIdFromFrame,
  validateDevice,
} from "../src/protocol.mjs";

const TOKEN = "test-only-relay-token-32-characters";
const TUNNEL_ID = "00112233-4455-6677-8899-aabbccddeeff";

function device(id, name) {
  return {
    id,
    name,
    platform: "test",
    appVersion: "0.1.13",
    protocolVersion: 1,
    minProtocolVersion: 1,
    capabilities: ["pairing", "noise-xx"],
  };
}

function registerMessage(id, name) {
  return JSON.stringify({
    type: "register",
    relayProtocolVersion: 1,
    device: device(id, name),
  });
}

function tunnelFrame(id, payload) {
  const hex = id.replaceAll("-", "");
  const frame = new Uint8Array(16 + payload.byteLength);
  for (let index = 0; index < 16; index += 1) {
    frame[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  frame.set(payload, 16);
  return frame.buffer;
}

class FakeSocket {
  constructor() {
    this.readyState = 1;
    this.attachment = null;
    this.sent = [];
  }

  serializeAttachment(value) {
    this.attachment = structuredClone(value);
  }

  deserializeAttachment() {
    return structuredClone(this.attachment);
  }

  send(value) {
    this.sent.push(value);
  }

  close() {
    this.readyState = 3;
  }

  controls() {
    return this.sent
      .filter((value) => typeof value === "string")
      .map((value) => JSON.parse(value));
  }
}

class FakeContext {
  constructor() {
    this.sockets = [];
  }

  acceptWebSocket(socket) {
    this.sockets.push(socket);
  }

  getWebSockets() {
    return this.sockets.filter((socket) => socket.readyState !== 3);
  }
}

function connectedRelay() {
  const ctx = new FakeContext();
  const relay = new RelayObject(ctx, { NELOA_RELAY_MAX_DEVICES: "64" });
  const first = new FakeSocket();
  const second = new FakeSocket();
  ctx.acceptWebSocket(first);
  ctx.acceptWebSocket(second);
  first.serializeAttachment({
    connectionId: "connection-a",
    registered: false,
    device: null,
    tunnels: {},
  });
  second.serializeAttachment({
    connectionId: "connection-b",
    registered: false,
    device: null,
    tunnels: {},
  });
  relay.webSocketMessage(first, registerMessage("device-a", "Phone"));
  relay.webSocketMessage(second, registerMessage("device-b", "Laptop"));
  return { relay, first, second };
}

test("bearer authentication requires the complete configured secret", () => {
  const authorized = new Request("https://relay.example/v1/ws", {
    headers: { authorization: `Bearer ${TOKEN}` },
  });
  const rejected = new Request("https://relay.example/v1/ws", {
    headers: { authorization: "Bearer wrong-token-with-enough-characters" },
  });
  assert.equal(requestIsAuthorized(authorized, TOKEN), true);
  assert.equal(requestIsAuthorized(rejected, TOKEN), false);
});

test("registration uses the Rust relay v1 JSON shape", () => {
  assert.deepEqual(parseRegistration(registerMessage("device-a", "Phone")), device("device-a", "Phone"));
  assert.throws(() => validateDevice({ ...device("device-a", "Phone"), capabilities: new Array(33).fill("x") }));
});

test("registered clients receive presence and can forward opaque tunnel frames", () => {
  const { relay, first, second } = connectedRelay();
  assert.equal(first.controls().some((message) => message.type === "registered"), true);
  assert.deepEqual(
    second.controls().findLast((message) => message.type === "presence").devices.map(({ id }) => id),
    ["device-a", "device-b"],
  );

  relay.webSocketMessage(first, JSON.stringify({
    type: "openTunnel",
    tunnelId: TUNNEL_ID,
    targetId: "device-b",
  }));
  assert.equal(first.controls().findLast((message) => message.type === "tunnelOpened").target.id, "device-b");
  assert.equal(second.controls().findLast((message) => message.type === "incomingTunnel").source.id, "device-a");

  const payload = new TextEncoder().encode("opaque Noise ciphertext");
  const frame = tunnelFrame(TUNNEL_ID, payload);
  assert.equal(tunnelIdFromFrame(frame), TUNNEL_ID);
  relay.webSocketMessage(first, frame);
  assert.deepEqual(new Uint8Array(second.sent.at(-1)), new Uint8Array(frame));
});

test("oversized and unauthorized tunnel frames are rejected without forwarding", () => {
  const { relay, first, second } = connectedRelay();
  const sentBefore = second.sent.length;
  relay.webSocketMessage(first, tunnelFrame(TUNNEL_ID, new Uint8Array(1)));
  assert.equal(second.sent.length, sentBefore);
  assert.equal(first.controls().at(-1).code, "tunnelNotFound");

  relay.webSocketMessage(first, new ArrayBuffer(16 + MAX_RELAY_PAYLOAD + 1));
  assert.equal(first.controls().at(-1).code, "invalidMessage");
});

test("disconnect closes the peer tunnel and removes the device from presence", () => {
  const { relay, first, second } = connectedRelay();
  relay.webSocketMessage(first, JSON.stringify({
    type: "openTunnel",
    tunnelId: TUNNEL_ID,
    targetId: "device-b",
  }));
  first.readyState = 3;
  relay.webSocketClose(first);
  assert.equal(second.controls().some((message) => (
    message.type === "tunnelClosed"
      && message.tunnelId === TUNNEL_ID
      && message.reason === "peerDisconnected"
  )), true);
  assert.deepEqual(
    second.controls().findLast((message) => message.type === "presence").devices.map(({ id }) => id),
    ["device-b"],
  );
});
