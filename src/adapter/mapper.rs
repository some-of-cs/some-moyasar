//! Pure request, response, error, and webhook mapping helpers for the Moyasar adapter.

use serde_json::{Map, Value, json};
use some_payment_core::{
    CaptureMode, CheckoutSessionRequest, CreatePaymentRequest, Money, NextAction,
    NormalizedWebhookEvent, PaymentError, PaymentResult, PaymentStatus, ProviderId,
    ProviderReference, RefundRequest, RefundResult, RefundStatus, VerificationChallenge,
};

use crate::alignment::checkout_intents::{InvoiceCheckoutInput, map_invoice_checkout};
use crate::alignment::idempotency_errors::{
    ClassifiedMoyasarError, MoyasarErrorClass, map_moyasar_error,
};
use crate::alignment::pan_and_status::{
    MappedNextAction, MappedPaymentStatus, MoyasarPaymentSnapshot, RawCardPolicy, guard_raw_pan,
    map_moyasar_status,
};
use crate::alignment::preflight_rules::{
    MoyasarPreflightInput, SplitCheckItem, validate_moyasar_preflight, validate_uuid_id,
};
use crate::alignment::webhook_handshake::{
    NormalizedMoyasarEvent, extract_verification_challenge, normalize_moyasar_event,
};
use crate::alignment::webhook_token_verifier::VerifiedMoyasarWebhook;
use crate::kernel::MoyasarError;

const MAPPED_STATUS_TABLE: &[(MappedPaymentStatus, PaymentStatus)] = &[
    (MappedPaymentStatus::Initiated, PaymentStatus::Initiated),
    (
        MappedPaymentStatus::RequiresAction,
        PaymentStatus::RequiresAction,
    ),
    (MappedPaymentStatus::Authorized, PaymentStatus::Authorized),
    (MappedPaymentStatus::Captured, PaymentStatus::Captured),
    (MappedPaymentStatus::Paid, PaymentStatus::Paid),
    (MappedPaymentStatus::Refunded, PaymentStatus::Refunded),
    (MappedPaymentStatus::Voided, PaymentStatus::Voided),
    (MappedPaymentStatus::Failed, PaymentStatus::Failed),
    (
        MappedPaymentStatus::SetupCompleted,
        PaymentStatus::SetupCompleted,
    ),
];

const WEBHOOK_STATUS_TABLE: &[(NormalizedMoyasarEvent, PaymentStatus)] = &[
    (NormalizedMoyasarEvent::PaymentPaid, PaymentStatus::Paid),
    (NormalizedMoyasarEvent::PaymentFailed, PaymentStatus::Failed),
    (
        NormalizedMoyasarEvent::PaymentAuthorized,
        PaymentStatus::Authorized,
    ),
    (
        NormalizedMoyasarEvent::PaymentCaptured,
        PaymentStatus::Captured,
    ),
    (
        NormalizedMoyasarEvent::PaymentRefunded,
        PaymentStatus::Refunded,
    ),
    (NormalizedMoyasarEvent::PaymentVoided, PaymentStatus::Voided),
    (
        NormalizedMoyasarEvent::PaymentAbandoned,
        PaymentStatus::Canceled,
    ),
    (
        NormalizedMoyasarEvent::PaymentVerified,
        PaymentStatus::SetupCompleted,
    ),
];

pub(super) fn map_local_error(provider: &ProviderId, err: MoyasarError) -> PaymentError {
    match err {
        MoyasarError::InvalidKey | MoyasarError::KeyTypeMismatch => {
            PaymentError::AuthenticationError {
                provider: provider.clone(),
                message: err.to_string(),
            }
        }
        MoyasarError::InvalidWebhookSecret => PaymentError::WebhookVerificationFailed {
            provider: provider.clone(),
            reason: err.to_string(),
        },
        other => PaymentError::validation(other.to_string()),
    }
}

pub(super) fn shape_error(provider: &ProviderId, kind: &str) -> PaymentError {
    PaymentError::ProviderError {
        provider: provider.clone(),
        http_status: None,
        vendor_code: None,
        message: format!("unexpected Moyasar {kind} response shape"),
    }
}

