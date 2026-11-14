//! Deterministic JSON fixtures for all Moyasar webhook events, STC Pay, SADAD, and ISO 8583 codes.

use serde_json::{Value, json};
use uuid::Uuid;

/// Fixed RFC 3339 timestamp used across deterministic fixtures.
pub const FIXTURE_TIMESTAMP: &str = "2026-11-14T12:00:00Z";

/// Canonical 11 webhook event types plus Moyasar's legacy `"payment_faild"` typo.
pub const ALL_WEBHOOK_EVENT_STRINGS: [&str; 12] = [
    "payment_paid",
    "payment_failed",
    "payment_faild",
    "payment_refunded",
    "payment_voided",
    "payment_authorized",
    "payment_captured",
    "payment_verified",
    "payment_abandoned",
    "payment_expired",
    "card_auth_authenticated",
    "card_auth_failed",
];

/// STC Pay sandbox mobile number that always succeeds.
pub const STCPAY_SANDBOX_SUCCESS: &str = "0515555555";
/// STC Pay sandbox mobile number that always rejects.
pub const STCPAY_SANDBOX_REJECTED: &str = "0515555558";
/// STC Pay sandbox mobile number that simulates insufficient funds.
pub const STCPAY_SANDBOX_INSUFFICIENT_FUNDS: &str = "0515555559";
/// SADAD sandbox biller code.
pub const SADAD_SANDBOX_BILLER: &str = "007";

/// Builds a deterministic webhook JSON payload for `event_type` and `secret_token` at [`FIXTURE_TIMESTAMP`].
#[must_use]
pub fn webhook_event_fixture(event_type: &str, secret_token: &str) -> Value {
    webhook_event_fixture_at(event_type, secret_token, FIXTURE_TIMESTAMP)
}

/// Builds a deterministic webhook JSON payload at an explicit RFC 3339 `created_at` timestamp.
#[must_use]
pub fn webhook_event_fixture_at(event_type: &str, secret_token: &str, created_at: &str) -> Value {
    let event_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, event_type.as_bytes());
    let data = if event_type.starts_with("card_auth_") {
        card_auth_fixture_data(event_type)
    } else {
        payment_fixture_data(event_type, created_at)
    };
    json!({
        "id": event_id,
        "type": event_type,
        "created_at": created_at,
        "secret_token": secret_token,
        "account_name": "some_moyasar_sandbox",
        "live": false,
        "data": data
    })
}

fn payment_fixture_data(event_type: &str, created_at: &str) -> Value {
    let status = match event_type {
        "payment_failed" | "payment_faild" => "failed",
        "payment_refunded" => "refunded",
        "payment_voided" => "voided",
        "payment_authorized" => "authorized",
        "payment_captured" => "captured",
        "payment_verified" => "verified",
        "payment_expired" => "expired",
        "payment_abandoned" => "initiated",
        _ => "paid",
    };
    let refunded = if status == "refunded" { 15000_u64 } else { 0 };
    let refunded_fmt = if refunded > 0 {
        "150.00 SAR"
    } else {
        "0.00 SAR"
    };
    let is_captured = matches!(status, "paid" | "captured" | "refunded");
    let (captured, captured_fmt) = if is_captured {
        (15000_u64, "150.00 SAR")
    } else {
        (0_u64, "0.00 SAR")
    };
    json!({
        "id": "pay_01J9FIXTURE",
        "status": status,
        "amount": 15000,
        "amount_format": "150.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": refunded,
        "refunded_format": refunded_fmt,
        "captured": captured,
        "captured_format": captured_fmt,
        "currency": "SAR",
        "source": {"type": "creditcard", "company": "mada"},
        "created_at": created_at,
        "updated_at": created_at
    })
}

fn card_auth_fixture_data(event_type: &str) -> Value {
    let is_auth = event_type == "card_auth_authenticated";
    let status = if is_auth { "authenticated" } else { "failed" };
    let txn_status = if is_auth { "Y" } else { "N" };
    json!({
        "id": "ca_01J9FIXTURE",
        "status": status,
        "transaction_url": "https://api.moyasar.com/v1/card_auth/ca_01J9FIXTURE/prepare",
        "result": {
            "eci": "05",
            "authentication_value": "AAABBIIFmAAAAAAAAAAAAAAAAAA=",
            "ds_transaction_id": "11111111-2222-3333-4444-555555555555",
            "version": "2.2.0",
            "transaction_status": txn_status,
            "auth_scheme": "visa",
            "is_frictionless": true
        }
    })
}

/// Generates all 12 webhook event fixtures (11 canonical + `"payment_faild"`).
#[must_use]
pub fn all_webhook_event_fixtures(secret_token: &str) -> Vec<(&'static str, Value)> {
    ALL_WEBHOOK_EVENT_STRINGS
        .iter()
        .map(|&name| (name, webhook_event_fixture(name, secret_token)))
        .collect()
}

