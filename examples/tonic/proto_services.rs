//! Zero-`protoc` Prost DTOs and Tonic service/client definitions for all crate resource services.

#![allow(clippy::result_large_err)]
#![allow(dead_code)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// Request to retrieve, cancel, or delete a resource by its Moyasar identifier.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RetrieveIdRpcRequest {
    /// Target resource identifier.
    #[prost(string, tag = "1")]
    pub id: String,
}

/// Response representing a Moyasar payment resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaymentJsonRpcResponse {
    /// Unique Moyasar payment identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current payment status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Payment amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "4")]
    pub currency: String,
}

/// Request to execute a smart refund or void on a payment.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SmartRefundRpcRequest {
    /// Target Moyasar payment identifier.
    #[prost(string, tag = "1")]
    pub payment_id: String,
    /// Optional partial refund amount in minor currency units.
    #[prost(int64, optional, tag = "2")]
    pub amount_minor: Option<i64>,
}

/// Response returned after executing a smart refund or void operation.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SmartRefundRpcResponse {
    /// Target Moyasar payment identifier.
    #[prost(string, tag = "1")]
    pub payment_id: String,
    /// Reversal action performed (`"refunded"` or `"voided"`).
    #[prost(string, tag = "2")]
    pub action: String,
    /// Reversed amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
}

/// Response representing a Moyasar payment intent resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IntentJsonRpcResponse {
    /// Unique Moyasar payment intent identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current payment intent status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Intent amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Whether the payment intent is currently usable for checkout.
    #[prost(bool, tag = "4")]
    pub usable: bool,
}

/// Response representing a Moyasar vaulted card token resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TokenRpcResponse {
    /// Unique Moyasar token identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current token status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Card network brand associated with the token.
    #[prost(string, tag = "3")]
    pub brand: String,
    /// Whether the token has been deleted.
    #[prost(bool, tag = "4")]
    pub deleted: bool,
}

/// Request to create a hosted Moyasar invoice in minor currency units.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateInvoiceRpcRequest {
    /// Invoice amount in minor currency units.
    #[prost(int64, tag = "1")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "2")]
    pub currency: String,
    /// Optional human-readable invoice description.
    #[prost(string, optional, tag = "3")]
    pub description: Option<String>,
}

/// Response representing a Moyasar invoice resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InvoiceRpcResponse {
    /// Unique Moyasar invoice identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current invoice status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Invoice amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "4")]
    pub currency: String,
}

/// Response representing a 3-D Secure card authentication challenge resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CardAuthRpcResponse {
    /// Unique 3DS card authentication identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current 3DS authentication status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Optional ACS challenge redirect URL.
    #[prost(string, optional, tag = "3")]
    pub transaction_url: Option<String>,
}

/// Request to list paginated resources or fetch a ledger summary.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListPageRpcRequest {
    /// Optional 1-based page number.
    #[prost(uint32, optional, tag = "1")]
    pub page: Option<u32>,
}

/// Response carrying a resource identifier and its serialized JSON payload.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct JsonPayloadRpcResponse {
    /// Primary resource or summary identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Serialized JSON representation of the resource payload.
    #[prost(string, tag = "2")]
    pub json: String,
}

/// Request to initiate a bank IBAN payout transfer in minor currency units.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreatePayoutRpcRequest {
    /// Source payout account identifier.
    #[prost(string, tag = "1")]
    pub source_id: String,
    /// Payout amount in minor currency units.
    #[prost(int64, tag = "2")]
    pub amount_minor: i64,
    /// Beneficiary IBAN destination.
    #[prost(string, tag = "3")]
    pub iban: String,
    /// Beneficiary full legal name.
    #[prost(string, tag = "4")]
    pub name: String,
    /// Beneficiary mobile phone number.
    #[prost(string, tag = "5")]
    pub mobile: String,
}

/// Request to look up card issuer metadata by BIN prefix.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BinLookupRpcRequest {
    /// First 6 to 8 digits of the payment card number.
    #[prost(string, tag = "1")]
    pub first_digits: String,
}

