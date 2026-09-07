import { useState } from "react";
import { ConnectionsView } from "./ConnectionsView";
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

  return (
    <div className="process-detail">
      <h2 className="process-detail__heading">PID {pid}</h2>
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
        {tab === "connections" && <ConnectionsView pid={pid} />}
        {tab !== "connections" && (
          <p className="process-detail__not-yet">
            Not implemented yet — Phase 0.1 only builds the Connections tab.
          </p>
        )}
      </div>
    </div>
  );
}
