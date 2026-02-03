//! URL parsing and manipulation for drbot.
//!
//! This crate provides:
//! - URL parsing
//! - URL building
//! - Query string handling

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// URL error types.
#[derive(Error, Debug)]
pub enum UrlError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Missing scheme")]
    MissingScheme,

    #[error("Missing host")]
    MissingHost,

    #[error("Invalid port: {0}")]
    InvalidPort(String),

    #[error("Invalid encoding: {0}")]
    InvalidEncoding(String),
}

/// Result type for URL operations.
pub type Result<T> = std::result::Result<T, UrlError>;

/// Parsed URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Url {
    /// URL scheme (http, https, etc.).
    pub scheme: String,
    /// Username (if present).
    pub username: Option<String>,
    /// Password (if present).
    pub password: Option<String>,
    /// Host (domain or IP).
    pub host: String,
    /// Port (if present).
    pub port: Option<u16>,
    /// Path.
    pub path: String,
    /// Query string (without ?).
    pub query: Option<String>,
    /// Fragment (without #).
    pub fragment: Option<String>,
}

impl Url {
    /// Parse URL from string.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Extract fragment
        let (s, fragment) = if let Some(pos) = s.rfind('#') {
            (&s[..pos], Some(s[pos + 1..].to_string()))
        } else {
            (s, None)
        };

        // Extract query
        let (s, query) = if let Some(pos) = s.rfind('?') {
            (&s[..pos], Some(s[pos + 1..].to_string()))
        } else {
            (s, None)
        };

        // Extract scheme
        let (scheme, rest) = s.split_once("://").ok_or_else(|| UrlError::MissingScheme)?;

        // Extract path
        let (authority, path) = if let Some(pos) = rest.find('/') {
            (&rest[..pos], rest[pos..].to_string())
        } else {
            (rest, "/".to_string())
        };

        // Extract userinfo
        let (userinfo, host_port) = if let Some(pos) = authority.rfind('@') {
            (Some(&authority[..pos]), &authority[pos + 1..])
        } else {
            (None, authority)
        };

        // Parse userinfo
        let (username, password) = if let Some(ui) = userinfo {
            if let Some(pos) = ui.find(':') {
                (Some(ui[..pos].to_string()), Some(ui[pos + 1..].to_string()))
            } else {
                (Some(ui.to_string()), None)
            }
        } else {
            (None, None)
        };

        // Extract port
        let (host, port) = if host_port.starts_with('[') {
            // IPv6
            if let Some(pos) = host_port.rfind(']') {
                let host = &host_port[1..pos];
                let port_str = &host_port[pos + 1..];
                let port = if port_str.starts_with(':') {
                    Some(
                        port_str[1..]
                            .parse()
                            .map_err(|_| UrlError::InvalidPort(port_str.to_string()))?,
                    )
                } else {
                    None
                };
                (host.to_string(), port)
            } else {
                return Err(UrlError::InvalidUrl("Invalid IPv6 address".to_string()));
            }
        } else if let Some(pos) = host_port.rfind(':') {
            let host = &host_port[..pos];
            let port = host_port[pos + 1..]
                .parse()
                .map_err(|_| UrlError::InvalidPort(host_port[pos + 1..].to_string()))?;
            (host.to_string(), Some(port))
        } else {
            (host_port.to_string(), None)
        };

        if host.is_empty() {
            return Err(UrlError::MissingHost);
        }

