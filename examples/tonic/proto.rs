//! Zero-`protoc` pure Rust Prost DTOs and Tonic service/client definitions for `moyasar.v1.MoyasarPaymentGateway`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// Request to create a hosted payment intent in minor currency units.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateIntentRpcRequest {
    /// Payment amount in minor currency units (e.g., halalas for SAR).
    #[prost(int64, tag = "1")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code (e.g., `"SAR"`).
    #[prost(string, tag = "2")]
    pub currency: String,
    /// Optional human-readable description for the payment intent.
    #[prost(string, optional, tag = "3")]
    pub description: Option<String>,
    /// HTTPS callback URL invoked after customer checkout completion.
    #[prost(string, tag = "4")]
    pub callback_url: String,
}

/// Response returned after creating a hosted payment intent.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateIntentRpcResponse {
    /// Unique Moyasar payment intent identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Optional client secret for frontend SDK confirmation.
    #[prost(string, optional, tag = "2")]
    pub client_secret: Option<String>,
    /// Payment amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "4")]
    pub currency: String,
    /// RFC 3339 expiration timestamp of the payment intent.
    #[prost(string, tag = "5")]
    pub expires_at: String,
}

/// Request to charge a previously vaulted payment token.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChargeTokenRpcRequest {
    /// Charge amount in minor currency units.
    #[prost(int64, tag = "1")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "2")]
    pub currency: String,
    /// Vaulted token identifier (must start with `token_`).
    #[prost(string, tag = "3")]
    pub token_id: String,
    /// Caller-supplied idempotency key for safe retries.
    #[prost(string, tag = "4")]
    pub idempotency_key: String,
}

/// Response returned after charging a saved payment token.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChargeTokenRpcResponse {
    /// Resulting Moyasar payment transaction identifier.
    #[prost(string, tag = "1")]
    pub transaction_id: String,
    /// Current payment status (e.g., `"paid"`, `"initiated"`, `"failed"`).
    #[prost(string, tag = "2")]
    pub status: String,
    /// Charged amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "4")]
    pub currency: String,
}

/// Request to refund a settled payment or void an authorized payment.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RefundOrVoidRpcRequest {
    /// Target Moyasar payment transaction identifier.
    #[prost(string, tag = "1")]
    pub transaction_id: String,
    /// Optional partial refund amount in minor currency units.
    #[prost(int64, optional, tag = "2")]
    pub amount_minor: Option<i64>,
}

/// Response returned after refunding or voiding a payment transaction.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RefundOrVoidRpcResponse {
    /// Target Moyasar payment transaction identifier.
    #[prost(string, tag = "1")]
    pub transaction_id: String,
    /// Reversal action performed (`"refunded"` or `"voided"`).
    #[prost(string, tag = "2")]
    pub action: String,
    /// Reversed amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_reversed_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "4")]
    pub currency: String,
}

/// Request to verify an incoming Moyasar webhook delivery.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct VerifyWebhookRpcRequest {
    /// Hex-encoded HMAC-SHA256 `x-moyasar-signature` header value.
    #[prost(string, tag = "1")]
    pub signature: String,
    /// Shared webhook signing secret token.
    #[prost(string, tag = "2")]
    pub secret_token: String,
    /// Raw unmodified HTTP request payload bytes.
    #[prost(bytes = "vec", tag = "3")]
    pub raw_body: Vec<u8>,
}

/// Response returned after verifying a Moyasar webhook delivery.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct VerifyWebhookRpcResponse {
    /// Unique webhook event identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Moyasar webhook event type (e.g., `"payment_paid"`).
    #[prost(string, tag = "2")]
    pub event_type: String,
    /// RFC 3339 event creation timestamp.
    #[prost(string, tag = "3")]
    pub created_at: String,
    /// Whether the event originated from the live environment.
    #[prost(bool, tag = "4")]
    pub live: bool,
    /// Serialized JSON representation of the verified event payload.
    #[prost(string, tag = "5")]
    pub payload_json: String,
}

