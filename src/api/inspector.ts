import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  Envelope,
  MonitoringStatus,
  NetworkConnection,
  ProcessInfo,
  ResolvedHostname,
  TrafficEvent,
} from "./types";

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

export async function getHostnames(pid: number): Promise<Envelope<ResolvedHostname[]>> {
  try {
    return await invoke("get_hostnames", { pid });
  } catch (e) {
    return transportFailure(e);
  }
}

export async function getTimeline(pid: number): Promise<Envelope<TrafficEvent[]>> {
  try {
    return await invoke("get_timeline", { pid });
  } catch (e) {
    return transportFailure(e);
  }
}

export function startMonitoring(pid: number, intervalMs?: number): Promise<MonitoringStatus> {
  return invoke("start_monitoring", { pid, intervalMs: intervalMs ?? null });
}

export function stopMonitoring(): Promise<MonitoringStatus> {
  return invoke("stop_monitoring");
}

export function getMonitoringStatus(): Promise<MonitoringStatus> {
  return invoke("get_monitoring_status");
}

export function onProcessesUpdated(
  cb: (envelope: Envelope<ProcessInfo[]>) => void,
): Promise<UnlistenFn> {
  return listen<Envelope<ProcessInfo[]>>("processes-updated", (e) => cb(e.payload));
}

export function onConnectionsUpdated(
  cb: (envelope: Envelope<NetworkConnection[]>) => void,
): Promise<UnlistenFn> {
  return listen<Envelope<NetworkConnection[]>>("connections-updated", (e) => cb(e.payload));
}

export function onMonitoringStateChanged(
  cb: (status: MonitoringStatus) => void,
): Promise<UnlistenFn> {
  return listen<MonitoringStatus>("monitoring-state-changed", (e) => cb(e.payload));
}

export function onTimelineEvent(cb: (event: TrafficEvent) => void): Promise<UnlistenFn> {
  return listen<TrafficEvent>("timeline-event", (e) => cb(e.payload));
}

export function onHostnamesUpdated(cb: () => void): Promise<UnlistenFn> {
  return listen<null>("hostnames-updated", () => cb());
}
