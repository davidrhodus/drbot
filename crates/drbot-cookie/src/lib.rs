//! Cookie parsing for drbot.
//!
//! This crate provides:
//! - Cookie parsing
//! - Cookie building
//! - Cookie jar management

use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

/// Cookie error types.
#[derive(Error, Debug)]
pub enum CookieError {
    #[error("Invalid cookie: {0}")]
    Invalid(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Result type for cookie operations.
pub type Result<T> = std::result::Result<T, CookieError>;

/// SameSite attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SameSite {
    /// Cookie is sent with all requests.
    None,
    /// Cookie is sent with same-site and top-level navigation.
    #[default]
    Lax,
    /// Cookie is only sent with same-site requests.
    Strict,
}

impl fmt::Display for SameSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SameSite::None => write!(f, "None"),
            SameSite::Lax => write!(f, "Lax"),
            SameSite::Strict => write!(f, "Strict"),
        }
    }
}

/// HTTP Cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Domain attribute.
    pub domain: Option<String>,
    /// Path attribute.
    pub path: Option<String>,
    /// Expires attribute.
    pub expires: Option<DateTime<Utc>>,
    /// Max-Age attribute (in seconds).
    pub max_age: Option<i64>,
    /// Secure attribute.
    pub secure: bool,
    /// HttpOnly attribute.
    pub http_only: bool,
    /// SameSite attribute.
    pub same_site: Option<SameSite>,
}

impl Cookie {
    /// Create new cookie.
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            domain: None,
            path: None,
            expires: None,
            max_age: None,
            secure: false,
            http_only: false,
            same_site: None,
        }
    }

    /// Parse cookie from Set-Cookie header value.
    pub fn parse(s: &str) -> Result<Self> {
        let mut parts = s.split(';');

        // First part is name=value
        let first = parts
            .next()
            .ok_or_else(|| CookieError::Invalid(s.to_string()))?;
        let (name, value) = first
            .split_once('=')
            .ok_or_else(|| CookieError::Invalid(s.to_string()))?;

        let mut cookie = Self::new(name.trim(), value.trim());

        // Parse attributes
        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some((key, val)) = part.split_once('=') {
                let key = key.trim().to_lowercase();
                let val = val.trim();

                match key.as_str() {
                    "domain" => cookie.domain = Some(val.to_string()),
                    "path" => cookie.path = Some(val.to_string()),
                    "expires" => {
                        // Try to parse HTTP date format
                        if let Ok(dt) = Self::parse_http_date(val) {
                            cookie.expires = Some(dt);
                        }
                    }
                    "max-age" => {
                        if let Ok(age) = val.parse() {
                            cookie.max_age = Some(age);
                        }
                    }
                    "samesite" => {
                        cookie.same_site = match val.to_lowercase().as_str() {
                            "strict" => Some(SameSite::Strict),
                            "lax" => Some(SameSite::Lax),
                            "none" => Some(SameSite::None),
                            _ => None,
                        };
                    }
                    _ => {}
                }
            } else {
                let key = part.to_lowercase();
                match key.as_str() {
                    "secure" => cookie.secure = true,
                    "httponly" => cookie.http_only = true,
                    _ => {}
                }
            }
        }

        Ok(cookie)
    }

    fn parse_http_date(s: &str) -> Result<DateTime<Utc>> {
        // Try RFC 2822 format
        DateTime::parse_from_rfc2822(s)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                // Try common cookie date format: "Wed, 09 Jun 2021 10:18:14 GMT"
                DateTime::parse_from_str(s, "%a, %d %b %Y %H:%M:%S GMT")
                    .map(|dt| dt.with_timezone(&Utc))
            })
            .map_err(|e| CookieError::ParseError(e.to_string()))
    }

    /// Set domain attribute.
    pub fn domain(mut self, domain: &str) -> Self {
        self.domain = Some(domain.to_string());
        self
    }

    /// Set path attribute.
    pub fn path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    /// Set expires attribute.
    pub fn expires(mut self, expires: DateTime<Utc>) -> Self {
        self.expires = Some(expires);
        self
    }

    /// Set max-age attribute.
    pub fn max_age(mut self, seconds: i64) -> Self {
        self.max_age = Some(seconds);
        self
    }

    /// Set max-age from duration.
    pub fn max_age_duration(mut self, duration: Duration) -> Self {
        self.max_age = Some(duration.num_seconds());
        self
    }

    /// Set secure attribute.
    pub fn secure(mut self) -> Self {
        self.secure = true;
        self
    }

    /// Set httponly attribute.
    pub fn http_only(mut self) -> Self {
        self.http_only = true;
        self
    }

    /// Set SameSite attribute.
    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = Some(same_site);
        self
    }

    /// Check if cookie is expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires {
            return expires < Utc::now();
        }
        if let Some(max_age) = self.max_age {
            return max_age <= 0;
        }
        false
    }

    /// Check if cookie is session cookie (no expires or max-age).
    pub fn is_session(&self) -> bool {
        self.expires.is_none() && self.max_age.is_none()
    }

    /// Convert to Set-Cookie header value.
    pub fn to_set_cookie_header(&self) -> String {
        let mut s = format!("{}={}", self.name, self.value);

        if let Some(ref domain) = self.domain {
            s.push_str(&format!("; Domain={}", domain));
        }

        if let Some(ref path) = self.path {
            s.push_str(&format!("; Path={}", path));
        }

        if let Some(expires) = self.expires {
            s.push_str(&format!(
                "; Expires={}",
                expires.format("%a, %d %b %Y %H:%M:%S GMT")
            ));
        }

        if let Some(max_age) = self.max_age {
            s.push_str(&format!("; Max-Age={}", max_age));
        }

        if self.secure {
            s.push_str("; Secure");
        }

        if self.http_only {
            s.push_str("; HttpOnly");
        }

        if let Some(same_site) = self.same_site {
            s.push_str(&format!("; SameSite={}", same_site));
        }

        s
    }

    /// Convert to Cookie header value (just name=value).
    pub fn to_cookie_header(&self) -> String {
        format!("{}={}", self.name, self.value)
    }
}

