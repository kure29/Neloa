import { TRANSFER_STATUS_LABELS } from "../lib/format";
import type { NeloaState } from "../lib/useNeloa";
import type { TransferRecord } from "../types";
import { Icon } from "../ui/icons";
import { Badge, Button, EmptyState, SectionTitle, cx } from "../ui/kit";

function statusTone(record: TransferRecord) {
  if (record.kind !== "file" || !record.status) return "ok" as const;
  if (record.status === "completed") return "ok" as const;
  if (record.status === "failed") return "danger" as const;
  return "warn" as const;
}

export function HistoryView({ app }: { app: NeloaState }) {
  return (
    <div className="page">
      <SectionTitle
        title="传输记录"
        meta={app.history.length > 0 ? `${app.history.length} 条` : undefined}
      />

      {app.history.length === 0 ? (
        <EmptyState
          icon="clock"
          title="还没有传输记录"
          description="与设备配对后发送文件，记录会出现在这里。"
        />
      ) : (
        <ul className="record-list">
          {app.history.map((record) => {
            const retryable = record.kind === "file"
              && record.status
              && record.status !== "completed"
              && record.direction === "sent"
              && Boolean(record.path);

            return (
              <li className="record" key={record.id}>
                <span className={cx("record-direction", record.direction)}>
                  <Icon name={record.direction === "sent" ? "sent" : "received"} />
                </span>
                <div className="record-body">
                  <strong className="truncate">{record.name}</strong>
                  <span className="truncate">
                    {record.peer} · {record.detail} · {record.time}
                  </span>
                </div>
                <div className="record-actions">
                  {retryable && (
                    <Button size="sm" onClick={() => app.retryTransfer(record)}>
                      重试
                    </Button>
                  )}
                  <Badge tone={statusTone(record)}>
                    {record.kind === "file" && record.status
                      ? TRANSFER_STATUS_LABELS[record.status]
                      : "已加密"}
                  </Badge>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
