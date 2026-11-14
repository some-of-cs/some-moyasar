//! SourcesService and ApplePayService client facades (US-7.1.0 Unit 3).

use crate::kernel::auth::{AuthKey, PublishableAuthKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::resources::sources::apple_pay::{
    ApplePayMerchantSession, InitiateApplePayParams, initiate_session_request,
};
use crate::resources::sources::issuer::{BinLookupParams, IssuerMetadata, get_issuer_request};

/// Borrowed service handle for Moyasar BIN/IIN issuer lookup (`POST /v1/source/issuer`).
///
/// Implements `Copy` so obtaining `client.sources()` incurs zero heap allocation.
/// Both [`SecretKey`](crate::kernel::auth::SecretKey) and
/// [`PublishableKey`](crate::kernel::auth::PublishableKey) clients may call
/// [`SourcesService::get_issuer`].
#[derive(Debug, Clone, Copy)]
pub struct SourcesService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> SourcesService<'a, K> {
    /// Constructs a new [`SourcesService`] borrowing the given client.
    pub const fn new(client: &'a MoyasarClient<K>) -> Self {
        Self { client }
    }

    /// Returns a reference to the underlying [`MoyasarClient`].
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }

    /// Looks up card issuer metadata from a 6- to 8-digit BIN/IIN via
    /// `POST /v1/source/issuer`.
    ///
    /// Available for both `SecretKey` and `PublishableKey` clients. Transient
    /// network, timeout, and 5xx responses are retried according to
    /// [`MoyasarClientConfig::max_network_retries`](crate::kernel::client::MoyasarClientConfig::max_network_retries).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] on 4xx API errors, [`MoyasarError::ServerError`]
    /// on exhausted 5xx responses, [`MoyasarError::Timeout`] on request timeout, or
    /// [`MoyasarError::Network`] on transport failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::sources::issuer::BinLookupParams;
    ///
    /// # async fn example(params: &BinLookupParams) -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let pk = PublishableKey::new("pk_test_YOUR_PUBLISHABLE_KEY")?;
    /// let client = MoyasarClient::new(pk)?;
    /// let issuer = client.sources().get_issuer(params).await?;
    /// let _ = issuer;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_issuer(
        &self,
        params: &BinLookupParams,
    ) -> Result<IssuerMetadata, MoyasarError> {
        get_issuer_request(self.client, params).await
    }
}