/// Request to initiate an Apple Pay merchant validation session.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ApplePaySessionRpcRequest {
    /// Apple Pay merchant validation URL.
    #[prost(string, tag = "1")]
    pub validation_url: String,
    /// Merchant display name shown on the Apple Pay sheet.
    #[prost(string, tag = "2")]
    pub display_name: String,
    /// Verified merchant web domain name.
    #[prost(string, tag = "3")]
    pub domain_name: String,
}

/// Request to register a webhook endpoint and verify delivery.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateWebhookEndpointRpcRequest {
    /// HTTPS destination URL for webhook deliveries.
    #[prost(string, tag = "1")]
    pub url: String,
    /// Shared secret used to sign webhook payloads.
    #[prost(string, tag = "2")]
    pub shared_secret: String,
}

/// Request to upsert a customer profile, charge a vaulted token, and run a subscription cycle.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CustomerChargeRpcRequest {
    /// Logical customer identifier.
    #[prost(string, tag = "1")]
    pub customer_id: String,
    /// Vaulted card token identifier (`token_...`).
    #[prost(string, tag = "2")]
    pub token_id: String,
    /// Charge amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "4")]
    pub currency: String,
    /// Caller-supplied idempotency key (`given_id`).
    #[prost(string, tag = "5")]
    pub given_id: String,
}

/// Request to execute a recurring subscription billing cycle.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscriptionCycleRpcRequest {
    /// Logical subscription identifier.
    #[prost(string, tag = "1")]
    pub subscription_id: String,
    /// Zero-based or one-based billing cycle index.
    #[prost(uint64, tag = "2")]
    pub cycle_index: u64,
    /// Billing cycle amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Vaulted card token identifier (`token_...`).
    #[prost(string, tag = "4")]
    pub token_id: String,
}

