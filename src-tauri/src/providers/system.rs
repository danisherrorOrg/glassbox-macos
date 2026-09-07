//! Direct `libproc` FFI helpers that providers need but `sysinfo` can't
//! reliably supply.
//!
//! Per the Phase 0 permissions spike (`docs/PERMISSIONS_AND_PLATFORM.md`,
//! "Implementation guidance for `SocketProvider`/`ProcessProvider`";
//! `pre-implementation/[2] FINDINGS.md` `PIF-046`): `sysinfo::Process::
//! user_id()` is not reliable enough to decide "is this pid owned by a
//! different user than me" — on the machine the spike ran on it reported
//! `uid=0` for a process actually owned by the logged-in user. This module
//! goes through `libproc` directly instead, the same source `ps`/`lsof` use.

use std::mem::MaybeUninit;

/// Exact layout of macOS's `struct proc_bsdinfo` (`libproc.h` /
/// `proc_info.h`), confirmed against `/Library/Developer/CommandLineTools/
/// SDKs/MacOSX.sdk/usr/include/sys/proc_info.h` via `bindgen` during the
/// Phase 0 spike. Every field is named (not padded/guessed) since
/// `proc_pidinfo` requires the buffer size to match the real struct size.
#[repr(C)]
struct ProcBsdInfo {
    pbi_flags: u32,
    pbi_status: u32,
    pbi_xstatus: u32,
    pbi_pid: u32,
    pbi_ppid: u32,
    pbi_uid: u32,
    pbi_gid: u32,
    pbi_ruid: u32,
    pbi_rgid: u32,
    pbi_svuid: u32,
    pbi_svgid: u32,
    rfu_1: u32,
    pbi_comm: [i8; 16],
    pbi_name: [i8; 32],
    pbi_nfiles: u32,
    pbi_pgid: u32,
    pbi_pjobc: u32,
    e_tdev: u32,
    e_tpgid: u32,
    pbi_nice: i32,
    pbi_start_tvsec: u64,
    pbi_start_tvusec: u64,
}

const PROC_PIDTBSDINFO: libc::c_int = 3;

unsafe extern "C" {
    fn proc_pidinfo(
        pid: libc::c_int,
        flavor: libc::c_int,
        arg: u64,
        buffer: *mut libc::c_void,
        buffersize: libc::c_int,
    ) -> libc::c_int;
}

/// Returns the uid that owns `pid`, or `None` if the lookup itself failed
/// (e.g. the process has already exited). This call is unprivileged and
/// works cross-user — the Phase 0 spike verified `proc_pidinfo(PROC_PIDTBSDINFO)`
/// succeeds regardless of who owns the target pid; it's the *socket*-listing
/// call (`PROC_PIDLISTFDS`) that's uid-gated, not this one.
pub fn owner_uid(pid: u32) -> Option<u32> {
    let mut info: MaybeUninit<ProcBsdInfo> = MaybeUninit::uninit();
    let size = std::mem::size_of::<ProcBsdInfo>() as libc::c_int;
    let ret = unsafe {
        proc_pidinfo(
            pid as libc::c_int,
            PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr() as *mut libc::c_void,
            size,
        )
    };
    if ret <= 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    Some(info.pbi_uid)
}

/// The uid this process itself is running as.
pub fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}
