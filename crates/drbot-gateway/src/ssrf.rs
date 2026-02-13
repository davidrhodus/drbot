//! SSRF policy helpers (OpenClaw parity).
//!
//! OpenClaw blocks private-network URL targets by default for any surfaces that
//! can fetch remote resources (browser screenshots, remote skill sync, etc).

use drbot_protocol::openclaw::{error_codes, ErrorShape};
use serde_json::json;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, Default)]
pub(crate) struct SsrfPolicy {
    pub allow_private_network: bool,
    pub allowed_hostnames: HashSet<String>,
}

impl SsrfPolicy {
    pub(crate) fn from_env(
        allow_private_env: &[&str],
        allowed_hostnames_env: Option<&str>,
    ) -> Self {
        let allow_private_network = allow_private_env.iter().any(|k| env_truthy(k));
        let allowed_hostnames = allowed_hostnames_env
            .and_then(|k| std::env::var(k).ok())
            .unwrap_or_default()
            .split(',')
            .map(|v| normalize_hostname(v))
            .filter(|v| !v.is_empty())
            .collect::<HashSet<_>>();
        Self {
            allow_private_network,
            allowed_hostnames,
        }
    }
}

pub(crate) fn env_truthy(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn normalize_hostname(hostname: &str) -> String {
    let mut normalized = hostname.trim().to_ascii_lowercase();
    if normalized.ends_with('.') {
        normalized.pop();
    }
    if normalized.starts_with('[') && normalized.ends_with(']') && normalized.len() >= 2 {
        normalized = normalized[1..normalized.len() - 1].to_string();
    }
    normalized
}

fn is_blocked_hostname(hostname: &str) -> bool {
    let normalized = normalize_hostname(hostname);
    if normalized.is_empty() {
        return false;
    }
    if normalized == "localhost" || normalized == "metadata.google.internal" {
        return true;
    }
    normalized.ends_with(".localhost")
        || normalized.ends_with(".local")
        || normalized.ends_with(".internal")
}

fn ipv4_private(v4: Ipv4Addr) -> bool {
    let [a, b, ..] = v4.octets();
    a == 0
        || a == 10
        || a == 127
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 100 && (64..=127).contains(&b))
}

fn ipv6_private(v6: Ipv6Addr) -> bool {
    if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
        return true;
    }
    let [first, ..] = v6.segments();
    // fc00::/7 unique local
    if (first & 0xfe00) == 0xfc00 {
        return true;
    }
    // fe80::/10 link-local
    if (first & 0xffc0) == 0xfe80 {
        return true;
    }
    // fec0::/10 site-local (deprecated, but keep blocked)
    if (first & 0xffc0) == 0xfec0 {
        return true;
    }
    false
}

pub(crate) fn ip_blocked_by_ssrf_policy(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            ipv4_private(v4)
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4() {
                return ip_blocked_by_ssrf_policy(IpAddr::V4(mapped));
            }
            ipv6_private(v6)
        }
    }
}

pub(crate) async fn ensure_url_allowed(
    url: &str,
    policy: &SsrfPolicy,
) -> Result<reqwest::Url, ErrorShape> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|e| {
        ErrorShape::new(error_codes::INVALID_REQUEST, format!("invalid url: {}", e))
    })?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                format!("unsupported url scheme: {}", other),
            ));
        }
    }

    let host = parsed.host_str().unwrap_or("");
    let host = normalize_hostname(host);
    if host.is_empty() {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "url missing host",
        ));
    }

    let is_explicit_allowed = policy.allowed_hostnames.contains(&host);
    if !policy.allow_private_network && !is_explicit_allowed {
        if is_blocked_hostname(&host) {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                "url blocked by SSRF policy",
            )
            .with_details(json!({ "host": host })));
        }

        if let Ok(ip) = host.parse::<IpAddr>() {
            if ip_blocked_by_ssrf_policy(ip) {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    "url blocked by SSRF policy",
                )
                .with_details(json!({ "host": host, "ip": ip.to_string() })));
            }
            return Ok(parsed);
        }

        let port = parsed.port_or_known_default().unwrap_or_else(|| {
            if parsed.scheme() == "https" {
                443
            } else {
                80
            }
        });

        let mut resolved: Vec<String> = Vec::new();
        let addrs = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| {
                ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    format!("failed to resolve host: {}", e),
                )
            })?;

        for addr in addrs {
            let ip = addr.ip();
            let ip_str = ip.to_string();
            if !resolved.contains(&ip_str) {
                resolved.push(ip_str.clone());
            }
            if ip_blocked_by_ssrf_policy(ip) {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    "url blocked by SSRF policy",
                )
                .with_details(json!({ "host": host, "ip": ip_str, "resolved": resolved })));
            }
        }
    }

    Ok(parsed)
}

