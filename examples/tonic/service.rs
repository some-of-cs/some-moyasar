//! `MoyasarGrpcService` implementation wrapping [`UnifiedPaymentGateway`].

#![allow(clippy::result_large_err)]
#![allow(dead_code)]
use http::header::{HeaderMap, HeaderName, HeaderValue};
use some_moyasar::prelude::{
    MoyasarClient, SecretKey, UnifiedIntentRequest, UnifiedPaymentGateway,
    UnifiedTokenChargeRequest, WEBHOOK_SECRET_TOKEN_HEADER, WEBHOOK_SIGNATURE_HEADER,
};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::proto::{
    ChargeTokenRpcRequest, ChargeTokenRpcResponse, CreateIntentRpcRequest, CreateIntentRpcResponse,
    MoyasarPaymentGateway, RefundOrVoidRpcRequest, RefundOrVoidRpcResponse,
    VerifyWebhookRpcRequest, VerifyWebhookRpcResponse,
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

/// Converts an unsigned minor-unit amount into a signed proto `i64`, failing on overflow.
fn to_proto_amount(amount_minor: u64) -> Result<i64, Status> {
    i64::try_from(amount_minor).map_err(|_| Status::internal("amount exceeds i64 range"))
}

/// Parses a caller-supplied UUID idempotency key, returning `INVALID_ARGUMENT` if malformed.
fn parse_idempotency_key(raw: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw)
        .map_err(|_| Status::invalid_argument("idempotency_key must be a valid UUID"))
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
        request: Request<CreateIntentRpcRequest>,
    ) -> Result<Response<CreateIntentRpcResponse>, Status> {
        let req = request.into_inner();
        let amount_minor = parse_non_negative_amount(req.amount_minor)?;
        let mut unified = UnifiedIntentRequest::new(amount_minor, &req.currency, &req.callback_url);
        if let Some(desc) = req.description.as_deref() {
            unified = unified.description(desc);
        }

        let res = self
            .client
            .create_intent(unified)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(CreateIntentRpcResponse {
            id: res.id,
            client_secret: res.client_secret,
            amount_minor: to_proto_amount(res.amount_minor)?,
            currency: res.currency.as_str().to_owned(),
            expires_at: res.expires_at.to_rfc3339(),
        }))
    }

    /// Charges a saved card token off-session via [`UnifiedPaymentGateway::charge_saved_token`].
    async fn charge_saved_token(
        &self,
        request: Request<ChargeTokenRpcRequest>,
    ) -> Result<Response<ChargeTokenRpcResponse>, Status> {
        let req = request.into_inner();
        let amount_minor = parse_non_negative_amount(req.amount_minor)?;
        let idempotency_key = parse_idempotency_key(&req.idempotency_key)?;
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
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(ChargeTokenRpcResponse {
            transaction_id: res.transaction_id,
            status: res.status,
            amount_minor: to_proto_amount(res.amount_minor)?,
            currency: res.currency.as_str().to_owned(),
        }))
    }

    /// Reverses a transaction via smart void or refund using [`UnifiedPaymentGateway::refund_or_void`].
    async fn refund_or_void(
        &self,
        request: Request<RefundOrVoidRpcRequest>,
    ) -> Result<Response<RefundOrVoidRpcResponse>, Status> {
        let req = request.into_inner();
        let amount_minor = req
            .amount_minor
            .map(parse_non_negative_amount)
            .transpose()?;

        let res = self
            .client
            .refund_or_void(&req.transaction_id, amount_minor)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(RefundOrVoidRpcResponse {
            transaction_id: res.transaction_id,
            action: res.action,
            amount_reversed_minor: to_proto_amount(res.amount_reversed_minor)?,
            currency: res.currency.as_str().to_owned(),
        }))
    }

    /// Verifies and parses an inbound Moyasar webhook via [`UnifiedPaymentGateway::verify_webhook`].
    async fn verify_webhook(
        &self,
        request: Request<VerifyWebhookRpcRequest>,
    ) -> Result<Response<VerifyWebhookRpcResponse>, Status> {
        let req = request.into_inner();
        let headers = build_webhook_headers(&req.signature, &req.secret_token)?;

        let event = self
            .client
            .verify_webhook(&headers, &req.raw_body)
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(VerifyWebhookRpcResponse {
            id: event.id.to_string(),
            event_type: event.event_type,
            created_at: event.created_at.to_rfc3339(),
            live: event.live,
            payload_json: event.data.to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use tonic::Code;

    use super::*;

    #[test]
    fn parse_non_negative_amount_accepts_zero_and_positive() {
        assert_eq!(parse_non_negative_amount(0).expect("zero valid"), 0);
        assert_eq!(
            parse_non_negative_amount(15_000).expect("positive valid"),
            15_000
        );
    }

    #[test]
    fn parse_non_negative_amount_rejects_negative() {
        let err = parse_non_negative_amount(-1).expect_err("negative must fail");
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    #[test]
    fn to_proto_amount_converts_valid_and_rejects_overflow() {
        assert_eq!(to_proto_amount(25_000).expect("valid u64"), 25_000);
        let err = to_proto_amount(u64::MAX).expect_err("overflow must fail");
        assert_eq!(err.code(), Code::Internal);
        assert_eq!(err.message(), "amount exceeds i64 range");
    }

    #[test]
    fn build_webhook_headers_inserts_non_empty_and_skips_empty() {
        let headers = build_webhook_headers("sig_abc", "").expect("valid headers");
        assert_eq!(
            headers
                .get(WEBHOOK_SIGNATURE_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("sig_abc")
        );
        assert!(headers.get(WEBHOOK_SECRET_TOKEN_HEADER).is_none());
    }

    #[test]
    fn build_webhook_headers_rejects_invalid_header_value() {
        let err = build_webhook_headers("bad\nvalue", "secret").expect_err("newline must fail");
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    #[test]
    fn parse_idempotency_key_accepts_uuid_and_rejects_invalid() {
        let valid = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(
            parse_idempotency_key(valid)
                .expect("valid uuid")
                .to_string(),
            valid
        );
        let err = parse_idempotency_key("not-a-uuid").expect_err("invalid uuid must fail");
        assert_eq!(err.code(), Code::InvalidArgument);
    }
}
