import {
  MAX_CONTROL_MESSAGE_SIZE,
  MAX_RELAY_FRAME,
  RELAY_PROTOCOL_VERSION,
  RelayProtocolError,
  errorControl,
  parseControlMessage,
  parseRegistration,
  timingSafeTextEqual,
  tunnelIdFromFrame,
  validToken,
  validateTargetId,
  validateTunnelId,
} from "./protocol.mjs";

const DEFAULT_MAX_DEVICES = 64;
const MAX_CONFIGURED_DEVICES = 4096;
const MAX_TUNNELS_PER_DEVICE = 32;
const RELAY_OBJECT_NAME = "neloa-single-user-v1";
const OPEN = 1;

function json(value, init = {}) {
  const headers = new Headers(init.headers);
  headers.set("content-type", "application/json; charset=utf-8");
  return new Response(JSON.stringify(value), { ...init, headers });
}

function configuredMaximum(value) {
  const parsed = Number.parseInt(value ?? "", 10);
  return Number.isInteger(parsed) && parsed >= 1 && parsed <= MAX_CONFIGURED_DEVICES
    ? parsed
    : DEFAULT_MAX_DEVICES;
}

export function requestIsAuthorized(request, token) {
  if (!validToken(token)) return false;
  const authorization = request.headers.get("authorization") ?? "";
  const prefix = "Bearer ";
  if (!authorization.startsWith(prefix)) return false;
  return timingSafeTextEqual(authorization.slice(prefix.length), token);
}

export function initialAttachment() {
  return {
    connectionId: crypto.randomUUID(),
    registered: false,
    device: null,
    tunnels: {},
  };
}

function readAttachment(socket) {
  const value = socket.deserializeAttachment();
  if (!value || typeof value !== "object") return initialAttachment();
  return {
    connectionId: typeof value.connectionId === "string"
      ? value.connectionId
      : crypto.randomUUID(),
    registered: value.registered === true,
    device: value.device && typeof value.device === "object" ? value.device : null,
    tunnels: value.tunnels && typeof value.tunnels === "object" ? value.tunnels : {},
  };
}

function writeAttachment(socket, state) {
  socket.serializeAttachment(state);
}

function safeSend(socket, message) {
  if (socket.readyState !== OPEN) return false;
  try {
    socket.send(typeof message === "string" || message instanceof ArrayBuffer
      ? message
      : JSON.stringify(message));
    return true;
  } catch {
    return false;
  }
}

function safeClose(socket, code, reason) {
  try {
    socket.close(code, reason);
  } catch {
    // The connection may already have completed its close handshake.
  }
}

function socketRecord(socket) {
  return { socket, state: readAttachment(socket) };
}

export class RelayObject {
  constructor(ctx, env) {
    this.ctx = ctx;
    this.env = env;
  }

  async fetch(request) {
    if ((request.headers.get("upgrade") ?? "").toLowerCase() !== "websocket") {
      return new Response("websocket upgrade required", { status: 426 });
    }
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    this.ctx.acceptWebSocket(server);
    writeAttachment(server, initialAttachment());
    return new Response(null, { status: 101, webSocket: client });
  }

  records(excluded = null) {
    return this.ctx.getWebSockets()
      .filter((socket) => socket !== excluded && socket.readyState === OPEN)
      .map(socketRecord);
  }

  registeredRecords(excluded = null) {
    return this.records(excluded).filter(({ state }) => state.registered && state.device);
  }

  findDevice(deviceId, excluded = null) {
    return this.registeredRecords(excluded)
      .find(({ state }) => state.device.id === deviceId) ?? null;
  }

  broadcastPresence(excluded = null) {
    const records = this.registeredRecords(excluded);
    const devices = records
      .map(({ state }) => state.device)
      .sort((left, right) => left.id.localeCompare(right.id));
    const message = { type: "presence", devices };
    for (const { socket } of records) safeSend(socket, message);
  }

  reject(socket, error, close = false) {
    safeSend(socket, errorControl(error));
    if (close) safeClose(socket, 1008, "relay protocol rejected the connection");
  }

