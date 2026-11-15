//! `MoyasarGrpcService` implementation wrapping [`UnifiedPaymentGateway`].

use http::header::{HeaderMap, HeaderName, HeaderValue};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use some_moyasar::prelude::{
    MoyasarClient, SecretKey, UnifiedIntentRequest, UnifiedPaymentGateway,
    UnifiedTokenChargeRequest, WEBHOOK_SECRET_TOKEN_HEADER, WEBHOOK_SIGNATURE_HEADER,
};

use crate::proto::{
    ChargeSavedTokenRequest, ChargeSavedTokenResponse, CreateIntentRequest, CreateIntentResponse,
    MoyasarPaymentGateway, RefundOrVoidRequest, RefundOrVoidResponse, VerifyWebhookRequest,
    VerifyWebhookResponse,
};
use crate::status_mapper::to_tonic_status;

/// Tonic gRPC service adapter over [`MoyasarClient<SecretKey>`] and [`UnifiedPaymentGateway`].
#[derive(Debug, Clone)]
pub struct MoyasarGrpcService {
    /// Authenticated Moyasar SDK client used to execute unified gateway operations.
    client: MoyasarClient<SecretKey>,
}

impl MoyasarGrpcService {
    /// Creates a new [`MoyasarGrpcService`] wrapping the provided [`MoyasarClient<SecretKey>`].
    #[must_use]
    pub fn new(client: MoyasarClient<SecretKey>) -> Self {
        Self { client }
    }
}

/// Validates that a signed proto minor-unit amount is non-negative and converts it to `u64`.
fn parse_non_negative_amount(amount_minor: i64) -> Result<u64, Status> {
    u64::try_from(amount_minor)
        .map_err(|_| Status::invalid_argument("amount_minor must be non-negative"))
}

/// Inserts a non-empty header value into `headers`, returning `INVALID_ARGUMENT` if malformed.
fn insert_optional_header(
    headers: &mut HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), Status> {
    if value.is_empty() {
        return Ok(());
    }
    let header_value = HeaderValue::from_str(value)
        .map_err(|_| Status::invalid_argument(format!("invalid {name} header value")))?;
    headers.insert(HeaderName::from_static(name), header_value);
    Ok(())
}

/// Builds an [`http::HeaderMap`] containing Moyasar webhook signature and secret token headers.
fn build_webhook_headers(signature: &str, secret_token: &str) -> Result<HeaderMap, Status> {
    let mut headers = HeaderMap::new();
    insert_optional_header(&mut headers, WEBHOOK_SIGNATURE_HEADER, signature)?;
    insert_optional_header(&mut headers, WEBHOOK_SECRET_TOKEN_HEADER, secret_token)?;
    Ok(headers)
}

#[tonic::async_trait]
impl MoyasarPaymentGateway for MoyasarGrpcService {
    /// Creates a hosted/client-secret payment intent via [`UnifiedPaymentGateway::create_intent`].
    async fn create_intent(
        &self,
        request: Request<CreateIntentRequest>,
    ) -> Result<Response<CreateIntentResponse>, Status> {
        let req = request.into_inner();
        let amount_minor = parse_non_negative_amount(req.amount_minor)?;
        let mut unified =
            UnifiedIntentRequest::new(amount_minor, &req.currency, &req.callback_url);
        if let Some(desc) = req.description.as_deref() {
            unified = unified.description(desc);
        }

        let res = self
            .client
            .create_intent(unified)
            .await
            .map_err(to_tonic_status)?;

        Ok(Response::new(CreateIntentResponse {
            id: res.id,
            client_secret: res.client_secret,
            amount_minor: res.amount_minor,
            currency: res.currency.as_str().to_owned(),
            expires_at: res.expires_at.to_rfc3339(),
        }))
    }

    /// Charges a saved card token off-session via [`UnifiedPaymentGateway::charge_saved_token`].
    async fn charge_saved_token(
        &self,
        request: Request<ChargeSavedTokenRequest>,
    ) -> Result<Response<ChargeSavedTokenResponse>, Status> {
        let req = request.into_inner();
        let amount_minor = parse_non_negative_amount(req.amount_minor)?;
        let idempotency_key = Uuid::parse_str(&req.idempotency_key)
            .map_err(|_| Status::invalid_argument("idempotency_key must be a valid UUID"))?;
        let unified = UnifiedTokenChargeRequest::new(
            amount_minor,
            &req.currency,
            &req.token_id,
            idempotency_key,
        );

        let res = self
            .client
            .charge_saved_token(unified)
            .await
            .map_err(to_tonic_status)?;

        Ok(Response::new(ChargeSavedTokenResponse {
            transaction_id: res.transaction_id,
            status: res.status,
            amount_minor: res.amount_minor,
            currency: res.currency.as_str().to_owned(),
        }))
    }

    /// Reverses a transaction via smart void or refund using [`UnifiedPaymentGateway::refund_or_void`].
    async fn refund_or_void(
        &self,
        request: Request<RefundOrVoidRequest>,
    ) -> Result<Response<RefundOrVoidResponse>, Status> {
        let req = request.into_inner();
        let amount_minor = req
            .amount_minor
            .map(parse_non_negative_amount)
            .transpose()?;

        let res = self
            .client
            .refund_or_void(&req.transaction_id, amount_minor)
            .await
            .map_err(to_tonic_status)?;

        Ok(Response::new(RefundOrVoidResponse {
            transaction_id: res.transaction_id,
            action: res.action,
            amount_reversed_minor: res.amount_reversed_minor,
            currency: res.currency.as_str().to_owned(),
        }))
    }

    /// Verifies and parses an inbound Moyasar webhook via [`UnifiedPaymentGateway::verify_webhook`].
    async fn verify_webhook(
        &self,
        request: Request<VerifyWebhookRequest>,
    ) -> Result<Response<VerifyWebhookResponse>, Status> {
        let req = request.into_inner();
        let headers = build_webhook_headers(&req.signature, &req.secret_token)?;

        let event = self
            .client
            .verify_webhook(&headers, &req.raw_body)
            .map_err(to_tonic_status)?;

        Ok(Response::new(VerifyWebhookResponse {
            id: event.id.to_string(),
            event_type: event.event_type,
            created_at: event.created_at.to_rfc3339(),
            live: event.live,
            data_json: event.data.to_string(),
        }))
    }
}