pub(super) fn map_api_error(
    provider: &ProviderId,
    status: u16,
    body: &str,
    retry_after: Option<u64>,
) -> PaymentError {
    let parsed = serde_json::from_str::<Value>(body).ok();
    let err_type = parsed.as_ref().and_then(|v| v.get("type")?.as_str());
    let err_code = parsed.as_ref().and_then(|v| v.get("code")?.as_str());
    let message = parsed
        .as_ref()
        .and_then(|v| v.get("message")?.as_str())
        .map_or_else(|| format!("Moyasar HTTP {status}"), str::to_owned);
    let classified = map_moyasar_error(Some(status), err_type, err_code);
    map_classified_error(provider, classified, message, retry_after)
}

fn map_client_or_flow_error(
    provider: &ProviderId,
    classified: &ClassifiedMoyasarError,
    message: &str,
) -> Option<PaymentError> {
    match classified.class {
        MoyasarErrorClass::ThreeDsFailed => Some(PaymentError::ThreeDsFailed {
            provider: provider.clone(),
            code: classified.vendor_code.clone(),
            message: message.to_owned(),
        }),
        MoyasarErrorClass::AuthenticationError => Some(PaymentError::AuthenticationError {
            provider: provider.clone(),
            message: message.to_owned(),
        }),
        MoyasarErrorClass::ValidationError => Some(PaymentError::ValidationError {
            message: message.to_owned(),
        }),
        MoyasarErrorClass::CardDeclined => Some(PaymentError::CardDeclined {
            provider: provider.clone(),
            decline_code: classified.vendor_code.clone(),
            advice_code: None,
            message: message.to_owned(),
        }),
        _ => None,
    }
}

fn map_classified_error(
    provider: &ProviderId,
    classified: ClassifiedMoyasarError,
    message: String,
    retry_after: Option<u64>,
) -> PaymentError {
    if let Some(err) = map_client_or_flow_error(provider, &classified, &message) {
        return err;
    }
    match classified.class {
        MoyasarErrorClass::RateLimited => PaymentError::RateLimited {
            provider: provider.clone(),
            retry_after_secs: retry_after,
        },
        MoyasarErrorClass::ProviderUnavailable => PaymentError::ProviderUnavailable {
            provider: provider.clone(),
            http_status: classified.http_status,
            indeterminate: classified.indeterminate,
            message,
        },
        _ => PaymentError::ProviderError {
            provider: provider.clone(),
            http_status: classified.http_status,
            vendor_code: classified.vendor_code,
            message,
        },
    }
}

pub(super) fn map_transport_error(provider: &ProviderId, err: &reqwest::Error) -> PaymentError {
    let message = if err.is_timeout() {
        "Moyasar request timed out"
    } else {
        "Moyasar transport failure"
    };
    PaymentError::ProviderUnavailable {
        provider: provider.clone(),
        http_status: None,
        indeterminate: true,
        message: message.to_owned(),
    }
}

pub(super) fn extract_payment_uuid(
    provider: &ProviderId,
    reference: &ProviderReference,
) -> Result<uuid::Uuid, PaymentError> {
    let raw_id = reference
        .payment_id()
        .ok_or_else(|| PaymentError::validation("ProviderReference requires payment_id"))?;
    validate_uuid_id(raw_id, "payment_id").map_err(|err| map_local_error(provider, err))
}

pub(super) fn resolve_source_type(req: &CreatePaymentRequest) -> Option<&str> {
    if let Some(explicit) = req.metadata.get("source_type").map(String::as_str) {
        return Some(explicit);
    }
    let token = req.payment_token.as_deref().map(str::trim)?;
    if token.eq_ignore_ascii_case("creditcard") || token.eq_ignore_ascii_case("credit_card") {
        return Some("creditcard");
    }
    Some("token")
}

