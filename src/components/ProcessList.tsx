import { useEffect, useMemo, useState } from "react";
import { getProcesses } from "../api/inspector";
import type { ProcessInfo } from "../api/types";
import { StatusBadge } from "./StatusBadge";
import type { ViewState } from "./StatusBadge";
import "./ProcessList.css";

export function ProcessList({
  selectedPid,
  onSelectProcess,
}: {
  selectedPid: number | null;
  onSelectProcess: (pid: number) => void;
}) {
  const [viewState, setViewState] = useState<ViewState>("loading");
  const [reason, setReason] = useState<string | null>(null);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [query, setQuery] = useState("");

  async function refresh() {
    setViewState("loading");
    const envelope = await getProcesses();
    setViewState(envelope.status.state);
    setReason(envelope.status.reason);
    setProcesses(envelope.data ?? []);
  }

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return processes;
    return processes.filter(
      (p) => p.name.toLowerCase().includes(q) || String(p.pid).includes(q),
    );
  }, [processes, query]);

  return (
    <div className="process-list">
      <div className="process-list__toolbar">
        <input
          className="process-list__search"
          placeholder="Search by name or PID…"
          value={query}
          onChange={(e) => setQuery(e.currentTarget.value)}
        />
        <button onClick={refresh} disabled={viewState === "loading"}>
          Refresh
        </button>
        <StatusBadge state={viewState} reason={reason} />
      </div>

      {viewState !== "loading" && viewState !== "observed" && (
        <p className="process-list__status-note">
          {reason ?? "Process list is not currently available."}
        </p>
      )}

      {(viewState === "observed" || viewState === "stale" || viewState === "transient_failure") && (
        <table className="process-list__table">
          <thead>
            <tr>
              <th>Name</th>
              <th>PID</th>
              <th>Path</th>
              <th>Connections</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            {filtered.length === 0 ? (
              <tr>
                <td colSpan={5} className="process-list__empty">
                  No processes observed.
                </td>
              </tr>
            ) : (
              filtered.map((p) => (
                <tr
                  key={p.pid}
                  className={p.pid === selectedPid ? "process-list__row--selected" : undefined}
                  onClick={() => onSelectProcess(p.pid)}
                >
                  <td>{p.name}</td>
                  <td>{p.pid}</td>
                  <td>
                    {p.executable_path !== null ? (
                      <span className="process-list__path" title={p.executable_path}>
                        {p.executable_path}
                      </span>
                    ) : (
                      <span className="process-list__path-unknown">path unknown</span>
                    )}
                  </td>
                  <td>{p.active_connection_count ?? "—"}</td>
                  <td>
                    <StatusBadge state={p.status.state} reason={p.status.reason} />
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