/// gRPC service trait for `moyasar.v1.MoyasarCoreResources`.
#[tonic::async_trait]
pub trait MoyasarCoreResources: Send + Sync + 'static {
    /// Retrieves a payment by ID.
    async fn retrieve_payment(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<PaymentJsonRpcResponse>, tonic::Status>;

    /// Refunds a captured payment or voids an authorized payment.
    async fn smart_refund(
        &self,
        request: tonic::Request<SmartRefundRpcRequest>,
    ) -> Result<tonic::Response<SmartRefundRpcResponse>, tonic::Status>;

    /// Retrieves a payment intent by ID.
    async fn retrieve_intent(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<IntentJsonRpcResponse>, tonic::Status>;

    /// Cancels an active payment intent by ID.
    async fn cancel_intent(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<IntentJsonRpcResponse>, tonic::Status>;

    /// Retrieves a vaulted card token by ID.
    async fn retrieve_token(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<TokenRpcResponse>, tonic::Status>;

    /// Deletes a vaulted card token by ID.
    async fn delete_token(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<TokenRpcResponse>, tonic::Status>;

    /// Creates a hosted invoice in minor currency units.
    async fn create_invoice(
        &self,
        request: tonic::Request<CreateInvoiceRpcRequest>,
    ) -> Result<tonic::Response<InvoiceRpcResponse>, tonic::Status>;

    /// Cancels an active invoice by ID.
    async fn cancel_invoice(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<InvoiceRpcResponse>, tonic::Status>;

    /// Retrieves a 3DS card authentication session by ID.
    async fn retrieve_card_auth(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<CardAuthRpcResponse>, tonic::Status>;
}

/// gRPC service trait for `moyasar.v1.MoyasarPlatformResources`.
#[tonic::async_trait]
pub trait MoyasarPlatformResources: Send + Sync + 'static {
    /// Lists registered payout accounts.
    async fn list_payout_accounts(
        &self,
        request: tonic::Request<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Creates a bank IBAN payout transfer.
    async fn create_payout(
        &self,
        request: tonic::Request<CreatePayoutRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Fetches aggregated settlement balances.
    async fn fetch_settlement_balance(
        &self,
        request: tonic::Request<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Retrieves a settlement and its lines by ID.
    async fn retrieve_settlement(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Lists platform transfers.
    async fn list_transfers(
        &self,
        request: tonic::Request<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Lists internal ledger transactions.
    async fn list_internal_transactions(
        &self,
        request: tonic::Request<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Looks up card issuer metadata by BIN prefix.
    async fn lookup_issuer(
        &self,
        request: tonic::Request<BinLookupRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Initiates an Apple Pay merchant validation session.
    async fn initiate_apple_pay_session(
        &self,
        request: tonic::Request<ApplePaySessionRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Creates and manages a webhook endpoint subscription.
    async fn manage_webhook_endpoint(
        &self,
        request: tonic::Request<CreateWebhookEndpointRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Charges a customer profile and runs a recurring subscription cycle.
    async fn charge_customer_and_subscription(
        &self,
        request: tonic::Request<CustomerChargeRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;
}

type BoxFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'static>>;
type HttpBody = tonic::body::BoxBody;
type HttpRequest = tonic::codegen::http::Request<HttpBody>;
type HttpResponse = tonic::codegen::http::Response<HttpBody>;

fn dispatch_unary<S, Req, Res>(
    svc: S,
    req: HttpRequest,
) -> BoxFuture<HttpResponse, std::convert::Infallible>
where
    S: tonic::server::UnaryService<Req, Response = Res> + Send + 'static,
    S::Future: Send + 'static,
    Req: prost::Message + Default + Send + 'static,
    Res: prost::Message + Send + 'static,
{
    Box::pin(async move {
        let mut grpc = tonic::server::Grpc::new(tonic::codec::ProstCodec::default());
        Ok(grpc.unary(svc, req).await)
    })
}

fn unimplemented_response() -> HttpResponse {
    let mut res = tonic::codegen::http::Response::new(tonic::body::empty_body());
    let headers = res.headers_mut();
    headers.insert(
        tonic::codegen::http::header::CONTENT_TYPE,
        tonic::codegen::http::HeaderValue::from_static("application/grpc"),
    );
    headers.insert(
        "grpc-status",
        tonic::codegen::http::HeaderValue::from_static("12"),
    );
    res
}

struct RetrievePaymentSvc<T>(Arc<T>);

impl<T: MoyasarCoreResources> tonic::server::UnaryService<RetrieveIdRpcRequest>
    for RetrievePaymentSvc<T>
{
    type Response = PaymentJsonRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<RetrieveIdRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.retrieve_payment(request).await })
    }
}

struct SmartRefundSvc<T>(Arc<T>);

impl<T: MoyasarCoreResources> tonic::server::UnaryService<SmartRefundRpcRequest>
    for SmartRefundSvc<T>
{
    type Response = SmartRefundRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<SmartRefundRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.smart_refund(request).await })
    }
}

struct RetrieveIntentSvc<T>(Arc<T>);

impl<T: MoyasarCoreResources> tonic::server::UnaryService<RetrieveIdRpcRequest>
    for RetrieveIntentSvc<T>
{
    type Response = IntentJsonRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<RetrieveIdRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.retrieve_intent(request).await })
    }
}

struct CancelIntentSvc<T>(Arc<T>);

impl<T: MoyasarCoreResources> tonic::server::UnaryService<RetrieveIdRpcRequest>
    for CancelIntentSvc<T>
{
    type Response = IntentJsonRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<RetrieveIdRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.cancel_intent(request).await })
    }
}

struct RetrieveTokenSvc<T>(Arc<T>);

impl<T: MoyasarCoreResources> tonic::server::UnaryService<RetrieveIdRpcRequest>
    for RetrieveTokenSvc<T>
{
    type Response = TokenRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<RetrieveIdRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.retrieve_token(request).await })
    }
}

struct DeleteTokenSvc<T>(Arc<T>);

impl<T: MoyasarCoreResources> tonic::server::UnaryService<RetrieveIdRpcRequest>
    for DeleteTokenSvc<T>
{
    type Response = TokenRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<RetrieveIdRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.delete_token(request).await })
    }
}

struct CreateInvoiceSvc<T>(Arc<T>);

impl<T: MoyasarCoreResources> tonic::server::UnaryService<CreateInvoiceRpcRequest>
    for CreateInvoiceSvc<T>
{
    type Response = InvoiceRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<CreateInvoiceRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.create_invoice(request).await })
    }
}

