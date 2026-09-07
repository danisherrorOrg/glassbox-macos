import { useEffect, useState } from "react";
import { getTimeline, onTimelineEvent } from "../api/inspector";
import type { TrafficEvent } from "../api/types";
import { StatusBadge } from "./StatusBadge";
import type { ViewState } from "./StatusBadge";
import "./ProcessList.css";
import "./TimelineView.css";

const EVENT_LABELS: Record<TrafficEvent["type"], string> = {
  connection_opened: "Connection opened",
  connection_closed: "Connection closed",
  connection_expired: "Connection expired",
  request: "Request",
  response: "Response",
};

export function TimelineView({ pid, isMonitoring }: { pid: number; isMonitoring: boolean }) {
  const [viewState, setViewState] = useState<ViewState>("loading");
  const [reason, setReason] = useState<string | null>(null);
  const [events, setEvents] = useState<TrafficEvent[]>([]);

  async function refresh() {
    setViewState("loading");
    const envelope = await getTimeline(pid);
    setViewState(envelope.status.state);
    setReason(envelope.status.reason);
    setEvents(envelope.data ?? []);
  }

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pid]);

  useEffect(() => {
    if (!isMonitoring) return;
    let unlisten: (() => void) | undefined;
    onTimelineEvent((event) => {
      setEvents((prev) => (prev.some((e) => e.event_id === event.event_id) ? prev : [...prev, event]));
    }).then((un) => {
      unlisten = un;
    });
    return () => unlisten?.();
  }, [isMonitoring, pid]);

  const sorted = [...events].sort((a, b) => b.timestamp.localeCompare(a.timestamp));

  return (
    <div className="process-list">
      <div className="process-list__toolbar">
        <button onClick={refresh} disabled={viewState === "loading"}>
          Refresh
        </button>
        <StatusBadge state={viewState} reason={reason} />
      </div>

      {viewState !== "loading" && viewState !== "observed" && (
        <p className="process-list__status-note">{reason ?? "Timeline is not currently available."}</p>
      )}

      {viewState === "observed" &&
        (sorted.length === 0 ? (
          <p className="process-list__status-note">No events yet.</p>
        ) : (
          <ul className="timeline-view__list">
            {sorted.map((e) => (
              <li key={e.event_id} className="timeline-view__item">
                <span className="timeline-view__time">{new Date(e.timestamp).toLocaleTimeString()}</span>
                <span className={`timeline-view__type timeline-view__type--${e.type}`}>
                  {EVENT_LABELS[e.type]}
                </span>
                <span className="timeline-view__conn">{e.connection_id}</span>
              </li>
            ))}
          </ul>
        ))}
    </div>
  );
}
