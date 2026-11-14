//! HTTP Basic Auth header encoding, API version pinning (`2024-01-01`), and retry transport.

use std::future::Future;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde::de::DeserializeOwned;
use zeroize::Zeroize;

use crate::kernel::auth::{AuthKey, PublishableKey, SecretKey};
use crate::kernel::error::MoyasarError;

/// Pinned Moyasar API version header value.
pub const API_VERSION: &str = "2024-01-01";

/// Custom Moyasar API version header name.
pub const MOYASAR_VERSION_HEADER: &str = "moyasar-version";

/// Encodes an `AuthKey` into an HTTP Basic Auth `HeaderValue`:
/// `Basic base64("{api_key}:")` with a trailing colon and empty password.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::auth::SecretKey;
/// use some_moyasar::kernel::transport::encode_basic_auth;
///
/// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY").unwrap();
/// let header = encode_basic_auth(&key).unwrap();
/// assert_eq!(
///     header.to_str().unwrap(),
///     "Basic c2tfdGVzdF9ZT1VSX1NFQ1JFVF9LRVk6"
/// );
/// assert!(header.is_sensitive());
/// ```
pub fn encode_basic_auth(key: &dyn AuthKey) -> Result<HeaderValue, MoyasarError> {
    let mut raw = format!("{}:", key.expose_secret());
    let mut encoded = STANDARD.encode(raw.as_bytes());
    raw.zeroize();

    let mut header_str = format!("Basic {encoded}");
    encoded.zeroize();

    let result = HeaderValue::from_str(&header_str).map_err(|_| MoyasarError::InvalidKey);
    header_str.zeroize();

    let mut header = result?;
    header.set_sensitive(true);
    Ok(header)
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

/// Executes an async request factory up to `max_network_retries` additional times
/// when [`MoyasarError::is_retryable`] is `true`, returning immediately on success
/// or non-retryable (e.g., 4xx) errors.
///
/// # Examples
///
/// ```
/// use std::sync::atomic::{AtomicU32, Ordering};
/// use some_moyasar::kernel::error::MoyasarError;
/// use some_moyasar::kernel::transport::execute_with_retry;
///
/// let attempts = AtomicU32::new(0);
/// let result = futures::executor::block_on(execute_with_retry(2, || async {
///     let count = attempts.fetch_add(1, Ordering::SeqCst);
///     if count < 2 {
///         Err(MoyasarError::Timeout)
///     } else {
///         Ok(200_u16)
///     }
/// }));
/// assert_eq!(result.unwrap(), 200);
/// assert_eq!(attempts.load(Ordering::SeqCst), 3);
/// ```
pub async fn execute_with_retry<T, F, Fut>(
    max_network_retries: u32,
    mut operation: F,
) -> Result<T, MoyasarError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, MoyasarError>>,
{
    let mut retries_left = max_network_retries;
    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) if retries_left > 0 && err.is_retryable() => {
                retries_left -= 1;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Alias for [`execute_with_retry`] that dispatches an async request factory with bounded retries.
pub async fn send_with_retry<T, F, Fut>(
    max_network_retries: u32,
    operation: F,
) -> Result<T, MoyasarError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, MoyasarError>>,
{
    execute_with_retry(max_network_retries, operation).await
}

/// Decodes a `2xx` [`reqwest::Response`] body as JSON into `T`, or maps non-`2xx`
/// responses into a structured [`MoyasarError`] via [`MoyasarError::from_api_response`].
pub async fn decode_json_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, MoyasarError> {
    let status = response.status();
    if status.is_success() {
        return response.json::<T>().await.map_err(MoyasarError::from);
    }
    let body = response.text().await.map_err(MoyasarError::from)?;
    Err(MoyasarError::from_api_response(status.as_u16(), &body))
}

/// Dispatches an idempotent `GET` request with query parameters through [`execute_with_retry`]
/// and decodes the JSON response via [`decode_json_response`].
pub async fn send_get_json<T, Q>(
    http: &reqwest::Client,
    url: &str,
    max_retries: u32,
    query: &Q,
) -> Result<T, MoyasarError>
where
    T: DeserializeOwned,
    Q: Serialize + Sync + ?Sized,
{
    execute_with_retry(max_retries, || async {
        let response = http
            .get(url)
            .query(query)
            .send()
            .await
            .map_err(MoyasarError::from)?;
        decode_json_response(response).await
    })
    .await
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
    use crate::kernel::error::ApiErrorType;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn encodes_basic_auth_with_trailing_colon() {
        let key = SecretKey::new("sk_test_YOUR_SECRET_KEY").expect("valid key");
        let header = HeaderValue::try_from(&key).expect("encodes header");
        let expected_b64 = STANDARD.encode("sk_test_YOUR_SECRET_KEY:");
        assert_eq!(header.to_str().unwrap(), format!("Basic {expected_b64}"));
        assert!(header.is_sensitive());
    }

    #[test]
    fn sets_pinned_api_version_header() {
        let key = PublishableKey::new("pk_test_abc").expect("valid key");
        let headers = build_default_headers(&key).expect("headers built");
        assert_eq!(headers.get(MOYASAR_VERSION_HEADER).unwrap(), API_VERSION);
    }

    #[tokio::test]
    async fn retries_transient_errors_and_stops_on_4xx() {
        let transient_attempts = AtomicU32::new(0);
        let ok = execute_with_retry(2, || async {
            let cur = transient_attempts.fetch_add(1, Ordering::SeqCst);
            if cur < 2 {
                Err(MoyasarError::ServerError {
                    status: 503,
                    message: "unavailable".into(),
                })
            } else {
                Ok("recovered")
            }
        })
        .await
        .expect("recovers on retry 2");
        assert_eq!(ok, "recovered");
        assert_eq!(transient_attempts.load(Ordering::SeqCst), 3);

        let client_err_attempts = AtomicU32::new(0);
        let err = send_with_retry(3, || async {
            client_err_attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "bad amount".into(),
                errors: Vec::new(),
                response_code: None,
            })
        })
        .await
        .expect_err("4xx must fail immediately");
        assert!(!err.is_retryable());
        assert_eq!(client_err_attempts.load(Ordering::SeqCst), 1);
    }
}