pub(super) fn validate_create_guards(
    provider: &ProviderId,
    allow_raw_cards: bool,
    req: &CreatePaymentRequest,
    source_type: Option<&str>,
) -> Result<(), PaymentError> {
    let policy = RawCardPolicy { allow_raw_cards };
    if let Some(st) = source_type {
        guard_raw_pan(st, policy).map_err(|err| map_local_error(provider, err))?;
    }
    let splits: Vec<SplitCheckItem> = req
        .splits
        .iter()
        .map(|item| SplitCheckItem {
            recipient_id: item.recipient_id.clone(),
            amount: item.amount,
        })
        .collect();
    let preflight = MoyasarPreflightInput {
        payment_id: None,
        amount: req.money.amount(),
        manual_capture: matches!(req.capture_mode, CaptureMode::Manual),
        source_type,
        metadata: &req.metadata,
        splits: &splits,
    };
    validate_moyasar_preflight(&preflight).map_err(|err| map_local_error(provider, err))
}

pub(super) fn build_create_payment_body(
    req: &CreatePaymentRequest,
    source_type: Option<&str>,
    given_id: Option<uuid::Uuid>,
) -> Value {
    let mut map = Map::new();
    let desc = req.description.as_deref().unwrap_or(&req.order_id);
    map.insert("amount".to_owned(), json!(req.money.amount()));
    map.insert("currency".to_owned(), json!(req.money.currency().as_str()));
    map.insert("description".to_owned(), json!(desc));
    if let Some(url) = req.callback_url.as_deref() {
        map.insert("callback_url".to_owned(), json!(url));
    }
    if let Some(id) = given_id {
        map.insert("given_id".to_owned(), json!(id.to_string()));
    }
    if !req.metadata.is_empty() {
        map.insert("metadata".to_owned(), json!(&req.metadata));
    }
    map.insert("source".to_owned(), build_source(req, source_type));
    if !req.splits.is_empty() {
        let splits: Vec<Value> = req
            .splits
            .iter()
            .map(|s| {
                json!({
                    "recipient_id": s.recipient_id,
                    "amount": s.amount,
                    "fee_source": s.fee_source,
                })
            })
            .collect();
        map.insert("splits".to_owned(), Value::Array(splits));
    }
    Value::Object(map)
}

fn build_source(req: &CreatePaymentRequest, source_type: Option<&str>) -> Value {
    let mut source = Map::new();
    let kind = source_type.unwrap_or("token");
    source.insert("type".to_owned(), json!(kind));
    if matches!(req.capture_mode, CaptureMode::Manual) {
        source.insert("manual".to_owned(), json!(true));
    }
    if let Some(token) = req
        .payment_token
        .as_deref()
        .filter(|_| kind.eq_ignore_ascii_case("token"))
    {
        source.insert("token".to_owned(), json!(token));
    }
    Value::Object(source)
}

pub(super) fn build_checkout_body(req: &CheckoutSessionRequest) -> Value {
    let mut map = Map::new();
    let desc = req.description.as_deref().unwrap_or(&req.order_id);
    map.insert("amount".to_owned(), json!(req.money.amount()));
    map.insert("currency".to_owned(), json!(req.money.currency().as_str()));
    map.insert("description".to_owned(), json!(desc));
    map.insert("success_url".to_owned(), json!(&req.success_url));
    if let Some(cancel) = req.cancel_url.as_deref() {
        map.insert("back_url".to_owned(), json!(cancel));
    }
    if !req.metadata.is_empty() {
        map.insert("metadata".to_owned(), json!(&req.metadata));
    }
    Value::Object(map)
}

pub(super) fn parse_payment_result(
    provider: &ProviderId,
    livemode: bool,
    payload: &Value,
    order_id: Option<&str>,
) -> Result<PaymentResult, PaymentError> {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| shape_error(provider, "payment"))?;
    let status_raw = payload
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| shape_error(provider, "payment"))?;
    let amount = payload
        .get("amount")
        .and_then(Value::as_u64)
        .ok_or_else(|| shape_error(provider, "payment"))?;
    let currency = payload
        .get("currency")
        .and_then(Value::as_str)
        .ok_or_else(|| shape_error(provider, "payment"))?;
    let outcome = map_moyasar_status(&payment_snapshot(status_raw, payload.get("source")))
        .map_err(|err| map_local_error(provider, err))?;
    let mut ref_builder = ProviderReference::builder(provider.clone()).payment_id(id);
    if let Some(order) = order_id {
        ref_builder = ref_builder.order_id(order);
    }
    Ok(PaymentResult {
        reference: ref_builder.build()?,
        status: map_payment_status(outcome.status),
        money: Money::new(amount, currency)?,
        captured_amount: payload.get("captured").and_then(Value::as_u64),
        refunded_amount: payload.get("refunded").and_then(Value::as_u64),
        next_action: outcome.next_action.map(map_next_action),
        checkout_url: None,
        livemode,
    })
}