struct CancelInvoiceSvc<T>(Arc<T>);

impl<T: MoyasarCoreResources> tonic::server::UnaryService<RetrieveIdRpcRequest>
    for CancelInvoiceSvc<T>
{
    type Response = InvoiceRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<RetrieveIdRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.cancel_invoice(request).await })
    }
}

struct RetrieveCardAuthSvc<T>(Arc<T>);

impl<T: MoyasarCoreResources> tonic::server::UnaryService<RetrieveIdRpcRequest>
    for RetrieveCardAuthSvc<T>
{
    type Response = CardAuthRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<RetrieveIdRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.retrieve_card_auth(request).await })
    }
}

/// Tonic gRPC server wrapper for [`MoyasarCoreResources`].
#[derive(Debug)]
pub struct MoyasarCoreResourcesServer<T> {
    inner: Arc<T>,
}

impl<T> MoyasarCoreResourcesServer<T> {
    /// Creates a new [`MoyasarCoreResourcesServer`] wrapping `inner`.
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

impl<T> Clone for MoyasarCoreResourcesServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: MoyasarCoreResources> tonic::server::NamedService for MoyasarCoreResourcesServer<T> {
    const NAME: &'static str = "moyasar.v1.MoyasarCoreResources";
}

fn dispatch_payment_intent_rpc<T: MoyasarCoreResources>(
    inner: &Arc<T>,
    req: HttpRequest,
) -> BoxFuture<HttpResponse, std::convert::Infallible> {
    match req.uri().path() {
        "/moyasar.v1.MoyasarCoreResources/RetrievePayment" => {
            dispatch_unary(RetrievePaymentSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarCoreResources/SmartRefund" => {
            dispatch_unary(SmartRefundSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarCoreResources/RetrieveIntent" => {
            dispatch_unary(RetrieveIntentSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarCoreResources/CancelIntent" => {
            dispatch_unary(CancelIntentSvc(Arc::clone(inner)), req)
        }
        _ => dispatch_token_invoice_3ds_rpc(inner, req),
    }
}

fn dispatch_token_invoice_3ds_rpc<T: MoyasarCoreResources>(
    inner: &Arc<T>,
    req: HttpRequest,
) -> BoxFuture<HttpResponse, std::convert::Infallible> {
    match req.uri().path() {
        "/moyasar.v1.MoyasarCoreResources/RetrieveToken" => {
            dispatch_unary(RetrieveTokenSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarCoreResources/DeleteToken" => {
            dispatch_unary(DeleteTokenSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarCoreResources/CreateInvoice" => {
            dispatch_unary(CreateInvoiceSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarCoreResources/CancelInvoice" => {
            dispatch_unary(CancelInvoiceSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarCoreResources/RetrieveCardAuth" => {
            dispatch_unary(RetrieveCardAuthSvc(Arc::clone(inner)), req)
        }
        _ => Box::pin(async move { Ok(unimplemented_response()) }),
    }
}

impl<T> tonic::codegen::Service<HttpRequest> for MoyasarCoreResourcesServer<T>
where
    T: MoyasarCoreResources,
{
    type Response = HttpResponse;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: HttpRequest) -> Self::Future {
        dispatch_payment_intent_rpc(&self.inner, req)
    }
}

/// Tonic gRPC client for `moyasar.v1.MoyasarCoreResources`.
#[derive(Debug, Clone)]
pub struct MoyasarCoreResourcesClient<T> {
    inner: tonic::client::Grpc<T>,
}

impl MoyasarCoreResourcesClient<tonic::transport::Channel> {
    /// Creates a new [`MoyasarCoreResourcesClient`] from a Tonic transport channel.
    pub fn new(channel: tonic::transport::Channel) -> Self {
        Self {
            inner: tonic::client::Grpc::new(channel),
        }
    }
}

impl<T> MoyasarCoreResourcesClient<T>
where
    T: tonic::client::GrpcService<tonic::body::BoxBody>,
    T::Error: Into<tonic::codegen::StdError>,
    T::ResponseBody: tonic::codegen::Body<Data = tonic::codegen::Bytes> + Send + 'static,
    <T::ResponseBody as tonic::codegen::Body>::Error: Into<tonic::codegen::StdError> + Send,
{
    async fn unary_call<Req, Res>(
        &mut self,
        request: impl tonic::IntoRequest<Req>,
        path: &'static str,
    ) -> Result<tonic::Response<Res>, tonic::Status>
    where
        Req: prost::Message + 'static,
        Res: prost::Message + Default + 'static,
    {
        self.inner.ready().await.map_err(|err| {
            tonic::Status::unknown(format!("Service was not ready: {}", err.into()))
        })?;
        let codec = tonic::codec::ProstCodec::default();
        let path = tonic::codegen::http::uri::PathAndQuery::from_static(path);
        self.inner.unary(request.into_request(), path, codec).await
    }

    /// Calls `RetrievePayment` on the remote core resources service.
    pub async fn retrieve_payment(
        &mut self,
        request: impl tonic::IntoRequest<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<PaymentJsonRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarCoreResources/RetrievePayment")
            .await
    }

    /// Calls `SmartRefund` on the remote core resources service.
    pub async fn smart_refund(
        &mut self,
        request: impl tonic::IntoRequest<SmartRefundRpcRequest>,
    ) -> Result<tonic::Response<SmartRefundRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarCoreResources/SmartRefund")
            .await
    }

    /// Calls `RetrieveIntent` on the remote core resources service.
    pub async fn retrieve_intent(
        &mut self,
        request: impl tonic::IntoRequest<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<IntentJsonRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarCoreResources/RetrieveIntent")
            .await
    }

    /// Calls `CancelIntent` on the remote core resources service.
    pub async fn cancel_intent(
        &mut self,
        request: impl tonic::IntoRequest<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<IntentJsonRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarCoreResources/CancelIntent")
            .await
    }

    /// Calls `RetrieveToken` on the remote core resources service.
    pub async fn retrieve_token(
        &mut self,
        request: impl tonic::IntoRequest<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<TokenRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarCoreResources/RetrieveToken")
            .await
    }

    /// Calls `DeleteToken` on the remote core resources service.
    pub async fn delete_token(
        &mut self,
        request: impl tonic::IntoRequest<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<TokenRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarCoreResources/DeleteToken")
            .await
    }

    /// Calls `CreateInvoice` on the remote core resources service.
    pub async fn create_invoice(
        &mut self,
        request: impl tonic::IntoRequest<CreateInvoiceRpcRequest>,
    ) -> Result<tonic::Response<InvoiceRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarCoreResources/CreateInvoice")
            .await
    }

    /// Calls `CancelInvoice` on the remote core resources service.
    pub async fn cancel_invoice(
        &mut self,
        request: impl tonic::IntoRequest<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<InvoiceRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarCoreResources/CancelInvoice")
            .await
    }

    /// Calls `RetrieveCardAuth` on the remote core resources service.
    pub async fn retrieve_card_auth(
        &mut self,
        request: impl tonic::IntoRequest<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<CardAuthRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarCoreResources/RetrieveCardAuth")
            .await
    }
}

struct ListPayoutAccountsSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<ListPageRpcRequest>
    for ListPayoutAccountsSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<ListPageRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.list_payout_accounts(request).await })
    }
}

struct CreatePayoutSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<CreatePayoutRpcRequest>
    for CreatePayoutSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<CreatePayoutRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.create_payout(request).await })
    }
}

