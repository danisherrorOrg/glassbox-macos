// Mirrors src-tauri/src/models — see docs/DATA_MODEL.md and
// docs/OBSERVATION_CONTRACT.md for the authoritative shapes. Keep these in
// sync with the Rust types by hand for now (no shared codegen yet).

export type ObservationState =
  | "observed"
  | "unavailable"
  | "permission_denied"
  | "unsupported"
  | "transient_failure"
  | "stale"
  | "unmatched";

export type StatusProvider = "process" | "socket" | "dns" | "traffic" | "engine";

export interface ObservationStatus {
  state: ObservationState;
  observed_at: string;
  last_successful_at: string | null;
  reason: string | null;
  provider: StatusProvider | null;
}

export interface Envelope<T> {
  status: ObservationStatus;
  data: T | null;
}

export type ProcessState = "running" | "exited";

export interface ProcessInfo {
  pid: number;
  name: string;
  executable_path: string | null;
  cpu_percent: number | null;
  memory_bytes: number | null;
  process_state: ProcessState;
  status: ObservationStatus;
  active_connection_count: number | null;
}

export type Protocol = "tcp" | "udp";

export type LifecycleState = "discovered" | "active" | "closed" | "expired";

export interface NetworkConnection {
  connection_id: string;
  pid: number;
  protocol: Protocol;
  local_addr: string;
  local_port: number;
  remote_addr: string | null;
  remote_port: number | null;
  state: string;
  bytes_sent: number | null;
  bytes_received: number | null;
  lifecycle_state: LifecycleState;
  first_seen: string;
  last_seen: string;
  status: ObservationStatus;
}

export type HostnameSource = "reverse_dns" | "sni" | "http_host";

export interface ResolvedHostname {
  connection_id: string;
  source: HostnameSource;
  hostname: string;
  confidence: number;
  status: ObservationStatus;
}

export type TrafficEventType =
  | "connection_opened"
  | "connection_closed"
  | "connection_expired"
  | "request"
  | "response";

export interface TrafficEvent {
  event_id: string;
  timestamp: string;
  type: TrafficEventType;
  connection_id: string | null;
  request_id: string | null;
  response_id: string | null;
}

export type MonitoringState = "idle" | "starting" | "running" | "stopping" | "stopped" | "failed";

export interface MonitoringStatus {
  state: MonitoringState;
  pid: number | null;
  reason: string | null;
}
