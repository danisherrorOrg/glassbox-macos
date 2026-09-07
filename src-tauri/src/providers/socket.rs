//! `SocketProvider` — see `docs/ARCHITECTURE.md`. Implemented with
//! `netstat2` (confirmed sufficient by the Phase 0 spike: it attributes
//! every socket to a PID on this OS version, no `libproc`-FFI-from-day-one
//! fallback needed — `docs/PERMISSIONS_AND_PLATFORM.md` checklist item 3).
//!
//! Permission handling: `netstat2` silently drops sockets it can't list for
//! a PID rather than surfacing an error (`PIF-046`) — so this provider's
//! `snapshot()` always reports whatever it *could* see as `observed`
//! (cross-user sockets are simply absent, exactly like unprivileged `lsof`).
//! Deciding whether a specific requested PID is denied vs. just genuinely
//! empty is the Engine's job, via `is_permitted`, which goes through
//! `libproc` directly (`providers::system::owner_uid`) rather than trusting
//! this — same reasoning as `ProcessProvider`'s spike-verified split.

use chrono::Utc;
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};

use crate::models::{Protocol, SocketObservation, SocketSnapshot};
use crate::providers::system;

pub trait SocketProvider: Send + Sync {
    fn snapshot(&self) -> SocketSnapshot;

    /// Whether `pid` is owned by the current user (and therefore whether
    /// its sockets are visible to this provider at all without elevation).
    fn is_permitted(&self, pid: u32) -> bool;
}

pub struct NetstatSocketProvider;

impl SocketProvider for NetstatSocketProvider {
    fn snapshot(&self) -> SocketSnapshot {
        let now = Utc::now();
        // IPv4/IPv6 only — AF_UNIX (local IPC) is deliberately out of scope
        // for a network inspector, not an oversight. See DECISIONS.md ADR-016.
        let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
        let proto_flags = ProtocolFlags::TCP | ProtocolFlags::UDP;

        match get_sockets_info(af_flags, proto_flags) {
            Ok(sockets) => {
                let observations = sockets
                    .into_iter()
                    .flat_map(|socket| {
                        let (protocol, local_addr, local_port, remote_addr, remote_port, state) =
                            match &socket.protocol_socket_info {
                                ProtocolSocketInfo::Tcp(tcp) => (
                                    Protocol::Tcp,
                                    tcp.local_addr.to_string(),
                                    tcp.local_port,
                                    Some(tcp.remote_addr.to_string()),
                                    Some(tcp.remote_port),
                                    tcp.state.to_string(),
                                ),
                                ProtocolSocketInfo::Udp(udp) => (
                                    Protocol::Udp,
                                    udp.local_addr.to_string(),
                                    udp.local_port,
                                    None,
                                    None,
                                    "UDP".to_string(),
                                ),
                            };

                        // LISTEN sockets report a remote 0.0.0.0:0/[::]:0 rather
                        // than omitting it — normalize to the documented "absent
                        // for LISTEN sockets" shape (`docs/DATA_MODEL.md`).
                        let (remote_addr, remote_port) = if state == "LISTEN" {
                            (None, None)
                        } else {
                            (remote_addr, remote_port)
                        };

                        let pids = socket.associated_pids.clone();
                        pids.into_iter().map(move |pid| SocketObservation {
                            pid,
                            protocol,
                            local_addr: local_addr.clone(),
                            local_port,
                            remote_addr: remote_addr.clone(),
                            remote_port,
                            state: state.clone(),
                            // Verified unobtainable via this provider stack —
                            // see the module doc comment and PIF-045.
                            bytes_sent: None,
                            bytes_received: None,
                        })
                    })
                    .collect();

                SocketSnapshot {
                    timestamp: now,
                    observations,
                    status: crate::models::ProviderStatus::observed(now),
                }
            }
            Err(e) => SocketSnapshot {
                timestamp: now,
                observations: Vec::new(),
                status: crate::models::ProviderStatus::transient_failure(now, e.to_string()),
            },
        }
    }

    fn is_permitted(&self, pid: u32) -> bool {
        match system::owner_uid(pid) {
            Some(uid) => uid == system::current_uid(),
            // If we can't even determine the owner (e.g. the process just
            // exited), don't claim it's denied — let the absence of data
            // speak for itself rather than asserting a permission failure
            // we didn't actually observe.
            None => true,
        }
    }
}