impl fmt::Display for Cookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.name, self.value)
    }
}

/// Cookie jar for storing and managing cookies.
#[derive(Debug, Clone, Default)]
pub struct CookieJar {
    cookies: HashMap<String, Cookie>,
}

impl CookieJar {
    /// Create new empty cookie jar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add cookie to jar.
    pub fn add(&mut self, cookie: Cookie) {
        self.cookies.insert(cookie.name.clone(), cookie);
    }

    /// Get cookie by name.
    pub fn get(&self, name: &str) -> Option<&Cookie> {
        self.cookies.get(name)
    }

    /// Remove cookie by name.
    pub fn remove(&mut self, name: &str) -> Option<Cookie> {
        self.cookies.remove(name)
    }

    /// Check if cookie exists.
    pub fn contains(&self, name: &str) -> bool {
        self.cookies.contains_key(name)
    }

    /// Get all cookies.
    pub fn all(&self) -> impl Iterator<Item = &Cookie> {
        self.cookies.values()
    }

    /// Get number of cookies.
    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    /// Check if jar is empty.
    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    /// Clear all cookies.
    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    /// Remove expired cookies.
    pub fn remove_expired(&mut self) {
        self.cookies.retain(|_, cookie| !cookie.is_expired());
    }

    /// Parse and add cookies from Set-Cookie headers.
    pub fn add_from_headers(&mut self, headers: &[&str]) {
        for header in headers {
            if let Ok(cookie) = Cookie::parse(header) {
                self.add(cookie);
            }
        }
    }