        Ok(Self {
            scheme: scheme.to_lowercase(),
            username,
            password,
            host,
            port,
            path,
            query,
            fragment,
        })
    }

    /// Get the origin (scheme + host + port).
    pub fn origin(&self) -> String {
        if let Some(port) = self.port {
            format!("{}://{}:{}", self.scheme, self.host, port)
        } else {
            format!("{}://{}", self.scheme, self.host)
        }
    }

    /// Get the authority (userinfo + host + port).
    pub fn authority(&self) -> String {
        let mut auth = String::new();

        if let Some(ref user) = self.username {
            auth.push_str(user);
            if let Some(ref pass) = self.password {
                auth.push(':');
                auth.push_str(pass);
            }
            auth.push('@');
        }

        auth.push_str(&self.host);

        if let Some(port) = self.port {
            auth.push(':');
            auth.push_str(&port.to_string());
        }

        auth
    }

    /// Get the full URL as string.
    pub fn to_string(&self) -> String {
        let mut url = format!("{}://{}", self.scheme, self.authority());
        url.push_str(&self.path);

        if let Some(ref query) = self.query {
            url.push('?');
            url.push_str(query);
        }

        if let Some(ref fragment) = self.fragment {
            url.push('#');
            url.push_str(fragment);
        }

        url
    }

    /// Get effective port (default based on scheme).
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or_else(|| match self.scheme.as_str() {
            "http" => 80,
            "https" => 443,
            "ftp" => 21,
            "ssh" => 22,
            "ws" => 80,
            "wss" => 443,
            _ => 80,
        })
    }

    /// Check if URL is secure (https, wss, etc.).
    pub fn is_secure(&self) -> bool {
        matches!(self.scheme.as_str(), "https" | "wss" | "sftp")
    }

    /// Get query parameters as HashMap.
    pub fn query_params(&self) -> HashMap<String, String> {
        self.query
            .as_ref()
            .map(|q| QueryString::parse(q))
            .unwrap_or_default()
    }

    /// Set query parameter.
    pub fn set_query_param(&mut self, key: &str, value: &str) {
        let mut params = self.query_params();
        params.insert(key.to_string(), value.to_string());
        self.query = Some(QueryString::stringify(&params));
    }

    /// Remove query parameter.
    pub fn remove_query_param(&mut self, key: &str) {
        let mut params = self.query_params();
        params.remove(key);
        self.query = if params.is_empty() {
            None
        } else {
            Some(QueryString::stringify(&params))
        };
    }

    /// Join with relative path.
    pub fn join(&self, path: &str) -> Result<Self> {
        if path.contains("://") {
            return Self::parse(path);
        }

        let mut new = self.clone();

        if path.starts_with('/') {
            new.path = path.to_string();
            new.query = None;
            new.fragment = None;
        } else if path.starts_with('?') {
            new.query = Some(path[1..].to_string());
            new.fragment = None;
        } else if path.starts_with('#') {
            new.fragment = Some(path[1..].to_string());
        } else {
            // Relative path
            let base_dir = if self.path.ends_with('/') {
                self.path.clone()
            } else if let Some(pos) = self.path.rfind('/') {
                self.path[..=pos].to_string()
            } else {
                "/".to_string()
            };
            new.path = format!("{}{}", base_dir, path);
            new.query = None;
            new.fragment = None;
        }

        Ok(new)
    }
}

impl FromStr for Url {
    type Err = UrlError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

/// URL builder.
pub struct UrlBuilder {
    scheme: String,
    username: Option<String>,
    password: Option<String>,
    host: String,
    port: Option<u16>,
    path: String,
    query: HashMap<String, String>,
    fragment: Option<String>,
}

impl UrlBuilder {
    /// Create new URL builder.
    pub fn new(scheme: &str, host: &str) -> Self {
        Self {
            scheme: scheme.to_string(),
            username: None,
            password: None,
            host: host.to_string(),
            port: None,
            path: "/".to_string(),
            query: HashMap::new(),
            fragment: None,
        }
    }

    /// Create HTTPS URL builder.
    pub fn https(host: &str) -> Self {
        Self::new("https", host)
    }

    /// Create HTTP URL builder.
    pub fn http(host: &str) -> Self {
        Self::new("http", host)
    }

    /// Set username.
    pub fn username(mut self, username: &str) -> Self {
        self.username = Some(username.to_string());
        self
    }

    /// Set password.
    pub fn password(mut self, password: &str) -> Self {
        self.password = Some(password.to_string());
        self
    }

    /// Set port.
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set path.
    pub fn path(mut self, path: &str) -> Self {
        self.path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };
        self
    }

    /// Add path segment.
    pub fn path_segment(mut self, segment: &str) -> Self {
        if !self.path.ends_with('/') {
            self.path.push('/');
        }
        self.path.push_str(&encode_path_segment(segment));
        self
    }

    /// Add query parameter.
    pub fn query(mut self, key: &str, value: &str) -> Self {
        self.query.insert(key.to_string(), value.to_string());
        self
    }

    /// Set fragment.
    pub fn fragment(mut self, fragment: &str) -> Self {
        self.fragment = Some(fragment.to_string());
        self
    }

    /// Build the URL.
    pub fn build(self) -> Url {
        Url {
            scheme: self.scheme,
            username: self.username,
            password: self.password,
            host: self.host,
            port: self.port,
            path: self.path,
            query: if self.query.is_empty() {
                None
            } else {
                Some(QueryString::stringify(&self.query))
            },
            fragment: self.fragment,
        }
    }

    /// Build and convert to string.
    pub fn to_string(self) -> String {
        self.build().to_string()
    }
}

/// Query string utilities.
pub struct QueryString;

