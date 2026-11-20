//! Ergonomic [`MoyasarGatewayAdapter`] wrapper around [`MoyasarClient<SecretKey>`].

use std::future::Future;
use std::pin::Pin;

use crate::kernel::auth::SecretKey;
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::unified::{
    PayoutCapability, SplitSettlementCapability, UnifiedBulkPayoutResponse, UnifiedChargeResponse,
    UnifiedIntentRequest, UnifiedIntentResponse, UnifiedPaymentGateway, UnifiedPayoutRequest,
    UnifiedPayoutResponse, UnifiedReversalResponse, UnifiedSplitPaymentRequest,
    UnifiedSplitPaymentResponse, UnifiedTokenChargeRequest, UnifiedWebhookEvent,
};

/// Ergonomic gateway adapter wrapping a [`MoyasarClient<SecretKey>`] and implementing
/// [`UnifiedPaymentGateway`], [`PayoutCapability`], and [`SplitSettlementCapability`].
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
    /// use some_moyasar::unified::{
    ///     PayoutCapability, SplitSettlementCapability, UnifiedPaymentGateway,
    /// };
    /// use some_moyasar::unified::router::MoyasarGatewayAdapter;
    ///
    /// let key = SecretKey::new("sk_test_adapter_demo").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let adapter = MoyasarGatewayAdapter::from_client(client);
    /// fn assert_caps<G: UnifiedPaymentGateway + PayoutCapability + SplitSettlementCapability>(
    ///     _g: &G,
    /// ) {
    /// }
    /// assert_caps(&adapter);
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

impl PayoutCapability for MoyasarGatewayAdapter {
    type Error = MoyasarError;

    fn disburse<'a>(
        &'a self,
        request: UnifiedPayoutRequest,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedPayoutResponse, Self::Error>> + Send + 'a>> {
        self.client.disburse(request)
    }

    fn disburse_bulk<'a>(
        &'a self,
        source_account_id: &'a str,
        requests: Vec<UnifiedPayoutRequest>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedBulkPayoutResponse, Self::Error>> + Send + 'a>> {
        self.client.disburse_bulk(source_account_id, requests)
    }

    fn payout_status<'a>(
        &'a self,
        payout_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedPayoutResponse, Self::Error>> + Send + 'a>> {
        self.client.payout_status(payout_id)
    }
}

impl SplitSettlementCapability for MoyasarGatewayAdapter {
    type Error = MoyasarError;

    fn create_split_payment<'a>(
        &'a self,
        request: UnifiedSplitPaymentRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<UnifiedSplitPaymentResponse, Self::Error>> + Send + 'a>,
    > {
        self.client.create_split_payment(request)
    }
}

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

    fn verify_webhook_payload(&self, raw_body: &[u8]) -> Result<UnifiedWebhookEvent, Self::Error> {
        self.client.verify_webhook_payload(raw_body)
    }
}
