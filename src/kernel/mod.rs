//! Core microkernel and transport layer for `some-moyasar`.

pub mod auth;
pub mod client;
#[cfg(feature = "env-keys")]
pub mod env;
pub mod error;
pub mod idempotency;
pub mod idempotency_v5;
pub mod metadata;
pub mod pagination;
pub mod paginator_stream;
pub mod response_code;
pub mod transport;
pub mod types;

pub use auth::{
    AuthKey, AuthMode, PublishableAuthKey, PublishableKey, SecretAuthKey, SecretKey,
    serialize_opt_secret, serialize_secret,
};
pub use client::{
    ClientInner, DEFAULT_BASE_URL, MoyasarClient, MoyasarClientBuilder, MoyasarClientConfig,
};
#[cfg(feature = "env-keys")]
pub use env::resolve_env_secret;
pub use error::{ApiErrorPayload, ApiErrorType, MoyasarError};
pub use idempotency::{GivenIdPolicy, inject_given_id};
pub use idempotency_v5::{MOYASAR_SUBSCRIPTION_NAMESPACE, derive_subscription_given_id};
pub use metadata::{METADATA_KEY_MAX_LEN, METADATA_MAX_KEYS, METADATA_VALUE_MAX_LEN, Metadata};
pub use pagination::{Meta, PaginatedList};
pub use paginator_stream::Paginator;
pub use response_code::{FieldError, ResponseCode, deserialize_field_errors};
pub use transport::{
    API_VERSION, MOYASAR_VERSION_HEADER, build_default_headers, decode_json_response,
    encode_basic_auth, execute_with_retry, send_get_json, send_with_retry,
};
pub use types::{
    Amount, Currency, RecipientType, is_valid_id_char, is_valid_path_segment, is_valid_prefixed_id,
    validate_path_segment,
};