impl QueryString {
    /// Parse query string into HashMap.
    pub fn parse(s: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();
        let s = s.trim_start_matches('?');

        for pair in s.split('&') {
            if pair.is_empty() {
                continue;
            }

            if let Some(pos) = pair.find('=') {
                let key = decode(&pair[..pos]).unwrap_or_else(|_| pair[..pos].to_string());
                let value =
                    decode(&pair[pos + 1..]).unwrap_or_else(|_| pair[pos + 1..].to_string());
                params.insert(key, value);
            } else {
                let key = decode(pair).unwrap_or_else(|_| pair.to_string());
                params.insert(key, String::new());
            }
        }

        params
    }

    /// Stringify HashMap to query string.
    pub fn stringify(params: &HashMap<String, String>) -> String {
        params
            .iter()
            .map(|(k, v)| {
                if v.is_empty() {
                    encode(k)
                } else {
                    format!("{}={}", encode(k), encode(v))
                }
            })
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// URL encode string.
pub fn encode(s: &str) -> String {
    let mut result = String::new();

    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }

    result
}

/// URL decode string.
pub fn decode(s: &str) -> Result<String> {
    let mut result = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() != 2 {
                return Err(UrlError::InvalidEncoding(format!(
                    "Incomplete escape: %{}",
                    hex
                )));
            }
            let byte = u8::from_str_radix(&hex, 16)
                .map_err(|_| UrlError::InvalidEncoding(format!("Invalid hex: %{}", hex)))?;
            result.push(byte);
        } else if c == '+' {
            result.push(b' ');
        } else {
            for b in c.to_string().as_bytes() {
                result.push(*b);
            }
        }
    }

    String::from_utf8(result).map_err(|e| UrlError::InvalidEncoding(e.to_string()))
}

/// URL encode path segment.
pub fn encode_path_segment(s: &str) -> String {
    let mut result = String::new();

    for c in s.chars() {
        match c {
            'A'..='Z'
            | 'a'..='z'
            | '0'..='9'
            | '-'
            | '_'
            | '.'
            | '~'
            | '!'
            | '$'
            | '&'
            | '\''
            | '('
            | ')'
            | '*'
            | '+'
            | ','
            | ';'
            | '='
            | ':'
            | '@' => result.push(c),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let url = Url::parse("https://example.com").unwrap();
        assert_eq!(url.scheme, "https");
        assert_eq!(url.host, "example.com");
        assert_eq!(url.path, "/");
    }

    #[test]
    fn test_parse_full() {
        let url = Url::parse("https://user:pass@example.com:8080/path?query=1#frag").unwrap();
        assert_eq!(url.scheme, "https");
        assert_eq!(url.username, Some("user".to_string()));
        assert_eq!(url.password, Some("pass".to_string()));
        assert_eq!(url.host, "example.com");
        assert_eq!(url.port, Some(8080));
        assert_eq!(url.path, "/path");
        assert_eq!(url.query, Some("query=1".to_string()));
        assert_eq!(url.fragment, Some("frag".to_string()));
    }

    #[test]
    fn test_parse_ipv6() {
        let url = Url::parse("http://[::1]:8080/path").unwrap();
        assert_eq!(url.host, "::1");
        assert_eq!(url.port, Some(8080));
    }

    #[test]
    fn test_url_builder() {
        let url = UrlBuilder::https("api.example.com")
            .port(443)
            .path_segment("v1")
            .path_segment("users")
            .query("page", "1")
            .query("limit", "10")
            .build();

        assert_eq!(url.scheme, "https");
        assert_eq!(url.host, "api.example.com");
        assert!(url.path.contains("v1"));
        assert!(url.path.contains("users"));
    }

    #[test]
    fn test_query_params() {
        let url = Url::parse("https://example.com?a=1&b=2").unwrap();
        let params = url.query_params();
        assert_eq!(params.get("a"), Some(&"1".to_string()));
        assert_eq!(params.get("b"), Some(&"2".to_string()));
    }

    #[test]
    fn test_encode_decode() {
        let original = "hello world!";
        let encoded = encode(original);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_join() {
        let base = Url::parse("https://example.com/a/b").unwrap();

        let joined = base.join("/c/d").unwrap();
        assert_eq!(joined.path, "/c/d");

        let joined = base.join("c/d").unwrap();
        assert_eq!(joined.path, "/a/c/d");
    }

    #[test]
    fn test_effective_port() {
        let url = Url::parse("https://example.com").unwrap();
        assert_eq!(url.effective_port(), 443);

        let url = Url::parse("http://example.com").unwrap();
        assert_eq!(url.effective_port(), 80);

        let url = Url::parse("https://example.com:8443").unwrap();
        assert_eq!(url.effective_port(), 8443);
    }
}
