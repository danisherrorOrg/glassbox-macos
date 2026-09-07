//! `ProcessProvider` — see `docs/ARCHITECTURE.md`. Implemented with
//! `sysinfo`, falling back to direct `libproc` FFI (`proc_pidpath`) for
//! `executable_path` where `sysinfo` doesn't supply it — the Phase 0 spike
//! confirmed `proc_pidpath` succeeds unprivileged even for other-user/root
//! processes, so no permission handling is needed for this provider (unlike
//! `SocketProvider`).

use chrono::Utc;
use sysinfo::System;

use crate::models::{ProcessObservation, ProcessSnapshot, ProviderStatus};

unsafe extern "C" {
    fn proc_pidpath(pid: libc::c_int, buffer: *mut libc::c_void, buffersize: u32) -> libc::c_int;
}

fn proc_pidpath_fallback(pid: u32) -> Option<String> {
    let mut buf = vec![0u8; 4096];
    let ret = unsafe {
        proc_pidpath(
            pid as libc::c_int,
            buf.as_mut_ptr() as *mut libc::c_void,
            buf.len() as u32,
        )
    };
    if ret <= 0 {
        return None;
    }
    buf.truncate(ret as usize);
    Some(String::from_utf8_lossy(&buf).to_string())
}

pub trait ProcessProvider: Send + Sync {
    fn snapshot(&self) -> ProcessSnapshot;
}

pub struct SysinfoProcessProvider;

impl ProcessProvider for SysinfoProcessProvider {
    fn snapshot(&self) -> ProcessSnapshot {
        let now = Utc::now();
        let mut sys = System::new_all();
        sys.refresh_all();

        let observations = sys
            .processes()
            .iter()
            .map(|(pid, process)| {
                let pid_u32 = pid.as_u32();
                // `None` (not `""`) when both sources fail — see the
                // `executable_path` doc comment on `ProcessObservation`.
                let executable_path = process
                    .exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .filter(|s| !s.is_empty())
                    .or_else(|| proc_pidpath_fallback(pid_u32));

                ProcessObservation {
                    pid: pid_u32,
                    name: process.name().to_string_lossy().to_string(),
                    executable_path,
                    cpu_percent: Some(process.cpu_usage()),
                    memory_bytes: Some(process.memory()),
                }
            })
            .collect();

        ProcessSnapshot {
            timestamp: now,
            observations,
            status: ProviderStatus::observed(now),
        }
    }
}
