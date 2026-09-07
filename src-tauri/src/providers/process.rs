//! `ProcessProvider` — see `docs/ARCHITECTURE.md`. Implemented with
//! `sysinfo`, falling back to direct `libproc` FFI (`proc_pidpath`) for
//! `executable_path` where `sysinfo` doesn't supply it — the Phase 0 spike
//! confirmed `proc_pidpath` succeeds unprivileged even for other-user/root
//! processes, so no permission handling is needed for this provider (unlike
//! `SocketProvider`).
//!
//! `name` is derived from `executable_path`'s basename, not
//! `sysinfo::Process::name()` directly — see `derive_name` below and
//! `docs/DATA_MODEL.md`'s note on `ProcessObservation` for why.

use std::path::Path;
use std::sync::Mutex;

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

/// `sysinfo::Process::name()` reads the kernel's `pbi_comm` field, which
/// macOS truncates to `MAXCOMLEN` (16 bytes, 15 usable) — e.g. "Google Chrome
/// Helper" comes back as "Google Chrome H". `executable_path`'s basename
/// doesn't have this limit, so prefer it whenever the path is known; fall
/// back to the (possibly-truncated) kernel name only when the path itself
/// is unknown.
fn derive_name(executable_path: &Option<String>, fallback: &str) -> String {
    executable_path
        .as_deref()
        .and_then(|p| Path::new(p).file_name())
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| fallback.to_string())
}

pub trait ProcessProvider: Send + Sync {
    fn snapshot(&self) -> ProcessSnapshot;

    fn capabilities(&self) -> crate::models::ProviderCapabilities {
        crate::models::ProviderCapabilities {
            process: Some(crate::models::Availability::Available),
            ..Default::default()
        }
    }
}

/// Holds one `System` across calls, refreshed in place rather than rebuilt
/// each time — `sysinfo` computes `cpu_usage()` as a delta against the
/// *previous* refresh of the *same* `System`; a fresh instance every call
/// has no prior sample and reports ~0% unconditionally. `Mutex`, not
/// `RefCell`, because `ProcessProvider` requires `Send + Sync` (`&self`,
/// not `&mut self` — see `docs/ARCHITECTURE.md`).
pub struct SysinfoProcessProvider {
    system: Mutex<System>,
}

impl SysinfoProcessProvider {
    pub fn new() -> Self {
        Self {
            system: Mutex::new(System::new_all()),
        }
    }
}

impl Default for SysinfoProcessProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessProvider for SysinfoProcessProvider {
    fn snapshot(&self) -> ProcessSnapshot {
        let now = Utc::now();
        let mut sys = self.system.lock().unwrap();
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
                let sysinfo_name = process.name().to_string_lossy().to_string();
                let name = derive_name(&executable_path, &sysinfo_name);

                ProcessObservation {
                    pid: pid_u32,
                    name,
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

#[cfg(test)]
mod tests {
    use super::{derive_name, ProcessProvider, SysinfoProcessProvider};

    #[test]
    fn prefers_path_basename_over_truncated_kernel_name() {
        let path = Some("/Applications/Google Chrome.app/.../Google Chrome Helper (Renderer)".to_string());
        // The truncated pbi_comm-derived name a real macOS process would
        // otherwise report for this binary.
        let truncated_kernel_name = "Google Chrome H";
        assert_eq!(
            derive_name(&path, truncated_kernel_name),
            "Google Chrome Helper (Renderer)"
        );
    }

    #[test]
    fn falls_back_to_kernel_name_when_path_unknown() {
        assert_eq!(derive_name(&None, "kernel_task"), "kernel_task");
    }

    /// Proves the actual root cause, not just that nothing panics: a fresh
    /// `System` per call has no prior sample, so `cpu_usage()` is always
    /// ~0% on its first-ever refresh. Persisting one `System` and refreshing
    /// it again after real CPU work must produce a nonzero delta.
    #[test]
    fn cpu_percent_becomes_measurable_after_second_refresh_of_same_instance() {
        let provider = SysinfoProcessProvider::new();
        let own_pid = std::process::id();

        let baseline = provider.snapshot();
        assert!(
            baseline.observations.iter().any(|p| p.pid == own_pid),
            "this test's own process should be in the first snapshot"
        );

        // Burn CPU on this thread so there's a real, measurable delta
        // between the two refreshes of the same `System` instance.
        let start = std::time::Instant::now();
        let mut x: u64 = 0;
        while start.elapsed() < std::time::Duration::from_millis(300) {
            x = x.wrapping_add(1);
        }
        std::hint::black_box(x);

        let after = provider.snapshot();
        let cpu = after
            .observations
            .iter()
            .find(|p| p.pid == own_pid)
            .and_then(|p| p.cpu_percent)
            .expect("cpu_percent should be Some for our own process");

        assert!(
            cpu > 0.5,
            "cpu_percent was {cpu}% after burning CPU for 300ms on the same provider \
             instance — a fresh System per call (the original bug) would report ~0% here"
        );
    }
}
