//! `ProcessProvider`/`SocketProvider`/`DNSProvider`/`TrafficProvider` traits
//! and their implementations. See `docs/ARCHITECTURE.md`.

mod dns;
mod process;
mod socket;
mod system;
mod traffic;

pub use dns::{DNSProvider, ReverseDnsProvider};
pub use process::{ProcessProvider, SysinfoProcessProvider};
pub use socket::{NetstatSocketProvider, SocketProvider};
#[allow(unused_imports)] // bare trait stub — see traffic.rs
pub use traffic::TrafficProvider;