    /// Generate Cookie header value.
    pub fn to_cookie_header(&self) -> String {
        self.cookies
            .values()
            .filter(|c| !c.is_expired())
            .map(|c| c.to_cookie_header())
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Get cookies for a specific domain and path.
    pub fn cookies_for(&self, domain: &str, path: &str) -> Vec<&Cookie> {
        self.cookies
            .values()
            .filter(|cookie| {
                // Check domain
                if let Some(ref cookie_domain) = cookie.domain {
                    let matches =
                        domain == cookie_domain || domain.ends_with(&format!(".{}", cookie_domain));
                    if !matches {
                        return false;
                    }
                }

                // Check path
                if let Some(ref cookie_path) = cookie.path {
                    if !path.starts_with(cookie_path) {
                        return false;
                    }
                }

                // Check expiration
                !cookie.is_expired()
            })
            .collect()
    }
}

/// Parse cookies from Cookie header value.
pub fn parse_cookie_header(header: &str) -> HashMap<String, String> {
    let mut cookies = HashMap::new();

    for part in header.split(';') {
        let part = part.trim();
        if let Some((name, value)) = part.split_once('=') {
            cookies.insert(name.trim().to_string(), value.trim().to_string());
        }
    }

    cookies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_new() {
        let cookie = Cookie::new("session", "abc123");
        assert_eq!(cookie.name, "session");
        assert_eq!(cookie.value, "abc123");
    }

    #[test]
    fn test_cookie_parse() {
        let cookie = Cookie::parse("session=abc123; Path=/; Secure; HttpOnly").unwrap();
        assert_eq!(cookie.name, "session");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.path, Some("/".to_string()));
        assert!(cookie.secure);
        assert!(cookie.http_only);
    }

    #[test]
    fn test_cookie_parse_with_domain() {
        let cookie = Cookie::parse("id=123; Domain=example.com; Path=/api").unwrap();
        assert_eq!(cookie.domain, Some("example.com".to_string()));
        assert_eq!(cookie.path, Some("/api".to_string()));
    }

    #[test]
    fn test_cookie_parse_same_site() {
        let cookie = Cookie::parse("token=xyz; SameSite=Strict").unwrap();
        assert_eq!(cookie.same_site, Some(SameSite::Strict));

        let cookie = Cookie::parse("token=xyz; SameSite=Lax").unwrap();
        assert_eq!(cookie.same_site, Some(SameSite::Lax));
    }

    #[test]
    fn test_cookie_builder() {
        let cookie = Cookie::new("auth", "token123")
            .domain("example.com")
            .path("/")
            .max_age(3600)
            .secure()
            .http_only()
            .same_site(SameSite::Strict);

        assert_eq!(cookie.domain, Some("example.com".to_string()));
        assert_eq!(cookie.max_age, Some(3600));
        assert!(cookie.secure);
        assert!(cookie.http_only);
        assert_eq!(cookie.same_site, Some(SameSite::Strict));
    }

    #[test]
    fn test_cookie_to_set_cookie_header() {
        let cookie = Cookie::new("session", "abc").path("/").secure().http_only();

        let header = cookie.to_set_cookie_header();
        assert!(header.contains("session=abc"));
        assert!(header.contains("Path=/"));
        assert!(header.contains("Secure"));
        assert!(header.contains("HttpOnly"));
    }

    #[test]
    fn test_cookie_jar() {
        let mut jar = CookieJar::new();

        jar.add(Cookie::new("a", "1"));
        jar.add(Cookie::new("b", "2"));

        assert_eq!(jar.len(), 2);
        assert_eq!(jar.get("a").map(|c| c.value.as_str()), Some("1"));

        let header = jar.to_cookie_header();
        assert!(header.contains("a=1"));
        assert!(header.contains("b=2"));
    }

    #[test]
    fn test_parse_cookie_header() {
        let cookies = parse_cookie_header("session=abc123; user=john");
        assert_eq!(cookies.get("session"), Some(&"abc123".to_string()));
        assert_eq!(cookies.get("user"), Some(&"john".to_string()));
    }

    #[test]
    fn test_cookie_is_session() {
        let session_cookie = Cookie::new("session", "abc");
        assert!(session_cookie.is_session());

        let persistent_cookie = Cookie::new("pref", "value").max_age(3600);
        assert!(!persistent_cookie.is_session());
    }
}
