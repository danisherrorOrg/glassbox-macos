import { useEffect, useState } from "react";
import {
  getMonitoringStatus,
  getProcesses,
  onMonitoringStateChanged,
  onProcessesUpdated,
  startMonitoring,
  stopMonitoring,
} from "../api/inspector";
import type { MonitoringStatus, ProcessInfo } from "../api/types";
import { ConnectionsView } from "./ConnectionsView";
import { StatusBadge } from "./StatusBadge";
import { TimelineView } from "./TimelineView";
import "./ProcessDetail.css";

type Tab = "overview" | "connections" | "traffic" | "timeline";

const TABS: { id: Tab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "connections", label: "Connections" },
  { id: "traffic", label: "API Traffic" },
  { id: "timeline", label: "Timeline" },
];

export function ProcessDetail({ pid }: { pid: number }) {
  const [tab, setTab] = useState<Tab>("connections");
  const [processInfo, setProcessInfo] = useState<ProcessInfo | null>(null);
  const [monitoring, setMonitoring] = useState<MonitoringStatus>({
    state: "idle",
    pid: null,
    reason: null,
  });
  const [busy, setBusy] = useState(false);

  async function refreshProcessInfo() {
    const envelope = await getProcesses();
    const info = envelope.data?.find((p) => p.pid === pid) ?? null;
    setProcessInfo(info);
  }

  useEffect(() => {
    refreshProcessInfo();
    getMonitoringStatus().then(setMonitoring);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pid]);

  useEffect(() => {
    let unlistenMonitoring: (() => void) | undefined;
    let unlistenProcesses: (() => void) | undefined;

    onMonitoringStateChanged((status) => setMonitoring(status)).then((un) => {
      unlistenMonitoring = un;
    });
    onProcessesUpdated((envelope) => {
      const info = envelope.data?.find((p) => p.pid === pid);
      if (info) setProcessInfo(info);
    }).then((un) => {
      unlistenProcesses = un;
    });

    return () => {
      unlistenMonitoring?.();
      unlistenProcesses?.();
    };
  }, [pid]);

  const isMonitoringThis = monitoring.state === "running" && monitoring.pid === pid;
  const monitoringElsewhere =
    monitoring.state === "running" && monitoring.pid !== null && monitoring.pid !== pid;

  async function toggleMonitoring() {
    setBusy(true);
    try {
      if (isMonitoringThis) {
        setMonitoring(await stopMonitoring());
      } else {
        setMonitoring(await startMonitoring(pid));
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="process-detail">
      <div className="process-detail__header">
        <h2 className="process-detail__heading">
          {processInfo?.name ?? "PID"} <span className="process-detail__pid">({pid})</span>
        </h2>
        <div className="process-detail__monitor-controls">
          {monitoringElsewhere && (
            <span className="process-detail__note">
              Monitoring PID {monitoring.pid} — starting here will take over.
            </span>
          )}
          <button onClick={toggleMonitoring} disabled={busy}>
            {isMonitoringThis ? "Stop Live Monitoring" : "Start Live Monitoring"}
          </button>
          {isMonitoringThis && <StatusBadge state="observed" reason="Live monitoring running" />}
        </div>
      </div>

      {processInfo?.process_state === "exited" && (
        <p className="process-detail__exited-banner">
          Process exited{monitoring.reason === "process exited" ? " — live monitoring stopped." : "."}
        </p>
      )}

      <div className="process-detail__tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={
              t.id === tab ? "process-detail__tab process-detail__tab--active" : "process-detail__tab"
            }
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="process-detail__panel">
        {tab === "connections" && <ConnectionsView pid={pid} isMonitoring={isMonitoringThis} />}
        {tab === "timeline" && <TimelineView pid={pid} isMonitoring={isMonitoringThis} />}
        {tab !== "connections" && tab !== "timeline" && (
          <p className="process-detail__not-yet">
            Not implemented yet — Phase 0.2 only builds Connections and Timeline.
          </p>
        )}
      </div>
    </div>
  );
}
