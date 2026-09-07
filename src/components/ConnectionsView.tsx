import { useEffect, useState } from "react";
import {
  getConnections,
  getHostnames,
  onConnectionsUpdated,
  onHostnamesUpdated,
} from "../api/inspector";
import type { NetworkConnection, ResolvedHostname } from "../api/types";
import { StatusBadge } from "./StatusBadge";
import type { ViewState } from "./StatusBadge";
import "./ProcessList.css";

function formatBytes(n: number | null): string {
  // Field-level absence rule (docs/OBSERVATION_CONTRACT.md): an absent
  // optional field renders as an explicit "not reported" affordance,
  // never as 0 or blank.
  return n === null ? "not reported" : n.toLocaleString();
}

export function ConnectionsView({ pid, isMonitoring }: { pid: number; isMonitoring: boolean }) {
  const [viewState, setViewState] = useState<ViewState>("loading");
  const [reason, setReason] = useState<string | null>(null);
  const [connections, setConnections] = useState<NetworkConnection[]>([]);
  const [hostnames, setHostnames] = useState<Map<string, ResolvedHostname>>(new Map());

  async function refreshHostnames() {
    const envelope = await getHostnames(pid);
    setHostnames(new Map((envelope.data ?? []).map((h) => [h.connection_id, h])));
  }

  async function refresh() {
    setViewState("loading");
    const envelope = await getConnections(pid);
    setViewState(envelope.status.state);
    setReason(envelope.status.reason);
    setConnections(envelope.data ?? []);
    refreshHostnames();
  }

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pid]);

  // Live updates only while this exact process is the one being monitored
  // — the backend only ever emits for a single monitored pid at a time.
  useEffect(() => {
    if (!isMonitoring) return;

    let unlistenConnections: (() => void) | undefined;
    let unlistenHostnames: (() => void) | undefined;

    onConnectionsUpdated((envelope) => {
      setViewState(envelope.status.state);
      setReason(envelope.status.reason);
      setConnections(envelope.data ?? []);
    }).then((un) => {
      unlistenConnections = un;
    });
    onHostnamesUpdated(refreshHostnames).then((un) => {
      unlistenHostnames = un;
    });

    return () => {
      unlistenConnections?.();
      unlistenHostnames?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isMonitoring, pid]);

  return (
    <div className="process-list">
      <div className="process-list__toolbar">
        <button onClick={refresh} disabled={viewState === "loading"}>
          Refresh
        </button>
        <StatusBadge state={viewState} reason={reason} />
      </div>

      {viewState !== "loading" && viewState !== "observed" && viewState !== "stale" && (
        <p className="process-list__status-note">
          {reason ?? "Connections are not currently available for this process."}
        </p>
      )}

      {(viewState === "observed" || viewState === "stale" || viewState === "transient_failure") && (
        <table className="process-list__table">
          <thead>
            <tr>
              <th>Protocol</th>
              <th>Local</th>
              <th>Remote</th>
              <th>State</th>
              <th>Bytes sent</th>
              <th>Bytes received</th>
              <th>Lifecycle</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            {connections.length === 0 ? (
              <tr>
                <td colSpan={8} className="process-list__empty">
                  No connections observed.
                </td>
              </tr>
            ) : (
              connections.map((c) => {
                const hostname = hostnames.get(c.connection_id);
                return (
                  <tr key={c.connection_id}>
                    <td>{c.protocol.toUpperCase()}</td>
                    <td>
                      {c.local_addr}:{c.local_port}
                    </td>
                    <td>
                      {c.remote_addr !== null && c.remote_port !== null ? (
                        <span title={c.remote_addr}>
                          {hostname ? `${hostname.hostname}:${c.remote_port}` : `${c.remote_addr}:${c.remote_port}`}
                        </span>
                      ) : (
                        "—"
                      )}
                    </td>
                    <td>{c.state}</td>
                    <td>{formatBytes(c.bytes_sent)}</td>
                    <td>{formatBytes(c.bytes_received)}</td>
                    <td>{c.lifecycle_state}</td>
                    <td>
                      <StatusBadge state={c.status.state} reason={c.status.reason} />
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      )}
    </div>
  );
}
