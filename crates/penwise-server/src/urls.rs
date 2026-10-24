//! Reachable addresses for this machine: the LAN IP, the Tailscale name, loopback.

/// Every URL this machine is reachable at, best first: LAN address, Tailscale name, loopback.
///
/// The tablet must not be handed `127.0.0.1`, so the LAN address is discovered rather than assumed.
pub fn platform_urls(port: u16) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(lan) = lan_address() {
        urls.push(format!("http://{lan}:{port}"));
    }
    if let Ok(output) = std::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        && output.status.success()
        && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        && let Some(name) = json
            .get("Self")
            .and_then(|s| s.get("DNSName"))
            .and_then(|d| d.as_str())
    {
        let name = name.trim_end_matches('.');
        urls.push(format!("http://{name}:{port}"));
    }
    urls.push(format!("http://127.0.0.1:{port}"));
    urls
}

/// This host's primary IPv4 address, by asking the routing table which source an outside route would use.
fn lan_address() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["-4", "route", "get", "1.1.1.1"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut words = text.split_whitespace();
    while let Some(word) = words.next() {
        if word == "src" {
            return words.next().map(str::to_owned);
        }
    }
    None
}