/// Builds a deterministic STC Pay sandbox payment fixture for `0515555555` / `0515555558` / `0515555559`.
#[must_use]
pub fn stcpay_sandbox_fixture(mobile: &str) -> Value {
    let (status, message) = match mobile {
        STCPAY_SANDBOX_SUCCESS => ("paid", "APPROVED"),
        STCPAY_SANDBOX_REJECTED => ("failed", "Rejected by STC Pay"),
        STCPAY_SANDBOX_INSUFFICIENT_FUNDS => ("failed", "Insufficient funds"),
        _ => ("failed", "Rejected by STC Pay"),
    };
    let (captured, captured_format) = if status == "paid" {
        (10000_u64, "100.00 SAR")
    } else {
        (0_u64, "0.00 SAR")
    };
    json!({
        "id": "pay_stcpay_sandbox",
        "status": status,
        "amount": 10000,
        "amount_format": "100.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": 0,
        "refunded_format": "0.00 SAR",
        "captured": captured,
        "captured_format": captured_format,
        "currency": "SAR",
        "source": {
            "type": "stcpay",
            "mobile": mobile,
            "reference_number": "STC-REF-001",
            "message": message
        },
        "created_at": FIXTURE_TIMESTAMP,
        "updated_at": FIXTURE_TIMESTAMP
    })
}

/// Builds a deterministic SADAD sandbox payment fixture using biller `007`.
#[must_use]
pub fn sadad_sandbox_fixture() -> Value {
    json!({
        "id": "pay_sadad_sandbox",
        "status": "initiated",
        "amount": 25000,
        "amount_format": "250.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": 0,
        "refunded_format": "0.00 SAR",
        "captured": 0,
        "captured_format": "0.00 SAR",
        "currency": "SAR",
        "source": {
            "type": "sadad",
            "provider_name": "Moyasar Sandbox SADAD",
            "biller_code": SADAD_SANDBOX_BILLER,
            "sadad_number": "SADAD-BILL-007",
            "bill_number": "SADAD-BILL-007"
        },
        "created_at": FIXTURE_TIMESTAMP,
        "updated_at": FIXTURE_TIMESTAMP
    })
}

/// Builds an ISO 8583 acquirer error payload fixture for `code` (e.g. `"05"`, `"51"`, `"54"`).
#[must_use]
pub fn iso8583_error_fixture(code: &str, message: &str) -> Value {
    json!({
        "type": "invalid_request_error",
        "message": message,
        "response_code": code,
        "errors": {}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::error::ApiErrorPayload;

    #[test]
    fn tc_8_3_0_02_generates_all_12_webhook_and_sandbox_fixtures() {
        assert_eq!(
            webhook_event_fixture("payment_paid", "whsec_test"),
            webhook_event_fixture("payment_paid", "whsec_test")
        );
        let fixtures = all_webhook_event_fixtures("whsec_test");
        assert_eq!(fixtures.len(), 12);
        assert!(fixtures.iter().any(|(k, _)| *k == "payment_faild"));

        #[cfg(all(
            feature = "webhook-engine",
            feature = "payments",
            feature = "card-auths"
        ))]
        for (name, val) in &fixtures {
            let evt: crate::webhook_engine::WebhookEvent =
                serde_json::from_value(val.clone()).expect("valid WebhookEvent");
            if name.starts_with("card_auth_") {
                evt.into_card_auth().expect("valid CardAuthentication");
            } else {
                let p = evt.into_payment().expect("valid Payment");
                if matches!(*name, "payment_authorized" | "payment_voided") {
                    assert_eq!(p.captured, 0);
                    assert_eq!(p.captured_format, "0.00 SAR");
                }
            }
        }

        verify_sandbox_and_iso_fixtures();
    }

    fn verify_sandbox_and_iso_fixtures() {
        #[cfg(feature = "payments")]
        {
            use crate::resources::payments::{
                Payment, PaymentStatus, SadadBillSourceResponse, StcPaySourceResponse,
            };

            for (mobile, expected_status) in [
                (STCPAY_SANDBOX_SUCCESS, PaymentStatus::Paid),
                (STCPAY_SANDBOX_REJECTED, PaymentStatus::Failed),
                (STCPAY_SANDBOX_INSUFFICIENT_FUNDS, PaymentStatus::Failed),
            ] {
                let raw = stcpay_sandbox_fixture(mobile);
                let payment: Payment = serde_json::from_value(raw.clone()).expect("valid Payment");
                assert_eq!(payment.status, expected_status);
                let src: StcPaySourceResponse =
                    serde_json::from_value(raw["source"].clone()).expect("valid StcPaySource");
                assert_eq!(src.mobile.as_deref(), Some(mobile));
            }

            let sadad_raw = sadad_sandbox_fixture();
            let sadad_pay: Payment =
                serde_json::from_value(sadad_raw.clone()).expect("valid SADAD Payment");
            assert_eq!(sadad_pay.status, PaymentStatus::Initiated);
            let sadad_src: SadadBillSourceResponse =
                serde_json::from_value(sadad_raw["source"].clone()).expect("valid SadadBillSource");
            assert_eq!(sadad_src.biller_code, "007");
            assert_eq!(sadad_src.sadad_number, "SADAD-BILL-007");
        }

        for (code, msg) in [
            ("05", "Do not honor"),
            ("51", "Insufficient funds"),
            ("54", "Expired card"),
        ] {
            let err_val = iso8583_error_fixture(code, msg);
            let parsed: ApiErrorPayload =
                serde_json::from_value(err_val).expect("valid ApiErrorPayload");
            assert_eq!(parsed.message.as_deref(), Some(msg));
            assert!(parsed.response_code.is_some());
        }
    }
}
