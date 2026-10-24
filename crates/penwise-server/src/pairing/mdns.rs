//! `_penwise._tcp` mDNS advertisement on the physical LAN interface (server-stack.md §5).
//!
//! Owner: slice c.
//!
//! `mdns_sd::ServiceDaemon` with every interface disabled except the one holding [`super::lan::lan_ipv4`]
//! (no `docker*`, `br-*`, `veth*`, `tailscale*`, loopback, IPv6 link-local), `enable_addr_auto()` so addresses
//! follow DHCP changes. TXT records: `v=<PROTOCOL_VERSION>`, `sid=<ServerId>` only — never a code or token
//! (TXT is readable by the whole LAN).

use core::fmt;

use mdns_sd::ServiceDaemon;
use penwise_proto::ServerId;

/// DNS-SD service type (label ≤ 15 chars).
pub const SERVICE_TYPE: &str = "_penwise._tcp.local.";

/// mDNS failure (non-fatal at start-up: logged, server keeps running).
#[derive(Debug, thiserror::Error)]
pub enum MdnsError {
    #[error(transparent)]
    Daemon(#[from] mdns_sd::Error),
    /// No usable LAN interface.
    #[error("no LAN interface")]
    NoInterface,
}

/// Running advertisement; dropping it without [`MdnsAdvertiser::shutdown`] leaves the daemon thread to exit
/// with the process.
pub struct MdnsAdvertiser {
    daemon: ServiceDaemon,
    fullname: String,
}

impl fmt::Debug for MdnsAdvertiser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MdnsAdvertiser")
            .field("fullname", &self.fullname)
            .finish_non_exhaustive()
    }
}

impl MdnsAdvertiser {
    /// Registers `<instance>._penwise._tcp.local.` on `port` with host name `<hostname>.local.`.
    ///
    /// # Errors
    /// [`MdnsError`].
    pub fn start(
        instance: &str,
        hostname: &str,
        port: u16,
        server: ServerId,
    ) -> Result<Self, MdnsError> {
        todo!()
    }

    /// Unregisters (goodbye packets) and stops the daemon.
    ///
    /// # Errors
    /// [`MdnsError::Daemon`].
    pub fn shutdown(self) -> Result<(), MdnsError> {
        todo!()
    }
}