/// gRPC service trait for `moyasar.v1.MoyasarPaymentGateway`.
#[tonic::async_trait]
pub trait MoyasarPaymentGateway: Send + Sync + 'static {
    /// Creates a hosted payment intent.
    async fn create_intent(
        &self,
        request: tonic::Request<CreateIntentRpcRequest>,
    ) -> Result<tonic::Response<CreateIntentRpcResponse>, tonic::Status>;

    /// Charges a previously vaulted payment token.
    async fn charge_saved_token(
        &self,
        request: tonic::Request<ChargeTokenRpcRequest>,
    ) -> Result<tonic::Response<ChargeTokenRpcResponse>, tonic::Status>;

    /// Refunds a captured payment or voids an uncaptured authorization.
    async fn refund_or_void(
        &self,
        request: tonic::Request<RefundOrVoidRpcRequest>,
    ) -> Result<tonic::Response<RefundOrVoidRpcResponse>, tonic::Status>;

    /// Verifies and parses an incoming Moyasar webhook payload.
    async fn verify_webhook(
        &self,
        request: tonic::Request<VerifyWebhookRpcRequest>,
    ) -> Result<tonic::Response<VerifyWebhookRpcResponse>, tonic::Status>;
}

/// Tonic gRPC server wrapper for [`MoyasarPaymentGateway`].
#[derive(Debug)]
pub struct MoyasarPaymentGatewayServer<T> {
    inner: Arc<T>,
}

impl<T> MoyasarPaymentGatewayServer<T> {
    /// Creates a new [`MoyasarPaymentGatewayServer`] wrapping `inner`.
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

impl<T> Clone for MoyasarPaymentGatewayServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: MoyasarPaymentGateway> tonic::server::NamedService for MoyasarPaymentGatewayServer<T> {
    const NAME: &'static str = "moyasar.v1.MoyasarPaymentGateway";
}

type BoxFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'static>>;

fn dispatch_unary<S, Req, Res>(
    svc: S,
    req: tonic::codegen::http::Request<tonic::body::BoxBody>,
) -> BoxFuture<tonic::codegen::http::Response<tonic::body::BoxBody>, std::convert::Infallible>
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

fn unimplemented_response() -> tonic::codegen::http::Response<tonic::body::BoxBody> {
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

struct CreateIntentSvc<T>(Arc<T>);

impl<T: MoyasarPaymentGateway> tonic::server::UnaryService<CreateIntentRpcRequest>
    for CreateIntentSvc<T>
{
    type Response = CreateIntentRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<CreateIntentRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.create_intent(request).await })
    }
}

struct ChargeSavedTokenSvc<T>(Arc<T>);

impl<T: MoyasarPaymentGateway> tonic::server::UnaryService<ChargeTokenRpcRequest>
    for ChargeSavedTokenSvc<T>
{
    type Response = ChargeTokenRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<ChargeTokenRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.charge_saved_token(request).await })
    }
}

struct RefundOrVoidSvc<T>(Arc<T>);

impl<T: MoyasarPaymentGateway> tonic::server::UnaryService<RefundOrVoidRpcRequest>
    for RefundOrVoidSvc<T>
{
    type Response = RefundOrVoidRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<RefundOrVoidRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.refund_or_void(request).await })
    }
}

struct VerifyWebhookSvc<T>(Arc<T>);

impl<T: MoyasarPaymentGateway> tonic::server::UnaryService<VerifyWebhookRpcRequest>
    for VerifyWebhookSvc<T>
{
    type Response = VerifyWebhookRpcResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, request: tonic::Request<VerifyWebhookRpcRequest>) -> Self::Future {
        let inner = Arc::clone(&self.0);
        Box::pin(async move { inner.verify_webhook(request).await })
    }
}

impl<T> tonic::codegen::Service<tonic::codegen::http::Request<tonic::body::BoxBody>>
    for MoyasarPaymentGatewayServer<T>
where
    T: MoyasarPaymentGateway,
{
    type Response = tonic::codegen::http::Response<tonic::body::BoxBody>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: tonic::codegen::http::Request<tonic::body::BoxBody>) -> Self::Future {
        match req.uri().path() {
            "/moyasar.v1.MoyasarPaymentGateway/CreateIntent" => {
                dispatch_unary(CreateIntentSvc(Arc::clone(&self.inner)), req)
            }
            "/moyasar.v1.MoyasarPaymentGateway/ChargeSavedToken" => {
                dispatch_unary(ChargeSavedTokenSvc(Arc::clone(&self.inner)), req)
            }
            "/moyasar.v1.MoyasarPaymentGateway/RefundOrVoid" => {
                dispatch_unary(RefundOrVoidSvc(Arc::clone(&self.inner)), req)
            }
            "/moyasar.v1.MoyasarPaymentGateway/VerifyWebhook" => {
                dispatch_unary(VerifyWebhookSvc(Arc::clone(&self.inner)), req)
            }
            _ => Box::pin(async move { Ok(unimplemented_response()) }),
        }
    }
}