  register(socket, text, state) {
    let device;
    try {
      device = parseRegistration(text);
    } catch (error) {
      this.reject(socket, error, true);
      return;
    }
    if (this.findDevice(device.id, socket)) {
      this.reject(socket, new RelayProtocolError(
        "duplicateDevice",
        "this device is already connected",
      ), true);
      return;
    }
    const maximum = configuredMaximum(this.env.NELOA_RELAY_MAX_DEVICES);
    if (this.registeredRecords(socket).length >= maximum) {
      this.reject(socket, new RelayProtocolError(
        "serverFull",
        "relay has reached its configured device limit",
      ), true);
      return;
    }
    writeAttachment(socket, {
      ...state,
      registered: true,
      device,
      tunnels: {},
    });
    safeSend(socket, {
      type: "registered",
      relayProtocolVersion: RELAY_PROTOCOL_VERSION,
    });
    this.broadcastPresence();
  }

  openTunnel(socket, state, control) {
    const tunnelId = validateTunnelId(control.tunnelId);
    const targetId = validateTargetId(control.targetId, tunnelId);
    if (targetId === state.device.id) {
      throw new RelayProtocolError(
        "invalidTarget",
        "a device cannot open a tunnel to itself",
        tunnelId,
      );
    }
    if (Object.keys(state.tunnels).length >= MAX_TUNNELS_PER_DEVICE) {
      throw new RelayProtocolError(
        "backpressure",
        "source device has too many active tunnels",
        tunnelId,
      );
    }
    if (this.registeredRecords().some(({ state: candidate }) => tunnelId in candidate.tunnels)) {
      throw new RelayProtocolError(
        "tunnelAlreadyExists",
        "tunnel id is already in use",
        tunnelId,
      );
    }
    const target = this.findDevice(targetId, socket);
    if (!target) {
      throw new RelayProtocolError("targetOffline", "target device is offline", tunnelId);
    }
    if (Object.keys(target.state.tunnels).length >= MAX_TUNNELS_PER_DEVICE) {
      throw new RelayProtocolError(
        "backpressure",
        "target device has too many active tunnels",
        tunnelId,
      );
    }
    state.tunnels[tunnelId] = targetId;
    target.state.tunnels[tunnelId] = state.device.id;
    writeAttachment(socket, state);
    writeAttachment(target.socket, target.state);

    if (!safeSend(target.socket, {
      type: "incomingTunnel",
      tunnelId,
      source: state.device,
    })) {
      delete state.tunnels[tunnelId];
      delete target.state.tunnels[tunnelId];
      writeAttachment(socket, state);
      writeAttachment(target.socket, target.state);
      throw new RelayProtocolError(
        "targetOffline",
        "target device disconnected before the tunnel opened",
        tunnelId,
      );
    }
    safeSend(socket, {
      type: "tunnelOpened",
      tunnelId,
      target: target.state.device,
    });
  }

  closeTunnel(socket, state, control) {
    const tunnelId = validateTunnelId(control.tunnelId);
    const targetId = state.tunnels[tunnelId];
    if (!targetId) {
      throw new RelayProtocolError("tunnelNotFound", "tunnel does not exist", tunnelId);
    }
    const target = this.findDevice(targetId, socket);
    delete state.tunnels[tunnelId];
    writeAttachment(socket, state);
    if (target) {
      delete target.state.tunnels[tunnelId];
      writeAttachment(target.socket, target.state);
    }
    const closed = { type: "tunnelClosed", tunnelId, reason: "requested" };
    if (target) safeSend(target.socket, closed);
    safeSend(socket, closed);
  }

  handleControl(socket, state, text) {
    const control = parseControlMessage(text);
    switch (control.type) {
      case "register":
        throw new RelayProtocolError("alreadyRegistered", "device has already registered");
      case "ping":
        if (!Number.isSafeInteger(control.nonce) || control.nonce < 0) {
          throw new RelayProtocolError("invalidMessage", "keepalive nonce is invalid");
        }
        safeSend(socket, { type: "pong", nonce: control.nonce });
        return;
      case "openTunnel":
        this.openTunnel(socket, state, control);
        return;
      case "closeTunnel":
        this.closeTunnel(socket, state, control);
        return;
      default:
        throw new RelayProtocolError("invalidMessage", "control message type is not supported");
    }
  }