struct FetchSettlementBalanceSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<ListPageRpcRequest>
    for FetchSettlementBalanceSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<ListPageRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.fetch_settlement_balance(request).await })
    }
}

struct RetrieveSettlementSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<RetrieveIdRpcRequest>
    for RetrieveSettlementSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<RetrieveIdRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.retrieve_settlement(request).await })
    }
}

struct ListTransfersSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<ListPageRpcRequest>
    for ListTransfersSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<ListPageRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.list_transfers(request).await })
    }
}

struct ListInternalTransactionsSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<ListPageRpcRequest>
    for ListInternalTransactionsSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<ListPageRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.list_internal_transactions(request).await })
    }
}

struct LookupIssuerSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<BinLookupRpcRequest>
    for LookupIssuerSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<BinLookupRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.lookup_issuer(request).await })
    }
}

struct InitiateApplePaySessionSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<ApplePaySessionRpcRequest>
    for InitiateApplePaySessionSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<ApplePaySessionRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.initiate_apple_pay_session(request).await })
    }
}

struct ManageWebhookEndpointSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<CreateWebhookEndpointRpcRequest>
    for ManageWebhookEndpointSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<CreateWebhookEndpointRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.manage_webhook_endpoint(request).await })
    }
}

struct ChargeCustomerAndSubscriptionSvc<T>(Arc<T>);