fn payment_snapshot<'a>(status: &'a str, source: Option<&'a Value>) -> MoyasarPaymentSnapshot<'a> {
    let bill_reference = source.and_then(|s| {
        s.get("bill_reference")
            .or_else(|| s.get("bill_number"))
            .or_else(|| s.get("reference_number"))
            .and_then(Value::as_str)
    });
    MoyasarPaymentSnapshot {
        status,
        source_type: source.and_then(|s| s.get("type")?.as_str()),
        transaction_url: source.and_then(|s| s.get("transaction_url")?.as_str()),
        bill_reference,
    }
}

fn map_payment_status(mapped: MappedPaymentStatus) -> PaymentStatus {
    MAPPED_STATUS_TABLE
        .iter()
        .find(|(src, _)| *src == mapped)
        .map_or(PaymentStatus::Initiated, |(_, dst)| *dst)
}

fn map_next_action(action: MappedNextAction) -> NextAction {
    match action {
        MappedNextAction::Redirect { url } => NextAction::Redirect { url },
        MappedNextAction::OtpChallenge { submission_url } => {
            NextAction::OtpChallenge { submission_url }
        }
        MappedNextAction::OfflineBill {
            bill_reference,
            instructions_url,
        } => NextAction::OfflineBill {
            bill_reference,
            instructions_url,
        },
    }
}

pub(super) fn parse_invoice_result(
    provider: &ProviderId,
    livemode: bool,
    payload: &Value,
    order_id: &str,
) -> Result<PaymentResult, PaymentError> {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| shape_error(provider, "invoice"))?;
    let amount = payload
        .get("amount")
        .and_then(Value::as_u64)
        .ok_or_else(|| shape_error(provider, "invoice"))?;
    let currency = payload
        .get("currency")
        .and_then(Value::as_str)
        .ok_or_else(|| shape_error(provider, "invoice"))?;
    let outcome = map_invoice_checkout(InvoiceCheckoutInput {
        invoice_id: id,
        invoice_url: payload.get("url").and_then(Value::as_str),
        amount,
        currency,
    })
    .map_err(|err| map_local_error(provider, err))?;
    let reference = ProviderReference::builder(provider.clone())
        .checkout_session_id(&outcome.invoice_id)
        .order_id(order_id)
        .build()?;
    Ok(PaymentResult {
        reference,
        status: PaymentStatus::RequiresAction,
        money: Money::new(outcome.amount, &outcome.currency)?,
        captured_amount: None,
        refunded_amount: None,
        next_action: Some(NextAction::Redirect {
            url: outcome.checkout_url.clone(),
        }),
        checkout_url: Some(outcome.checkout_url),
        livemode,
    })
}

pub(super) fn parse_refund_result(
    provider: &ProviderId,
    payload: &Value,
    req: &RefundRequest,
) -> Result<RefundResult, PaymentError> {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| shape_error(provider, "refund"))?;
    let status_raw = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("refunded");
    let status = if status_raw.eq_ignore_ascii_case("failed") {
        RefundStatus::Failed
    } else {
        RefundStatus::Succeeded
    };
    let money = resolve_refund_money(provider, payload, req)?;
    Ok(RefundResult {
        refund_id: id.to_owned(),
        payment_reference: req.reference.clone(),
        status,
        money,
        reason_detail: None,
    })
}

