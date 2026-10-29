//! The script injected into every HTML page a project port serves (design-harness spec §6.2).
//!
//! It reports scroll/size/hits to the frame host, applies the host's scroll and navigation requests, saves the
//! scroll position across reloads (session storage, keyed by pathname) and announces route changes. The source is
//! [`BRIDGE_JS`]; every project port serves the same bytes at [`super::BRIDGE_PATH`].

/// The bridge source. The same code for every project, and it reveals nothing, so it is served unguarded.
pub const BRIDGE_JS: &str = include_str!("bridge.js");

#[cfg(test)]
mod tests;
