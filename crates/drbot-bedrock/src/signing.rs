//! AWS Signature Version 4 signing.
//!
//! Implements the AWS SigV4 signing process for Bedrock API requests.

use chrono::Utc;
use ring::hmac;
use std::collections::BTreeMap;

/// AWS credentials.
#[derive(Debug, Clone)]
pub struct AwsCredentials {
    /// Access key ID.
    pub access_key_id: String,
    /// Secret access key.
    pub secret_access_key: String,
    /// Session token (optional, for temporary credentials).
    pub session_token: Option<String>,
}

impl AwsCredentials {
    /// Load credentials from environment variables.
    pub fn from_env() -> Option<Self> {
        let access_key_id = std::env::var("AWS_ACCESS_KEY_ID").ok()?;
        let secret_access_key = std::env::var("AWS_SECRET_ACCESS_KEY").ok()?;
        let session_token = std::env::var("AWS_SESSION_TOKEN").ok();

        Some(Self {
            access_key_id,
            secret_access_key,
            session_token,
        })
    }

    /// Create credentials directly.
    pub fn new(access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
        }
    }

    /// Add session token.
    pub fn with_session_token(mut self, token: impl Into<String>) -> Self {
        self.session_token = Some(token.into());
        self
    }
}

/// Sign an AWS request using SigV4.
pub struct AwsSigner {
    /// Service name.
    service: String,
    /// Region.
    region: String,
    /// Credentials.
    credentials: AwsCredentials,
}

impl AwsSigner {
    /// Create a new signer.
    pub fn new(
        service: impl Into<String>,
        region: impl Into<String>,
        credentials: AwsCredentials,
    ) -> Self {
        Self {
            service: service.into(),
            region: region.into(),
            credentials,
        }
    }

    /// Sign a request and return the required headers.
    pub fn sign(
        &self,
        method: &str,
        url: &str,
        headers: &BTreeMap<String, String>,
        body: &[u8],
    ) -> BTreeMap<String, String> {
        let now = Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

        // Parse URL
        let parsed = url::Url::parse(url).expect("Invalid URL");
        let host = parsed.host_str().unwrap_or_default();
        let canonical_uri = parsed.path();
        let canonical_querystring = parsed.query().unwrap_or_default();

        // Create signed headers
        let mut signed_headers: BTreeMap<String, String> = headers.clone();
        signed_headers.insert("host".to_string(), host.to_string());
        signed_headers.insert("x-amz-date".to_string(), amz_date.clone());

        if let Some(token) = &self.credentials.session_token {
            signed_headers.insert("x-amz-security-token".to_string(), token.clone());
        }

        // Hash the payload
        let payload_hash = hex::encode(ring::digest::digest(&ring::digest::SHA256, body));
        signed_headers.insert("x-amz-content-sha256".to_string(), payload_hash.clone());

        // Build canonical headers
        let signed_header_names: Vec<&str> = signed_headers.keys().map(|k| k.as_str()).collect();
        let signed_headers_str = signed_header_names.join(";");

        let canonical_headers: String = signed_headers
            .iter()
            .map(|(k, v)| format!("{}:{}\n", k.to_lowercase(), v.trim()))
            .collect();

        // Create canonical request
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            canonical_uri,
            canonical_querystring,
            canonical_headers,
            signed_headers_str,
            payload_hash
        );

        // Create string to sign
        let algorithm = "AWS4-HMAC-SHA256";
        let credential_scope = format!(
            "{}/{}/{}/aws4_request",
            date_stamp, self.region, self.service
        );
        let canonical_request_hash = hex::encode(ring::digest::digest(
            &ring::digest::SHA256,
            canonical_request.as_bytes(),
        ));
        let string_to_sign = format!(
            "{}\n{}\n{}\n{}",
            algorithm, amz_date, credential_scope, canonical_request_hash
        );

        // Calculate signature
        let signature = self.calculate_signature(&date_stamp, &string_to_sign);

        // Build authorization header
        let authorization = format!(
            "{} Credential={}/{}, SignedHeaders={}, Signature={}",
            algorithm,
            self.credentials.access_key_id,
            credential_scope,
            signed_headers_str,
            signature
        );

        // Return headers to add
        let mut result = BTreeMap::new();
        result.insert("Authorization".to_string(), authorization);
        result.insert("x-amz-date".to_string(), amz_date);
        result.insert("x-amz-content-sha256".to_string(), payload_hash);
        if let Some(token) = &self.credentials.session_token {
            result.insert("x-amz-security-token".to_string(), token.clone());
        }

        result
    }

    /// Calculate the signature.
    fn calculate_signature(&self, date_stamp: &str, string_to_sign: &str) -> String {
        let k_secret = format!("AWS4{}", self.credentials.secret_access_key);
        let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes());
        let k_region = hmac_sha256(&k_date, self.region.as_bytes());
        let k_service = hmac_sha256(&k_region, self.service.as_bytes());
        let k_signing = hmac_sha256(&k_service, b"aws4_request");
        let signature = hmac_sha256(&k_signing, string_to_sign.as_bytes());
        hex::encode(signature)
    }
}

/// HMAC-SHA256.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hmac::sign(&key, data).as_ref().to_vec()
}
