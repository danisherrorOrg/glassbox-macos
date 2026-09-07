import { useEffect, useState } from "react";
import { getConnections } from "../api/inspector";
import type { NetworkConnection } from "../api/types";
import { StatusBadge } from "./StatusBadge";
import type { ViewState } from "./StatusBadge";
import "./ProcessList.css";

function formatBytes(n: number | null): string {
  // Field-level absence rule (docs/OBSERVATION_CONTRACT.md): an absent
  // optional field renders as an explicit "not reported" affordance,
  // never as 0 or blank.
  return n === null ? "not reported" : n.toLocaleString();
}

export function ConnectionsView({ pid }: { pid: number }) {
  const [viewState, setViewState] = useState<ViewState>("loading");
  const [reason, setReason] = useState<string | null>(null);
  const [connections, setConnections] = useState<NetworkConnection[]>([]);

  async function refresh() {
    setViewState("loading");
    const envelope = await getConnections(pid);
    setViewState(envelope.status.state);
    setReason(envelope.status.reason);
    setConnections(envelope.data ?? []);
  }

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pid]);

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
              connections.map((c) => (
                <tr key={c.connection_id}>
                  <td>{c.protocol.toUpperCase()}</td>
                  <td>
                    {c.local_addr}:{c.local_port}
                  </td>
                  <td>
                    {c.remote_addr !== null && c.remote_port !== null
                      ? `${c.remote_addr}:${c.remote_port}`
                      : "—"}
                  </td>
                  <td>{c.state}</td>
                  <td>{formatBytes(c.bytes_sent)}</td>
                  <td>{formatBytes(c.bytes_received)}</td>
                  <td>{c.lifecycle_state}</td>
                  <td>
                    <StatusBadge state={c.status.state} reason={c.status.reason} />
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      )}
    </div>
  );
}