fn resolve_refund_money(
    provider: &ProviderId,
    payload: &Value,
    req: &RefundRequest,
) -> Result<Money, PaymentError> {
    if let Some(partial) = req.amount.as_ref() {
        return Ok(partial.clone());
    }
    let amt = payload
        .get("refunded")
        .and_then(Value::as_u64)
        .filter(|&v| v > 0)
        .or_else(|| payload.get("amount").and_then(Value::as_u64))
        .ok_or_else(|| shape_error(provider, "refund"))?;
    let cur = payload
        .get("currency")
        .and_then(Value::as_str)
        .ok_or_else(|| shape_error(provider, "refund"))?;
    Money::new(amt, cur)
}

pub(super) fn map_webhook_event(
    provider: &ProviderId,
    verified: VerifiedMoyasarWebhook,
) -> Result<NormalizedWebhookEvent, PaymentError> {
    let normalized = normalize_moyasar_event(&verified.event_type);
    let status = normalized.and_then(|evt| {
        WEBHOOK_STATUS_TABLE
            .iter()
            .find(|(src, _)| *src == evt)
            .map(|(_, st)| *st)
    });
    let verification_challenge =
        extract_challenge_if_handshake(provider, normalized, &verified.raw_redacted)?;
    let (reference, money) = extract_webhook_context(provider, &verified.raw_redacted);
    Ok(NormalizedWebhookEvent {
        provider_id: provider.clone(),
        event_id: verified.event_id,
        event_type: verified.event_type,
        livemode: verified.livemode,
        requires_hydration: false,
        reference,
        status,
        money,
        verification_challenge,
        raw_redacted: verified.raw_redacted,
    })
}

fn extract_challenge_if_handshake(
    provider: &ProviderId,
    event: Option<NormalizedMoyasarEvent>,
    payload: &Value,
) -> Result<Option<VerificationChallenge>, PaymentError> {
    if event != Some(NormalizedMoyasarEvent::WebhookVerification) {
        return Ok(None);
    }
    let challenge =
        extract_verification_challenge(payload).map_err(|err| map_local_error(provider, err))?;
    Ok(Some(VerificationChallenge {
        webhook_id: challenge.webhook_id.clone(),
        verification_code: challenge.verification_code.clone(),
    }))
}

