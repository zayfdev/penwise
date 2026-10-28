//! Reachable addresses for this machine: the LAN IP, the Tailscale name, loopback.

use std::time::{Duration, Instant};

use tokio::process::Command;
use tokio::sync::Mutex;

/// How long a discovered address list is reused (the network can change: wifi roam, VPN up, login before DHCP).
const CACHE_FOR: Duration = Duration::from_secs(30);
/// Each helper process (`ip`, `tailscale`) gets this long; a hung daemon must not hang `/pair`.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

/// Where the server is reachable, recomputed on demand so a late or changed network is picked up.
#[derive(Debug)]
pub struct Urls {
    port: u16,
    /// `Some`: use exactly these (tests, embedding); `None`: ask the machine.
    fixed: Option<Vec<String>>,
    cache: Mutex<Option<(Instant, Vec<String>)>>,
}

impl Urls {
    /// Discovers addresses per request (cached about 30 s).
    #[must_use]
    pub fn discovered(port: u16) -> Self {
        Self {
            port,
            fixed: None,
            cache: Mutex::new(None),
        }
    }

    /// Always answers `urls`.
    #[must_use]
    pub fn fixed(urls: Vec<String>) -> Self {
        Self {
            port: 0,
            fixed: Some(urls),
            cache: Mutex::new(None),
        }
    }

    /// Every URL this machine is reachable at, best first: LAN address, Tailscale name, loopback.
    pub async fn get(&self) -> Vec<String> {
        if let Some(fixed) = &self.fixed {
            return fixed.clone();
        }
        let mut cache = self.cache.lock().await;
        if let Some((at, urls)) = cache.as_ref()
            && at.elapsed() < CACHE_FOR
        {
            return urls.clone();
        }
        let urls = platform_urls(self.port).await;
        // A list with only loopback means the network is not up yet: look again on the next request.
        if urls.len() > 1 {
            *cache = Some((Instant::now(), urls.clone()));
        }
        urls
    }
}

/// Runs `program args…` and returns stdout on success; `None` on failure, absence or timeout.
async fn run(program: &str, args: &[&str]) -> Option<Vec<u8>> {
    let mut command = Command::new(program);
    command
        .args(args)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null());
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    output.status.success().then_some(output.stdout)
}

/// Every URL this machine is reachable at, best first: LAN address, Tailscale name, loopback.
///
/// The tablet must not be handed `127.0.0.1`, so the LAN address is discovered rather than assumed.
pub async fn platform_urls(port: u16) -> Vec<String> {
    let (lan, tailscale) = tokio::join!(lan_address(), tailscale_name());
    let mut urls = Vec::new();
    if let Some(lan) = lan {
        urls.push(format!("http://{lan}:{port}"));
    }
    if let Some(name) = tailscale {
        urls.push(format!("http://{name}:{port}"));
    }
    urls.push(format!("http://127.0.0.1:{port}"));
    urls
}

async fn tailscale_name() -> Option<String> {
    let stdout = run("tailscale", &["status", "--json"]).await?;
    let json = serde_json::from_slice::<serde_json::Value>(&stdout).ok()?;
    let name = json.get("Self")?.get("DNSName")?.as_str()?;
    Some(name.trim_end_matches('.').to_owned())
}

/// This host's primary IPv4 address, by asking the routing table which source an outside route would use.
async fn lan_address() -> Option<String> {
    let stdout = run("ip", &["-4", "route", "get", "1.1.1.1"]).await?;
    source_of(&String::from_utf8_lossy(&stdout))
}

/// The word after `src` in `ip route get` output.
fn source_of(text: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    while let Some(word) = words.next() {
        if word == "src" {
            return words.next().map(str::to_owned);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_address_follows_src() {
        let line = "1.1.1.1 via 192.168.1.1 dev wlan0 src 192.168.1.19 uid 1000\n    cache";
        assert_eq!(Some("192.168.1.19".to_owned()), source_of(line));
        assert_eq!(None, source_of("RTNETLINK answers: Network is unreachable"));
    }

    #[tokio::test]
    async fn fixed_urls_are_returned_as_given() {
        let urls = Urls::fixed(vec!["http://10.0.0.2:7420".to_owned()]);
        assert_eq!(vec!["http://10.0.0.2:7420".to_owned()], urls.get().await);
    }

    #[tokio::test]
    async fn a_missing_program_is_none_not_a_hang() {
        assert!(run("penwise-no-such-program", &[]).await.is_none());
    }
}
