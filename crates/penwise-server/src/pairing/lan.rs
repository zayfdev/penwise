//! LAN address for the QR payload and mDNS (server-stack.md §5 "Binding").
//!
//! Owner: slice c.

use std::net::Ipv4Addr;

/// Interface name prefixes never offered (containers, bridges, VPN, loopback).
pub const EXCLUDED_PREFIXES: &[&str] = &["lo", "docker", "br-", "veth", "virbr", "tailscale", "tun", "wg"];

/// One interface address (input to the pure picker).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceAddr {
    pub name: String,
    pub ip: Ipv4Addr,
}

/// Picks the LAN address from `candidates`: drops excluded prefixes and non-private addresses (RFC 1918
/// only; not `100.64/10`); prefers `192.168/16`, then `10/8`, then `172.16/12`; ties by interface name.
#[must_use]
pub fn pick_lan_ipv4(candidates: &[InterfaceAddr]) -> Option<Ipv4Addr> {
    todo!()
}

/// [`pick_lan_ipv4`] over `if_addrs::get_if_addrs()` (errors → `None`, logged).
#[must_use]
pub fn lan_ipv4() -> Option<Ipv4Addr> {
    todo!()
}
