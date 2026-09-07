import type { ObservationState } from "../api/types";
import "./StatusBadge.css";

// The shared state model, docs/[9] TODO.md Phase 0.1: "loading" is
// client-side only, never a backend status; every other value is one of
// the seven ObservationStatus states, rendered verbatim. There is no
// "empty" state (an observed status with zero rows just means zero rows)
// and no "error" state (transport failures already arrive as
// "unavailable" — see src/api/inspector.ts).
export type ViewState = "loading" | ObservationState;

const LABELS: Record<ViewState, string> = {
  loading: "Loading…",
  observed: "Observed",
  unavailable: "Unavailable",
  permission_denied: "Permission denied",
  unsupported: "Unsupported",
  transient_failure: "Transient failure",
  stale: "Stale",
  unmatched: "Unmatched",
};

export function StatusBadge({ state, reason }: { state: ViewState; reason?: string | null }) {
  return (
    <span className={`status-badge status-badge--${state}`} title={reason ?? undefined}>
      {LABELS[state]}
    </span>
  );
}