fn extract_webhook_context(
    provider: &ProviderId,
    payload: &Value,
) -> (Option<ProviderReference>, Option<Money>) {
    let Some(data) = payload.get("data") else {
        return (None, None);
    };
    let reference = data.get("id").and_then(Value::as_str).and_then(|pid| {
        ProviderReference::builder(provider.clone())
            .payment_id(pid)
            .build()
            .ok()
    });
    let money = data
        .get("amount")
        .and_then(Value::as_u64)
        .zip(data.get("currency").and_then(Value::as_str))
        .and_then(|(amt, cur)| Money::new(amt, cur).ok());
    (reference, money)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use some_payment_core::{
        CaptureMode, CreatePaymentRequest, Money, NextAction, PaymentError, PaymentStatus,
        ProviderId, ProviderReference, RefundRequest, RefundStatus,
    };

    use crate::alignment::webhook_token_verifier::VerifiedMoyasarWebhook;

    use super::{
        build_create_payment_body, map_api_error, map_webhook_event, parse_payment_result,
        parse_refund_result,
    };

    const VALID_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";

    fn moyasar_id() -> ProviderId {
        ProviderId::new("moyasar")
    }

    #[test]
    fn build_create_payment_body_includes_source_given_id_and_metadata() {
        let mut req = CreatePaymentRequest::new(Money::new(10_000, "SAR").expect("money"), "ORD-1");
        req.capture_mode = CaptureMode::Manual;
        req.payment_token = Some("tok_abc123".to_owned());
        req.callback_url = Some("https://example.com/cb".to_owned());
        req.metadata.insert("channel".to_owned(), "web".to_owned());
        let given_id = uuid::Uuid::parse_str(VALID_UUID).expect("uuid");
        let body = build_create_payment_body(&req, Some("token"), Some(given_id));
        assert_eq!(body["amount"], json!(10_000));
        assert_eq!(body["currency"], json!("SAR"));
        assert_eq!(body["description"], json!("ORD-1"));
        assert_eq!(body["given_id"], json!(VALID_UUID));
        assert_eq!(body["metadata"]["channel"], json!("web"));
        assert_eq!(body["source"]["type"], json!("token"));
        assert_eq!(body["source"]["manual"], json!(true));
        assert_eq!(body["source"]["token"], json!("tok_abc123"));
    }

    #[test]
    fn parse_payment_result_maps_status_next_action_and_amounts() {
        let payload = json!({
            "id": VALID_UUID,
            "status": "initiated",
            "amount": 5_000,
            "currency": "SAR",
            "captured": 0,
            "refunded": 0,
            "source": {
                "type": "creditcard",
                "transaction_url": "https://3ds.example.com/challenge"
            }
        });
        let result = parse_payment_result(&moyasar_id(), false, &payload, Some("ORD-9"))
            .expect("parsed payment");
        assert_eq!(result.status, PaymentStatus::RequiresAction);
        assert_eq!(result.money.amount(), 5_000);
        assert_eq!(result.reference.order_id(), Some("ORD-9"));
        assert!(matches!(
            result.next_action,
            Some(NextAction::Redirect { .. })
        ));
    }

    #[test]
    fn parse_refund_result_handles_partial_and_full_refunds() {
        let reference = ProviderReference::builder(moyasar_id())
            .payment_id(VALID_UUID)
            .build()
            .expect("ref");
        let payload = json!({
            "id": VALID_UUID,
            "status": "refunded",
            "amount": 5_000,
            "refunded": 5_000,
            "currency": "SAR"
        });
        let full_req = RefundRequest {
            reference: reference.clone(),
            amount: None,
            reason: None,
            idempotency_key: None,
        };
        let full = parse_refund_result(&moyasar_id(), &payload, &full_req).expect("full");
        assert_eq!(full.status, RefundStatus::Succeeded);
        assert_eq!(full.money.amount(), 5_000);

        let partial_req = RefundRequest {
            reference,
            amount: Some(Money::new(1_500, "SAR").expect("money")),
            reason: None,
            idempotency_key: None,
        };
        let partial = parse_refund_result(&moyasar_id(), &payload, &partial_req).expect("partial");
        assert_eq!(partial.money.amount(), 1_500);
    }

    #[test]
    fn map_api_error_classifies_auth_rate_limit_and_server_errors() {
        let id = moyasar_id();
        let auth = map_api_error(
            &id,
            401,
            r#"{"type":"authentication_error","message":"bad key"}"#,
            None,
        );
        assert!(matches!(auth, PaymentError::AuthenticationError { .. }));

        let rate = map_api_error(&id, 429, r#"{"message":"slow down"}"#, Some(30));
        assert!(matches!(
            rate,
            PaymentError::RateLimited {
                retry_after_secs: Some(30),
                ..
            }
        ));

        let unavailable = map_api_error(&id, 503, "upstream down", None);
        assert!(matches!(
            unavailable,
            PaymentError::ProviderUnavailable {
                indeterminate: true,
                ..
            }
        ));
    }

    #[test]
    fn map_webhook_event_maps_typo_status_and_verification_handshake() {
        let id = moyasar_id();
        let typo = VerifiedMoyasarWebhook {
            event_id: "evt_1".to_owned(),
            event_type: "payment_faild".to_owned(),
            livemode: false,
            raw_redacted: json!({
                "data": { "id": VALID_UUID, "amount": 2500, "currency": "SAR" }
            }),
        };
        let mapped = map_webhook_event(&id, typo).expect("mapped typo");
        assert_eq!(mapped.status, Some(PaymentStatus::Failed));
        assert_eq!(mapped.money.expect("money").amount(), 2500);

        let handshake = VerifiedMoyasarWebhook {
            event_id: "evt_2".to_owned(),
            event_type: "webhook_verification".to_owned(),
            livemode: true,
            raw_redacted: json!({
                "data": { "webhook_id": "wh_1", "verification_code": "654321" }
            }),
        };
        let hs = map_webhook_event(&id, handshake).expect("mapped handshake");
        let challenge = hs.verification_challenge.expect("challenge");
        assert_eq!(challenge.webhook_id, "wh_1");
        assert_eq!(challenge.verification_code, "654321");
    }
}
