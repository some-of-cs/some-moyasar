//! Ergonomic [`MoyasarGatewayAdapter`] wrapper around [`MoyasarClient<SecretKey>`].

use std::future::Future;
use std::pin::Pin;

use crate::kernel::auth::SecretKey;
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::unified::{
    PayoutCapability, UnifiedChargeResponse, UnifiedIntentRequest, UnifiedIntentResponse,
    UnifiedPaymentGateway, UnifiedReversalResponse, UnifiedTokenChargeRequest, UnifiedWebhookEvent,
};

/// Ergonomic gateway adapter wrapping a [`MoyasarClient<SecretKey>`] and implementing
/// [`UnifiedPaymentGateway`] and [`PayoutCapability`].
#[derive(Debug, Clone)]
pub struct MoyasarGatewayAdapter {
    client: MoyasarClient<SecretKey>,
}

impl MoyasarGatewayAdapter {
    /// Wraps an existing [`MoyasarClient<SecretKey>`] in a [`MoyasarGatewayAdapter`].
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::unified::UnifiedPaymentGateway;
    /// use some_moyasar::unified::router::MoyasarGatewayAdapter;
    ///
    /// let key = SecretKey::new("sk_test_adapter_demo").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let adapter = MoyasarGatewayAdapter::from_client(client);
    /// fn assert_gateway<G: UnifiedPaymentGateway>(_g: &G) {}
    /// assert_gateway(&adapter);
    /// ```
    #[must_use]
    pub const fn from_client(client: MoyasarClient<SecretKey>) -> Self {
        Self { client }
    }

    /// Returns a reference to the underlying [`MoyasarClient<SecretKey>`].
    #[must_use]
    pub const fn client(&self) -> &MoyasarClient<SecretKey> {
        &self.client
    }
}

impl PayoutCapability for MoyasarGatewayAdapter {}

impl UnifiedPaymentGateway for MoyasarGatewayAdapter {
    type Error = MoyasarError;

    fn create_intent<'a>(
        &'a self,
        request: UnifiedIntentRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedIntentResponse, Self::Error>> + Send + 'a>> {
        self.client.create_intent(request)
    }

    fn charge_saved_token<'a>(
        &'a self,
        request: UnifiedTokenChargeRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedChargeResponse, Self::Error>> + Send + 'a>> {
        self.client.charge_saved_token(request)
    }

    fn refund_or_void<'a>(
        &'a self,
        transaction_id: &'a str,
        amount_minor: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedReversalResponse, Self::Error>> + Send + 'a>>
    {
        self.client.refund_or_void(transaction_id, amount_minor)
    }

    fn verify_webhook(
        &self,
        headers: &http::HeaderMap,
        raw_body: &[u8],
    ) -> Result<UnifiedWebhookEvent, Self::Error> {
        self.client.verify_webhook(headers, raw_body)
    }
}
