export const RELAY_PROTOCOL_VERSION = 1;
export const TUNNEL_HEADER_LENGTH = 16;
export const MAX_RELAY_PAYLOAD = 256 * 1024;
export const MAX_RELAY_FRAME = TUNNEL_HEADER_LENGTH + MAX_RELAY_PAYLOAD;
export const MAX_CONTROL_MESSAGE_SIZE = 32 * 1024;
export const MAX_DEVICE_ID_LENGTH = 128;
export const MAX_DEVICE_NAME_LENGTH = 128;
export const MAX_CAPABILITIES = 32;
export const MAX_CAPABILITY_LENGTH = 64;
export const MINIMUM_TOKEN_LENGTH = 32;
export const MAXIMUM_TOKEN_LENGTH = 512;

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const encoder = new TextEncoder();

export class RelayProtocolError extends Error {
  constructor(code, message, tunnelId = null) {
    super(message);
    this.name = "RelayProtocolError";
    this.code = code;
    this.tunnelId = tunnelId;
  }
}

export function byteLength(value) {
  return encoder.encode(value).byteLength;
}

function validateText(label, value, maximumLength) {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new RelayProtocolError("invalidDevice", `${label} is empty`);
  }
  if (byteLength(value) > maximumLength) {
    throw new RelayProtocolError("invalidDevice", `${label} is too long`);
  }
}

export function validateDevice(device) {
  if (!device || typeof device !== "object" || Array.isArray(device)) {
    throw new RelayProtocolError("invalidDevice", "device metadata is missing");
  }
  validateText("device id", device.id, MAX_DEVICE_ID_LENGTH);
  validateText("device name", device.name, MAX_DEVICE_NAME_LENGTH);
  validateText("platform", device.platform, MAX_CAPABILITY_LENGTH);
  validateText("app version", device.appVersion, MAX_CAPABILITY_LENGTH);
  if (!Number.isInteger(device.protocolVersion)
    || !Number.isInteger(device.minProtocolVersion)
    || device.protocolVersion < 1
    || device.minProtocolVersion < 1
    || device.minProtocolVersion > device.protocolVersion) {
    throw new RelayProtocolError("invalidDevice", "device protocol range is invalid");
  }
  if (!Array.isArray(device.capabilities) || device.capabilities.length > MAX_CAPABILITIES) {
    throw new RelayProtocolError("invalidDevice", "device capabilities are invalid");
  }
  for (const capability of device.capabilities) {
    validateText("capability", capability, MAX_CAPABILITY_LENGTH);
  }
  return {
    id: device.id,
    name: device.name,
    platform: device.platform,
    appVersion: device.appVersion,
    protocolVersion: device.protocolVersion,
    minProtocolVersion: device.minProtocolVersion,
    capabilities: [...device.capabilities],
  };
}

export function parseControlMessage(text) {
  if (typeof text !== "string" || byteLength(text) > MAX_CONTROL_MESSAGE_SIZE) {
    throw new RelayProtocolError("invalidMessage", "control message exceeds the size limit");
  }
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    throw new RelayProtocolError("invalidMessage", "control message is not valid relay JSON");
  }
  if (!value || typeof value !== "object" || Array.isArray(value) || typeof value.type !== "string") {
    throw new RelayProtocolError("invalidMessage", "control message type is missing");
  }
  return value;
}

export function parseRegistration(text) {
  const control = parseControlMessage(text);
  if (control.type !== "register") {
    throw new RelayProtocolError(
      "registrationRequired",
      "the first relay message must register the device",
    );
  }
  if (control.relayProtocolVersion !== RELAY_PROTOCOL_VERSION) {
    throw new RelayProtocolError(
      "incompatibleProtocol",
      "relay protocol version is not supported",
    );
  }
  return validateDevice(control.device);
}

export function validateTunnelId(value) {
  if (typeof value !== "string" || !UUID_PATTERN.test(value)) {
    throw new RelayProtocolError("invalidMessage", "tunnel id is not a valid UUID");
  }
  return value.toLowerCase();
}

export function validateTargetId(value, tunnelId) {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new RelayProtocolError("invalidTarget", "target device id is empty", tunnelId);
  }
  if (byteLength(value) > MAX_DEVICE_ID_LENGTH) {
    throw new RelayProtocolError("invalidTarget", "target device id is too long", tunnelId);
  }
  return value;
}

export function tunnelIdFromFrame(message) {
  if (!(message instanceof ArrayBuffer)) {
    throw new RelayProtocolError("invalidMessage", "relay data frame must be binary");
  }
  if (message.byteLength < TUNNEL_HEADER_LENGTH) {
    throw new RelayProtocolError("invalidMessage", "relay frame is shorter than its header");
  }
  if (message.byteLength > MAX_RELAY_FRAME) {
    throw new RelayProtocolError("invalidMessage", "relay frame payload exceeds the limit");
  }
  const bytes = new Uint8Array(message, 0, TUNNEL_HEADER_LENGTH);
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function validToken(token) {
  return typeof token === "string"
    && token.length >= MINIMUM_TOKEN_LENGTH
    && token.length <= MAXIMUM_TOKEN_LENGTH
    && !/\s/.test(token);
}

export function timingSafeTextEqual(left, right) {
  const leftBytes = encoder.encode(left);
  const rightBytes = encoder.encode(right);
  const maximum = Math.max(leftBytes.byteLength, rightBytes.byteLength);
  let difference = leftBytes.byteLength ^ rightBytes.byteLength;
  for (let index = 0; index < maximum; index += 1) {
    difference |= (leftBytes[index] ?? 0) ^ (rightBytes[index] ?? 0);
  }
  return difference === 0;
}

export function errorControl(error) {
  return {
    type: "error",
    code: error instanceof RelayProtocolError ? error.code : "invalidMessage",
    message: error instanceof Error ? error.message : "relay request failed",
    ...(error instanceof RelayProtocolError && error.tunnelId
      ? { tunnelId: error.tunnelId }
      : {}),
  };
}
