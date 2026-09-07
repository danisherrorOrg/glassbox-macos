import { invoke } from "@tauri-apps/api/core";
import type { Envelope, NetworkConnection, ProcessInfo } from "./types";

// A rejected `invoke()` call is a transport failure (the IPC bridge itself
// failed), distinct from any backend-reported ObservationStatus — per
// docs/OBSERVATION_CONTRACT.md there is no separate "error" state, so this
// renders identically to `unavailable`.
function transportFailure<T>(reason: unknown): Envelope<T> {
  return {
    status: {
      state: "unavailable",
      observed_at: new Date().toISOString(),
      last_successful_at: null,
      reason: reason instanceof Error ? reason.message : String(reason),
      provider: null,
    },
    data: null,
  };
}

export async function getProcesses(): Promise<Envelope<ProcessInfo[]>> {
  try {
    return await invoke("get_processes");
  } catch (e) {
    return transportFailure(e);
  }
}

export async function getConnections(pid: number): Promise<Envelope<NetworkConnection[]>> {
  try {
    return await invoke("get_connections", { pid });
  } catch (e) {
    return transportFailure(e);
  }
}