/// Borrowed service handle for Moyasar Apple Pay Web Merchant session initiation
/// (`POST /v1/applepay/initiate`).
///
/// Implements `Copy` so obtaining `client.apple_pay()` incurs zero heap allocation.
/// Session initiation via [`ApplePayService::initiate_session`] is restricted at
/// compile time to [`PublishableAuthKey`] clients.
#[derive(Debug, Clone, Copy)]
pub struct ApplePayService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> ApplePayService<'a, K> {
    /// Constructs a new [`ApplePayService`] borrowing the given client.
    pub const fn new(client: &'a MoyasarClient<K>) -> Self {
        Self { client }
    }

    /// Returns a reference to the underlying [`MoyasarClient`].
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<'a, K: PublishableAuthKey> ApplePayService<'a, K> {
    /// Initiates a signed Apple Pay Web Merchant session via
    /// `POST /v1/applepay/initiate`.
    ///
    /// Restricted to [`PublishableAuthKey`] clients (`PublishableKey`). Attempting
    /// to call this method on a `MoyasarClient<SecretKey>` fails at compile time.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidBaseUrl`] if `params.validation_url` is not a
    /// valid `https://` URL, [`MoyasarError::Api`] on 4xx API errors,
    /// [`MoyasarError::ServerError`] on exhausted 5xx responses,
    /// [`MoyasarError::Timeout`] on request timeout, or [`MoyasarError::Network`]
    /// on transport failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::sources::apple_pay::InitiateApplePayParams;
    ///
    /// # async fn example(params: &InitiateApplePayParams) -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let pk = PublishableKey::new("pk_test_YOUR_PUBLISHABLE_KEY")?;
    /// let client = MoyasarClient::new(pk)?;
    /// let session = client.apple_pay().initiate_session(params).await?;
    /// let _ = session;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Attempting to initiate an Apple Pay session with a [`SecretKey`](crate::kernel::auth::SecretKey)
    /// is rejected at compile time:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::sources::apple_pay::InitiateApplePayParams;
    ///
    /// # async fn reject_secret_key(params: &InitiateApplePayParams) {
    /// let sk = SecretKey::new("sk_test_YOUR_SECRET_KEY").unwrap();
    /// let client = MoyasarClient::new(sk).unwrap();
    /// let _ = client.apple_pay().initiate_session(params).await;
    /// # }
    /// ```
    pub async fn initiate_session(
        &self,
        params: &InitiateApplePayParams,
    ) -> Result<ApplePayMerchantSession, MoyasarError> {
        initiate_session_request(self.client, params).await
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a zero-clone [`SourcesService`] handle for BIN/IIN issuer lookup.
    pub const fn sources(&self) -> SourcesService<'_, K> {
        SourcesService::new(self)
    }

    /// Returns a zero-clone [`ApplePayService`] handle for Apple Pay merchant session initiation.
    pub const fn apple_pay(&self) -> ApplePayService<'_, K> {
        ApplePayService::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::{PublishableKey, SecretKey};

    #[test]
    fn tc_7_1_0_01_sources_service_accepts_secret_and_publishable_keys() {
        let sk = SecretKey::new("sk_test_unit3_sources").expect("valid secret key");
        let sk_client = MoyasarClient::new(sk).expect("secret client builds");
        let sk_sources: SourcesService<'_, SecretKey> = sk_client.sources();
        assert!(std::ptr::eq(sk_sources.client(), &sk_client));

        let pk = PublishableKey::new("pk_test_unit3_sources").expect("valid publishable key");
        let pk_client = MoyasarClient::new(pk).expect("publishable client builds");
        let pk_sources: SourcesService<'_, PublishableKey> = pk_client.sources();
        assert!(std::ptr::eq(pk_sources.client(), &pk_client));

        let copied = pk_sources;
        assert!(std::ptr::eq(copied.client(), &pk_client));
    }

    #[test]
    fn tc_7_1_0_03_apple_pay_service_binds_publishable_client_with_zero_clone() {
        let pk = PublishableKey::new("pk_test_unit3_apple_pay").expect("valid publishable key");
        let pk_client = MoyasarClient::new(pk).expect("publishable client builds");
        let apple_pay: ApplePayService<'_, PublishableKey> = pk_client.apple_pay();
        let copied = apple_pay;
        assert!(std::ptr::eq(copied.client(), &pk_client));
    }

    #[test]
    fn tc_7_1_0_04_secret_key_apple_pay_facade_lacks_initiate_session() {
        let sk = SecretKey::new("sk_test_unit3_apple_pay").expect("valid secret key");
        let sk_client = MoyasarClient::new(sk).expect("secret client builds");
        let sk_apple_pay: ApplePayService<'_, SecretKey> = sk_client.apple_pay();
        assert!(std::ptr::eq(sk_apple_pay.client(), &sk_client));
    }

    #[test]
    fn tc_7_1_0_law_service_debug_redacts_api_keys() {
        let sk = SecretKey::new("sk_test_sensitive_secret_999").expect("valid secret key");
        let sk_client = MoyasarClient::new(sk).expect("client builds");
        let sources_dbg = format!("{:?}", sk_client.sources());
        assert!(!sources_dbg.contains("sk_test_sensitive_secret_999"));
        assert!(sources_dbg.contains("[REDACTED]"));

        let pk = PublishableKey::new("pk_test_sensitive_pub_999").expect("valid publishable key");
        let pk_client = MoyasarClient::new(pk).expect("client builds");
        let apple_pay_dbg = format!("{:?}", pk_client.apple_pay());
        assert!(!apple_pay_dbg.contains("pk_test_sensitive_pub_999"));
        assert!(apple_pay_dbg.contains("[REDACTED]"));
    }
}
