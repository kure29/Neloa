import type { TransferRecord } from "../types";

export type HistoryFilter = "all" | "sent" | "received";

export interface HistoryGroup {
  /** Local calendar day, `YYYY-MM-DD`. */
  key: string;
  label: string;
  records: TransferRecord[];
}

function dayKey(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

function dayLabel(date: Date, now: Date): string {
  const today = dayKey(now);
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  const key = dayKey(date);
  if (key === today) return "今天";
  if (key === dayKey(yesterday)) return "昨天";
  if (date.getFullYear() === now.getFullYear()) {
    return `${date.getMonth() + 1}月${date.getDate()}日`;
  }
  return `${date.getFullYear()}年${date.getMonth() + 1}月${date.getDate()}日`;
}

/** "今天 14:02", "9月30日 08:15": a record's moment relative to today. */
export function formatRecordMoment(atMs: number, now: Date = new Date()): string {
  const date = new Date(atMs);
  const clock = `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
  return `${dayLabel(date, now)} ${clock}`;
}

/**
 * Splits history into local calendar days. Records are stored newest first,
 * and both the groups and the records inside them keep that order.
 */
export function groupHistoryByDay(
  records: TransferRecord[],
  filter: HistoryFilter = "all",
  now: Date = new Date(),
): HistoryGroup[] {
  const groups: HistoryGroup[] = [];
  const byKey = new Map<string, HistoryGroup>();
  for (const record of records) {
    if (filter !== "all" && record.direction !== filter) continue;
    const date = new Date(record.atMs);
    const key = dayKey(date);
    let group = byKey.get(key);
    if (!group) {
      group = { key, label: dayLabel(date, now), records: [] };
      byKey.set(key, group);
      groups.push(group);
    }
    group.records.push(record);
  }
  return groups;
}