impl<T: MoyasarPlatformResources> tonic::server::UnaryService<CustomerChargeRpcRequest>
    for ChargeCustomerAndSubscriptionSvc<T>
{
    type Response = JsonPayloadRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<CustomerChargeRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.charge_customer_and_subscription(request).await })
    }
}

/// Tonic gRPC server wrapper for [`MoyasarPlatformResources`].
#[derive(Debug)]
pub struct MoyasarPlatformResourcesServer<T> {
    inner: Arc<T>,
}

impl<T> MoyasarPlatformResourcesServer<T> {
    /// Creates a new [`MoyasarPlatformResourcesServer`] wrapping `inner`.
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

impl<T> Clone for MoyasarPlatformResourcesServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: MoyasarPlatformResources> tonic::server::NamedService
    for MoyasarPlatformResourcesServer<T>
{
    const NAME: &'static str = "moyasar.v1.MoyasarPlatformResources";
}

fn dispatch_ledger_payout_rpc<T: MoyasarPlatformResources>(
    inner: &Arc<T>,
    req: HttpRequest,
) -> BoxFuture<HttpResponse, std::convert::Infallible> {
    match req.uri().path() {
        "/moyasar.v1.MoyasarPlatformResources/ListPayoutAccounts" => {
            dispatch_unary(ListPayoutAccountsSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarPlatformResources/CreatePayout" => {
            dispatch_unary(CreatePayoutSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarPlatformResources/FetchSettlementBalance" => {
            dispatch_unary(FetchSettlementBalanceSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarPlatformResources/RetrieveSettlement" => {
            dispatch_unary(RetrieveSettlementSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarPlatformResources/ListTransfers" => {
            dispatch_unary(ListTransfersSvc(Arc::clone(inner)), req)
        }
        _ => dispatch_sources_stripe_rpc(inner, req),
    }
}

fn dispatch_sources_stripe_rpc<T: MoyasarPlatformResources>(
    inner: &Arc<T>,
    req: HttpRequest,
) -> BoxFuture<HttpResponse, std::convert::Infallible> {
    match req.uri().path() {
        "/moyasar.v1.MoyasarPlatformResources/ListInternalTransactions" => {
            dispatch_unary(ListInternalTransactionsSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarPlatformResources/LookupIssuer" => {
            dispatch_unary(LookupIssuerSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarPlatformResources/InitiateApplePaySession" => {
            dispatch_unary(InitiateApplePaySessionSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarPlatformResources/ManageWebhookEndpoint" => {
            dispatch_unary(ManageWebhookEndpointSvc(Arc::clone(inner)), req)
        }
        "/moyasar.v1.MoyasarPlatformResources/ChargeCustomerAndSubscription" => {
            dispatch_unary(ChargeCustomerAndSubscriptionSvc(Arc::clone(inner)), req)
        }
        _ => Box::pin(async move { Ok(unimplemented_response()) }),
    }
}

impl<T> tonic::codegen::Service<HttpRequest> for MoyasarPlatformResourcesServer<T>
where
    T: MoyasarPlatformResources,
{
    type Response = HttpResponse;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: HttpRequest) -> Self::Future {
        dispatch_ledger_payout_rpc(&self.inner, req)
    }
}

/// Tonic gRPC client for `moyasar.v1.MoyasarPlatformResources`.
#[derive(Debug, Clone)]
pub struct MoyasarPlatformResourcesClient<T> {
    inner: tonic::client::Grpc<T>,
}

impl MoyasarPlatformResourcesClient<tonic::transport::Channel> {
    /// Creates a new [`MoyasarPlatformResourcesClient`] from a Tonic transport channel.
    pub fn new(channel: tonic::transport::Channel) -> Self {
        Self {
            inner: tonic::client::Grpc::new(channel),
        }
    }
}

impl<T> MoyasarPlatformResourcesClient<T>
where
    T: tonic::client::GrpcService<tonic::body::BoxBody>,
    T::Error: Into<tonic::codegen::StdError>,
    T::ResponseBody: tonic::codegen::Body<Data = tonic::codegen::Bytes> + Send + 'static,
    <T::ResponseBody as tonic::codegen::Body>::Error: Into<tonic::codegen::StdError> + Send,
{
    async fn unary_call<Req, Res>(
        &mut self,
        request: impl tonic::IntoRequest<Req>,
        path: &'static str,
    ) -> Result<tonic::Response<Res>, tonic::Status>
    where
        Req: prost::Message + 'static,
        Res: prost::Message + Default + 'static,
    {
        self.inner.ready().await.map_err(|err| {
            tonic::Status::unknown(format!("Service was not ready: {}", err.into()))
        })?;
        let codec = tonic::codec::ProstCodec::default();
        let path = tonic::codegen::http::uri::PathAndQuery::from_static(path);
        self.inner.unary(request.into_request(), path, codec).await
    }

    /// Calls `ListPayoutAccounts` on the remote platform resources service.
    pub async fn list_payout_accounts(
        &mut self,
        request: impl tonic::IntoRequest<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(
            request,
            "/moyasar.v1.MoyasarPlatformResources/ListPayoutAccounts",
        )
        .await
    }

    /// Calls `CreatePayout` on the remote platform resources service.
    pub async fn create_payout(
        &mut self,
        request: impl tonic::IntoRequest<CreatePayoutRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarPlatformResources/CreatePayout")
            .await
    }

    /// Calls `FetchSettlementBalance` on the remote platform resources service.
    pub async fn fetch_settlement_balance(
        &mut self,
        request: impl tonic::IntoRequest<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(
            request,
            "/moyasar.v1.MoyasarPlatformResources/FetchSettlementBalance",
        )
        .await
    }

    /// Calls `RetrieveSettlement` on the remote platform resources service.
    pub async fn retrieve_settlement(
        &mut self,
        request: impl tonic::IntoRequest<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(
            request,
            "/moyasar.v1.MoyasarPlatformResources/RetrieveSettlement",
        )
        .await
    }

    /// Calls `ListTransfers` on the remote platform resources service.
    pub async fn list_transfers(
        &mut self,
        request: impl tonic::IntoRequest<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(
            request,
            "/moyasar.v1.MoyasarPlatformResources/ListTransfers",
        )
        .await
    }

    /// Calls `ListInternalTransactions` on the remote platform resources service.
    pub async fn list_internal_transactions(
        &mut self,
        request: impl tonic::IntoRequest<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(
            request,
            "/moyasar.v1.MoyasarPlatformResources/ListInternalTransactions",
        )
        .await
    }

    /// Calls `LookupIssuer` on the remote platform resources service.
    pub async fn lookup_issuer(
        &mut self,
        request: impl tonic::IntoRequest<BinLookupRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarPlatformResources/LookupIssuer")
            .await
    }

    /// Calls `InitiateApplePaySession` on the remote platform resources service.
    pub async fn initiate_apple_pay_session(
        &mut self,
        request: impl tonic::IntoRequest<ApplePaySessionRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(
            request,
            "/moyasar.v1.MoyasarPlatformResources/InitiateApplePaySession",
        )
        .await
    }

    /// Calls `ManageWebhookEndpoint` on the remote platform resources service.
    pub async fn manage_webhook_endpoint(
        &mut self,
        request: impl tonic::IntoRequest<CreateWebhookEndpointRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(
            request,
            "/moyasar.v1.MoyasarPlatformResources/ManageWebhookEndpoint",
        )
        .await
    }

    /// Calls `ChargeCustomerAndSubscription` on the remote platform resources service.
    pub async fn charge_customer_and_subscription(
        &mut self,
        request: impl tonic::IntoRequest<CustomerChargeRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
        self.unary_call(
            request,
            "/moyasar.v1.MoyasarPlatformResources/ChargeCustomerAndSubscription",
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use tonic::server::NamedService;

    use super::*;

    #[test]
    fn core_and_platform_dtos_roundtrip_minor_units() {
        let invoice_req = CreateInvoiceRpcRequest {
            amount_minor: 25_000,
            currency: "SAR".into(),
            description: Some("Subscription Invoice".into()),
        };
        let bytes = invoice_req.encode_to_vec();
        let decoded =
            CreateInvoiceRpcRequest::decode(bytes.as_slice()).expect("valid invoice prost");
        assert_eq!(decoded, invoice_req);

        let sub_req = SubscriptionCycleRpcRequest {
            subscription_id: "sub_01".into(),
            cycle_index: 1,
            amount_minor: 9_900,
            token_id: "token_abc".into(),
        };
        let sub_bytes = sub_req.encode_to_vec();
        let sub_decoded =
            SubscriptionCycleRpcRequest::decode(sub_bytes.as_slice()).expect("valid sub prost");
        assert_eq!(sub_decoded, sub_req);
    }

    struct DummyResources;

    #[tonic::async_trait]
    impl MoyasarCoreResources for DummyResources {
        async fn retrieve_payment(
            &self,
            _request: tonic::Request<RetrieveIdRpcRequest>,
        ) -> Result<tonic::Response<PaymentJsonRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn smart_refund(
            &self,
            _request: tonic::Request<SmartRefundRpcRequest>,
        ) -> Result<tonic::Response<SmartRefundRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn retrieve_intent(
            &self,
            _request: tonic::Request<RetrieveIdRpcRequest>,
        ) -> Result<tonic::Response<IntentJsonRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn cancel_intent(
            &self,
            _request: tonic::Request<RetrieveIdRpcRequest>,
        ) -> Result<tonic::Response<IntentJsonRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn retrieve_token(
            &self,
            _request: tonic::Request<RetrieveIdRpcRequest>,
        ) -> Result<tonic::Response<TokenRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn delete_token(
            &self,
            _request: tonic::Request<RetrieveIdRpcRequest>,
        ) -> Result<tonic::Response<TokenRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn create_invoice(
            &self,
            _request: tonic::Request<CreateInvoiceRpcRequest>,
        ) -> Result<tonic::Response<InvoiceRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn cancel_invoice(
            &self,
            _request: tonic::Request<RetrieveIdRpcRequest>,
        ) -> Result<tonic::Response<InvoiceRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn retrieve_card_auth(
            &self,
            _request: tonic::Request<RetrieveIdRpcRequest>,
        ) -> Result<tonic::Response<CardAuthRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }
    }

    #[tonic::async_trait]
    impl MoyasarPlatformResources for DummyResources {
        async fn list_payout_accounts(
            &self,
            _request: tonic::Request<ListPageRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn create_payout(
            &self,
            _request: tonic::Request<CreatePayoutRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn fetch_settlement_balance(
            &self,
            _request: tonic::Request<ListPageRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn retrieve_settlement(
            &self,
            _request: tonic::Request<RetrieveIdRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn list_transfers(
            &self,
            _request: tonic::Request<ListPageRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn list_internal_transactions(
            &self,
            _request: tonic::Request<ListPageRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn lookup_issuer(
            &self,
            _request: tonic::Request<BinLookupRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn initiate_apple_pay_session(
            &self,
            _request: tonic::Request<ApplePaySessionRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn manage_webhook_endpoint(
            &self,
            _request: tonic::Request<CreateWebhookEndpointRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn charge_customer_and_subscription(
            &self,
            _request: tonic::Request<CustomerChargeRpcRequest>,
        ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }
    }

    #[test]
    fn server_named_services_match_proto_packages() {
        assert_eq!(
            MoyasarCoreResourcesServer::<DummyResources>::NAME,
            "moyasar.v1.MoyasarCoreResources"
        );
        assert_eq!(
            MoyasarPlatformResourcesServer::<DummyResources>::NAME,
            "moyasar.v1.MoyasarPlatformResources"
        );
    }
}