pub(crate) async fn ensure_ws_url_allowed(
    url: &str,
    policy: &SsrfPolicy,
) -> Result<reqwest::Url, ErrorShape> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|e| {
        ErrorShape::new(error_codes::INVALID_REQUEST, format!("invalid url: {}", e))
    })?;

    match parsed.scheme() {
        "ws" | "wss" => {}
        other => {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                format!("unsupported url scheme: {}", other),
            ));
        }
    }

    let host = parsed.host_str().unwrap_or("");
    let host = normalize_hostname(host);
    if host.is_empty() {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "url missing host",
        ));
    }

    let is_explicit_allowed = policy.allowed_hostnames.contains(&host);
    if !policy.allow_private_network && !is_explicit_allowed {
        if is_blocked_hostname(&host) {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                "url blocked by SSRF policy",
            )
            .with_details(json!({ "host": host })));
        }

        if let Ok(ip) = host.parse::<IpAddr>() {
            if ip_blocked_by_ssrf_policy(ip) {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    "url blocked by SSRF policy",
                )
                .with_details(json!({ "host": host, "ip": ip.to_string() })));
            }
            return Ok(parsed);
        }

        let port = parsed.port_or_known_default().unwrap_or_else(|| {
            if parsed.scheme() == "wss" {
                443
            } else {
                80
            }
        });

        let mut resolved: Vec<String> = Vec::new();
        let addrs = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| {
                ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    format!("failed to resolve host: {}", e),
                )
            })?;

        for addr in addrs {
            let ip = addr.ip();
            let ip_str = ip.to_string();
            if !resolved.contains(&ip_str) {
                resolved.push(ip_str.clone());
            }
            if ip_blocked_by_ssrf_policy(ip) {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    "url blocked by SSRF policy",
                )
                .with_details(json!({ "host": host, "ip": ip_str, "resolved": resolved })));
            }
        }
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_blocked_hostnames() {
        assert!(is_blocked_hostname("localhost"));
        assert!(is_blocked_hostname("metadata.google.internal"));
        assert!(is_blocked_hostname("foo.local"));
        assert!(is_blocked_hostname("foo.internal"));
        assert!(is_blocked_hostname("foo.localhost"));
        assert!(!is_blocked_hostname("example.com"));
    }

    #[test]
    fn blocks_private_ips() {
        assert!(ip_blocked_by_ssrf_policy(IpAddr::V4(Ipv4Addr::new(
            127, 0, 0, 1
        ))));
        assert!(ip_blocked_by_ssrf_policy(IpAddr::V4(Ipv4Addr::new(
            10, 0, 0, 1
        ))));
        assert!(ip_blocked_by_ssrf_policy(IpAddr::V4(Ipv4Addr::new(
            192, 168, 1, 1
        ))));
        assert!(ip_blocked_by_ssrf_policy(IpAddr::V6(
            "::1".parse().unwrap()
        )));
        assert!(ip_blocked_by_ssrf_policy(IpAddr::V6(
            "fc00::1".parse().unwrap()
        )));
        assert!(ip_blocked_by_ssrf_policy(IpAddr::V6(
            "fe80::1".parse().unwrap()
        )));
        // IPv4-mapped IPv6 should be treated as IPv4.
        assert!(ip_blocked_by_ssrf_policy(IpAddr::V6(
            "::ffff:127.0.0.1".parse().unwrap()
        )));
        assert!(!ip_blocked_by_ssrf_policy(IpAddr::V4(Ipv4Addr::new(
            8, 8, 8, 8
        ))));
    }
}
