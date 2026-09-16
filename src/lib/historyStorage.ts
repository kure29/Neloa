import type { FileTransferStatus, TransferRecord } from "../types";

const HISTORY_STORAGE_KEY = "neloa.transfer-history.v1";
const MAX_HISTORY_RECORDS = 200;
const FILE_STATUSES = new Set<FileTransferStatus>([
  "completed",
  "failed",
  "cancelled",
  "rejected",
]);

function isOptionalString(value: unknown): value is string | undefined {
  return value === undefined || typeof value === "string";
}

function isTransferRecord(value: unknown): value is TransferRecord {
  if (!value || typeof value !== "object") return false;
  const record = value as Partial<TransferRecord>;
  return typeof record.id === "string"
    && typeof record.name === "string"
    && typeof record.detail === "string"
    && typeof record.peer === "string"
    && typeof record.atMs === "number"
    && Number.isFinite(record.atMs)
    && (record.direction === "sent" || record.direction === "received")
    && (record.kind === undefined || record.kind === "text" || record.kind === "file")
    && (record.status === undefined || FILE_STATUSES.has(record.status))
    && isOptionalString(record.path)
    && isOptionalString(record.peerId)
    && (record.size === undefined || (typeof record.size === "number" && Number.isFinite(record.size)));
}

export function loadTransferHistory(): TransferRecord[] {
  try {
    const stored = window.localStorage.getItem(HISTORY_STORAGE_KEY);
    if (!stored) return [];
    const parsed: unknown = JSON.parse(stored);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isTransferRecord).slice(0, MAX_HISTORY_RECORDS);
  } catch {
    return [];
  }
}

export function saveTransferHistory(records: TransferRecord[]): void {
  window.localStorage.setItem(
    HISTORY_STORAGE_KEY,
    JSON.stringify(records.slice(0, MAX_HISTORY_RECORDS)),
  );
}

export function prependTransferRecord(
  records: TransferRecord[],
  record: TransferRecord,
): TransferRecord[] {
  return [record, ...records.filter((item) => item.id !== record.id)]
    .slice(0, MAX_HISTORY_RECORDS);
}