/// Tonic gRPC client for `moyasar.v1.MoyasarPaymentGateway`.
#[derive(Debug, Clone)]
pub struct MoyasarPaymentGatewayClient<T> {
    inner: tonic::client::Grpc<T>,
}

impl MoyasarPaymentGatewayClient<tonic::transport::Channel> {
    /// Creates a new [`MoyasarPaymentGatewayClient`] from a Tonic transport channel.
    pub fn new(channel: tonic::transport::Channel) -> Self {
        Self {
            inner: tonic::client::Grpc::new(channel),
        }
    }
}

impl<T> MoyasarPaymentGatewayClient<T>
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

    /// Calls `CreateIntent` on the remote gateway service.
    pub async fn create_intent(
        &mut self,
        request: impl tonic::IntoRequest<CreateIntentRpcRequest>,
    ) -> Result<tonic::Response<CreateIntentRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarPaymentGateway/CreateIntent")
            .await
    }

    /// Calls `ChargeSavedToken` on the remote gateway service.
    pub async fn charge_saved_token(
        &mut self,
        request: impl tonic::IntoRequest<ChargeTokenRpcRequest>,
    ) -> Result<tonic::Response<ChargeTokenRpcResponse>, tonic::Status> {
        self.unary_call(
            request,
            "/moyasar.v1.MoyasarPaymentGateway/ChargeSavedToken",
        )
        .await
    }

    /// Calls `RefundOrVoid` on the remote gateway service.
    pub async fn refund_or_void(
        &mut self,
        request: impl tonic::IntoRequest<RefundOrVoidRpcRequest>,
    ) -> Result<tonic::Response<RefundOrVoidRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarPaymentGateway/RefundOrVoid")
            .await
    }

    /// Calls `VerifyWebhook` on the remote gateway service.
    pub async fn verify_webhook(
        &mut self,
        request: impl tonic::IntoRequest<VerifyWebhookRpcRequest>,
    ) -> Result<tonic::Response<VerifyWebhookRpcResponse>, tonic::Status> {
        self.unary_call(request, "/moyasar.v1.MoyasarPaymentGateway/VerifyWebhook")
            .await
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use tonic::server::NamedService;

    use super::*;

    #[test]
    fn prost_dtos_roundtrip_minor_units() {
        let req = CreateIntentRpcRequest {
            amount_minor: 15_000,
            currency: "SAR".into(),
            description: Some("Order #42".into()),
            callback_url: "https://example.com/callback".into(),
        };
        let encoded = req.encode_to_vec();
        let decoded = CreateIntentRpcRequest::decode(encoded.as_slice()).expect("valid prost");
        assert_eq!(decoded, req);
    }

    struct DummyGateway;

    #[tonic::async_trait]
    impl MoyasarPaymentGateway for DummyGateway {
        async fn create_intent(
            &self,
            _request: tonic::Request<CreateIntentRpcRequest>,
        ) -> Result<tonic::Response<CreateIntentRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn charge_saved_token(
            &self,
            _request: tonic::Request<ChargeTokenRpcRequest>,
        ) -> Result<tonic::Response<ChargeTokenRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn refund_or_void(
            &self,
            _request: tonic::Request<RefundOrVoidRpcRequest>,
        ) -> Result<tonic::Response<RefundOrVoidRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }

        async fn verify_webhook(
            &self,
            _request: tonic::Request<VerifyWebhookRpcRequest>,
        ) -> Result<tonic::Response<VerifyWebhookRpcResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("stub"))
        }
    }

    #[test]
    fn server_named_service_matches_proto_package() {
        assert_eq!(
            MoyasarPaymentGatewayServer::<DummyGateway>::NAME,
            "moyasar.v1.MoyasarPaymentGateway"
        );
    }
}