  handleBinary(socket, state, message) {
    const tunnelId = tunnelIdFromFrame(message);
    const targetId = state.tunnels[tunnelId];
    if (!targetId) {
      throw new RelayProtocolError("tunnelNotFound", "tunnel does not exist", tunnelId);
    }
    const target = this.findDevice(targetId, socket);
    if (!target) {
      delete state.tunnels[tunnelId];
      writeAttachment(socket, state);
      throw new RelayProtocolError("targetOffline", "tunnel peer is offline", tunnelId);
    }
    if (!safeSend(target.socket, message)) {
      delete state.tunnels[tunnelId];
      delete target.state.tunnels[tunnelId];
      writeAttachment(socket, state);
      writeAttachment(target.socket, target.state);
      throw new RelayProtocolError("targetOffline", "tunnel peer disconnected", tunnelId);
    }
  }

  webSocketMessage(socket, message) {
    const state = readAttachment(socket);
    if (!state.registered || !state.device) {
      if (typeof message !== "string") {
        this.reject(socket, new RelayProtocolError(
          "registrationRequired",
          "the first relay message must register the device",
        ), true);
        return;
      }
      this.register(socket, message, state);
      return;
    }
    try {
      if (typeof message === "string") {
        if (message.length > MAX_CONTROL_MESSAGE_SIZE) {
          throw new RelayProtocolError("invalidMessage", "control message exceeds the size limit");
        }
        this.handleControl(socket, state, message);
      } else if (message instanceof ArrayBuffer) {
        if (message.byteLength > MAX_RELAY_FRAME) {
          throw new RelayProtocolError("invalidMessage", "relay frame payload exceeds the limit");
        }
        this.handleBinary(socket, state, message);
      } else {
        throw new RelayProtocolError("invalidMessage", "relay message type is not supported");
      }
    } catch (error) {
      this.reject(socket, error);
    }
  }

  cleanup(socket) {
    const disconnected = readAttachment(socket);
    if (!disconnected.registered || !disconnected.device) return;
    for (const record of this.registeredRecords(socket)) {
      const affected = Object.entries(record.state.tunnels)
        .filter(([, peerId]) => peerId === disconnected.device.id)
        .map(([tunnelId]) => tunnelId);
      if (affected.length === 0) continue;
      for (const tunnelId of affected) {
        delete record.state.tunnels[tunnelId];
        safeSend(record.socket, {
          type: "tunnelClosed",
          tunnelId,
          reason: "peerDisconnected",
        });
      }
      writeAttachment(record.socket, record.state);
    }
    this.broadcastPresence(socket);
  }

  webSocketClose(socket) {
    this.cleanup(socket);
  }

  webSocketError(socket) {
    this.cleanup(socket);
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/healthz") {
      if (!validToken(env.NELOA_RELAY_TOKEN)) {
        return json({ status: "misconfigured", relayProtocolVersion: RELAY_PROTOCOL_VERSION }, {
          status: 503,
        });
      }
      return json({ status: "ok", relayProtocolVersion: RELAY_PROTOCOL_VERSION });
    }
    if (url.pathname !== "/v1/ws") return new Response("not found", { status: 404 });
    if (request.method !== "GET"
      || (request.headers.get("upgrade") ?? "").toLowerCase() !== "websocket") {
      return new Response("websocket upgrade required", { status: 426 });
    }
    if (!requestIsAuthorized(request, env.NELOA_RELAY_TOKEN)) {
      return new Response("relay authentication required", { status: 401 });
    }
    const id = env.RELAY_OBJECT.idFromName(RELAY_OBJECT_NAME);
    return env.RELAY_OBJECT.get(id).fetch(request);
  },
};
