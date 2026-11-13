//! HTTP Basic Auth header encoding and API version pinning (`2024-01-01`).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use zeroize::Zeroize;

use crate::kernel::auth::{AuthKey, PublishableKey, SecretKey};
use crate::kernel::error::MoyasarError;

/// Pinned Moyasar API version header value.
pub const API_VERSION: &str = "2024-01-01";

/// Custom Moyasar API version header name.
pub const MOYASAR_VERSION_HEADER: &str = "moyasar-version";

/// Encodes an `AuthKey` into an HTTP Basic Auth `HeaderValue`:
/// `Basic base64("{api_key}:")` with a trailing colon and empty password.
pub fn encode_basic_auth(key: &dyn AuthKey) -> Result<HeaderValue, MoyasarError> {
    let mut raw = format!("{}:", key.expose_secret());
    let encoded = STANDARD.encode(raw.as_bytes());
    raw.zeroize();

    let header_str = format!("Basic {encoded}");
    HeaderValue::from_str(&header_str).map_err(|_| MoyasarError::InvalidKey)
}

/// Builds default headers (`Authorization`, `Moyasar-Version`, `Accept`) for all requests.
pub fn build_default_headers(key: &dyn AuthKey) -> Result<HeaderMap, MoyasarError> {
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, encode_basic_auth(key)?);
    headers.insert(
        HeaderName::from_static(MOYASAR_VERSION_HEADER),
        HeaderValue::from_static(API_VERSION),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    Ok(headers)
}

impl TryFrom<&SecretKey> for HeaderValue {
    type Error = MoyasarError;

    fn try_from(key: &SecretKey) -> Result<Self, Self::Error> {
        encode_basic_auth(key)
    }
}

impl TryFrom<&PublishableKey> for HeaderValue {
    type Error = MoyasarError;

    fn try_from(key: &PublishableKey) -> Result<Self, Self::Error> {
        encode_basic_auth(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_basic_auth_with_trailing_colon() {
        let key = SecretKey::new("sk_test_YOUR_SECRET_KEY").expect("valid key");
        let header = HeaderValue::try_from(&key).expect("encodes header");
        let expected_b64 = STANDARD.encode("sk_test_YOUR_SECRET_KEY:");
        assert_eq!(header.to_str().unwrap(), format!("Basic {expected_b64}"));
    }

    #[test]
    fn sets_pinned_api_version_header() {
        let key = PublishableKey::new("pk_test_abc").expect("valid key");
        let headers = build_default_headers(&key).expect("headers built");
        assert_eq!(headers.get(MOYASAR_VERSION_HEADER).unwrap(), API_VERSION);
    }
}
