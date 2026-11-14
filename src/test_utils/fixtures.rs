//! Deterministic JSON fixtures for all Moyasar webhook events, STC Pay, SADAD, and ISO 8583 codes.

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

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

/// Builds a deterministic webhook JSON payload for `event_type` and `secret_token`.
#[must_use]
pub fn webhook_event_fixture(event_type: &str, secret_token: &str) -> Value {
    let event_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, event_type.as_bytes());
    json!({
        "id": event_id,
        "type": event_type,
        "created_at": Utc::now().to_rfc3339(),
        "secret_token": secret_token,
        "account_name": "some_moyasar_sandbox",
        "live": false,
        "data": {
            "id": "pay_01J9FIXTURE",
            "status": "paid",
            "amount": 15000,
            "currency": "SAR"
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
        STCPAY_SANDBOX_INSUFFICIENT_FUNDS => ("failed", "Insufficient funds"),
        _ => ("failed", "Rejected by STC Pay"),
    };
    let now = Utc::now().to_rfc3339();
    json!({
        "id": "pay_stcpay_sandbox",
        "status": status,
        "amount": 10000,
        "amount_format": "100.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": 0,
        "refunded_format": "0.00 SAR",
        "captured": if status == "paid" { 10000 } else { 0 },
        "captured_format": if status == "paid" { "100.00 SAR" } else { "0.00 SAR" },
        "currency": "SAR",
        "source": {
            "type": "stcpay",
            "mobile": mobile,
            "reference_number": "STC-REF-001",
            "message": message
        },
        "created_at": now,
        "updated_at": now
    })
}

/// Builds a deterministic SADAD sandbox payment fixture using biller `007`.
#[must_use]
pub fn sadad_sandbox_fixture() -> Value {
    let now = Utc::now().to_rfc3339();
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
            "biller_code": SADAD_SANDBOX_BILLER,
            "bill_number": "SADAD-BILL-007"
        },
        "created_at": now,
        "updated_at": now
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

    #[test]
    fn tc_8_3_0_02_generates_all_12_webhook_and_sandbox_fixtures() {
        let fixtures = all_webhook_event_fixtures("whsec_test");
        assert_eq!(fixtures.len(), 12);
        assert!(fixtures.iter().any(|(k, _)| *k == "payment_faild"));

        assert_eq!(
            stcpay_sandbox_fixture(STCPAY_SANDBOX_SUCCESS)["status"],
            "paid"
        );
        assert_eq!(
            stcpay_sandbox_fixture(STCPAY_SANDBOX_REJECTED)["status"],
            "failed"
        );
        assert_eq!(
            stcpay_sandbox_fixture(STCPAY_SANDBOX_INSUFFICIENT_FUNDS)["status"],
            "failed"
        );
        assert_eq!(sadad_sandbox_fixture()["source"]["biller_code"], "007");
        assert_eq!(
            iso8583_error_fixture("51", "Insufficient funds")["response_code"],
            "51"
        );
    }
}
