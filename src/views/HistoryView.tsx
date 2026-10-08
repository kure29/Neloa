import { useEffect, useState } from "react";

import { TRANSFER_STATUS_LABELS, formatTime } from "../lib/format";
import { groupHistoryByDay, type HistoryFilter } from "../lib/historyGroups";
import type { NeloaState } from "../lib/useNeloa";
import type { TransferRecord } from "../types";
import { Icon } from "../ui/icons";
import { Badge, Button, EmptyState, IconButton, PageHeader, cx } from "../ui/kit";

function statusTone(record: TransferRecord) {
  if (record.kind !== "file" || !record.status) return "ok" as const;
  if (record.status === "completed") return "ok" as const;
  if (record.status === "failed") return "danger" as const;
  return "warn" as const;
}

const FILTERS: Array<{ id: HistoryFilter; label: string }> = [
  { id: "all", label: "全部" },
  { id: "sent", label: "发送" },
  { id: "received", label: "接收" },
];

export function HistoryView({ app }: { app: NeloaState }) {
  const [filter, setFilter] = useState<HistoryFilter>("all");
  const [confirmClear, setConfirmClear] = useState(false);
  const groups = groupHistoryByDay(app.history, filter);
  const counts: Record<HistoryFilter, number> = {
    all: app.history.length,
    sent: app.history.filter((record) => record.direction === "sent").length,
    received: app.history.filter((record) => record.direction === "received").length,
  };

  // Clearing is irreversible, so the first press only arms it for a moment.
  useEffect(() => {
    if (!confirmClear) return;
    const timer = window.setTimeout(() => setConfirmClear(false), 3000);
    return () => window.clearTimeout(timer);
  }, [confirmClear]);

  return (
    <div className="page">
      <PageHeader
        title="传输记录"
        subtitle={app.history.length > 0
          ? `共 ${app.history.length} 条，只保存在这台设备`
          : "记录只保存在这台设备"}
        aside={app.history.length > 0 ? (
          <Button
            variant={confirmClear ? "danger" : "ghost"}
            size="sm"
            onClick={() => {
              if (!confirmClear) {
                setConfirmClear(true);
                return;
              }
              setConfirmClear(false);
              app.clearHistory();
            }}
          >
            <Icon name="trash" size={14} />
            {confirmClear ? "再点一次清空全部" : "清空"}
          </Button>
        ) : undefined}
      />

      {app.history.length > 0 && (
        <div className="segmented" role="group" aria-label="按方向筛选记录">
          {FILTERS.map((option) => (
            <button
              key={option.id}
              type="button"
              className={cx("segment", filter === option.id && "active")}
              aria-pressed={filter === option.id}
              onClick={() => setFilter(option.id)}
            >
              {option.label}
              <span className="segment-count">{counts[option.id]}</span>
            </button>
          ))}
        </div>
      )}

      {app.history.length === 0 ? (
        <EmptyState
          icon="clock"
          title="还没有传输记录"
          description="与设备配对后发送文件，记录会出现在这里。"
        />
      ) : groups.length === 0 ? (
        <p className="card-empty">
          {filter === "sent" ? "还没有发送过文件。" : "还没有接收过文件。"}
        </p>
      ) : groups.map((group) => (
        <section className="record-group" key={group.key} aria-labelledby={`history-day-${group.key}`}>
          <h2 className="record-group-title" id={`history-day-${group.key}`}>
            {group.label}
            <span>{group.records.length} 条</span>
          </h2>
          <ul className="record-list">
            {group.records.map((record) => {
              const retryable = record.kind === "file"
                && record.status
                && record.status !== "completed"
                && record.direction === "sent"
                && Boolean(record.path);
              const revealable = app.shell === "desktop"
                && record.kind === "file"
                && record.status === "completed"
                && record.direction === "received"
                && Boolean(record.path);

              return (
                <li className="record" key={record.id}>
                  <span className={cx("record-direction", record.direction)}>
                    <Icon name={record.direction === "sent" ? "sent" : "received"} />
                  </span>
                  <div className="record-body">
                    <strong className="truncate">{record.name}</strong>
                    <span className="truncate">
                      {formatTime(record.atMs)} · {record.direction === "sent" ? "发给" : "来自"} {record.peer} · {record.detail}
                    </span>
                  </div>
                  <div className="record-actions">
                    {retryable && (
                      <Button size="sm" onClick={() => app.retryTransfer(record)}>
                        重试
                      </Button>
                    )}
                    {revealable && (
                      <Button
                        size="sm"
                        aria-label={`在文件管理器中显示 ${record.name}`}
                        onClick={() => void app.revealHistoryItem(record)}
                      >
                        <Icon name="folder" size={14} />
                        显示
                      </Button>
                    )}
                    <Badge tone={statusTone(record)}>
                      {record.kind === "file" && record.status
                        ? TRANSFER_STATUS_LABELS[record.status]
                        : "已加密"}
                    </Badge>
                    <IconButton
                      icon="trash"
                      className="record-delete"
                      label={`删除 ${record.name} 的记录`}
                      onClick={() => app.removeHistoryRecord(record.id)}
                    />
                  </div>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}
