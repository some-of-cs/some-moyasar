//! E2E tests for Rocket settlements and merchant balance endpoints.
#![forbid(unsafe_code)]

use rocket::http::Status;
use serde_json::{Value, json};
use some_moyasar::prelude::{
    Amount, Currency, MerchantBalance, Settlement, SettlementRecipientType,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// Deterministic ISO 8601 UTC timestamp used in mock settlement payloads.
const FIXTURE_CREATED_AT: &str = "2026-11-14T00:00:00Z";

/// Input parameters for constructing a deterministic [`Settlement`] JSON fixture.
struct SettlementFixture {
    /// Unique Moyasar settlement identifier.
    id: &'static str,
    /// Classification of the settlement recipient (e.g., `"entity"` or `"platform"`).
    recipient_type: &'static str,
    /// Gross settlement amount in minor units.
    amount: i64,
    /// Processing fee deducted in minor units.
    fee: i64,
    /// VAT/tax deducted in minor units.
    tax: i64,
    /// Merchant-facing settlement batch reference code.
    reference: &'static str,
    /// Count of settled transactions included in the batch.
    settlement_count: u32,
}

/// Builds a deterministic [`Settlement`] JSON fixture using integer minor units.
fn settlement_json(fixture: SettlementFixture) -> Value {
    json!({
        "id": fixture.id,
        "recipient_type": fixture.recipient_type,
        "currency": "SAR",
        "source_currency": "SAR",
        "amount": fixture.amount,
        "fee": fixture.fee,
        "tax": fixture.tax,
        "reference": fixture.reference,
        "settlement_count": fixture.settlement_count,
        "created_at": FIXTURE_CREATED_AT,
    })
}

/// Mounts a single-shot `GET` JSON responder on the mock server.
async fn mount_get_json(
    mock_server: &some_moyasar::test_utils::MoyasarMockServer,
    upstream_path: &str,
    status: u16,
    body: Value,
) {
    Mock::given(method("GET"))
        .and(path(upstream_path))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;
}

/// Expected minor-unit fields for asserting a [`Settlement`] entry.
struct ExpectedSettlementAmounts {
    /// Expected Moyasar settlement identifier.
    id: &'static str,
    /// Expected settlement recipient classification.
    recipient_type: SettlementRecipientType,
    /// Expected gross settlement amount in minor units.
    amount: i64,
    /// Expected processing fee in minor units.
    fee: i64,
    /// Expected VAT/tax in minor units.
    tax: i64,
    /// Expected net settlement amount in minor units (`amount - fee - tax`).
    net: i64,
}

/// Asserts that a [`Settlement`] entry matches the expected minor-unit amounts.
fn assert_settlement_entry(item: &Settlement, expected: ExpectedSettlementAmounts) {
    assert_eq!(item.id.as_str(), expected.id);
    assert_eq!((item.recipient_type, item.currency), (expected.recipient_type, Currency::SAR));
    let expected_amounts = (
        Amount::from_minor(expected.amount),
        Amount::from_minor(expected.fee),
        Amount::from_minor(expected.tax),
    );
    assert_eq!((item.amount, item.fee, item.tax), expected_amounts);
    assert_eq!(item.net(), Amount::from_minor(expected.net));
}

/// Verifies `GET /settlements` lists settlements with integer minor-unit `Amount`s.
#[rocket::async_test]
async fn lists_settlements_with_minor_unit_amounts() {
    let (mock_server, client) = super::spawn_test_client().await;
    let payload = json!({
        "settlements": [
            settlement_json(SettlementFixture {
                id: "stl_e2e_01",
                recipient_type: "entity",
                amount: 250_000,
                fee: 4_500,
                tax: 675,
                reference: "STL-2026-11-01",
                settlement_count: 42,
            }),
            settlement_json(SettlementFixture {
                id: "stl_e2e_02",
                recipient_type: "platform",
                amount: 100_000,
                fee: 2_000,
                tax: 300,
                reference: "STL-2026-11-02",
                settlement_count: 15,
            }),
        ],
        "meta": {
            "current_page": 1,
            "next_page": null,
            "prev_page": null,
            "total_pages": 1,
            "total_count": 2,
        }
    });
    mount_get_json(&mock_server, "/v1/settlements", 200, payload).await;

    let response = client.get("/settlements").dispatch().await;
    assert_eq!(response.status(), Status::Ok);

    let body: Value = response
        .into_json()
        .await
        .expect("settlement list JSON response");
    let items: Vec<Settlement> =
        serde_json::from_value(body["items"].clone()).expect("valid Settlement items");

    assert_eq!(items.len(), 2);
    assert_eq!(body["current_page"].as_u64(), Some(1));
    assert_eq!(body["total_count"].as_u64(), Some(2));
    assert!(body["next_page"].is_null());

    assert_settlement_entry(
        &items[0],
        ExpectedSettlementAmounts {
            id: "stl_e2e_01",
            recipient_type: SettlementRecipientType::Entity,
            amount: 250_000,
            fee: 4_500,
            tax: 675,
            net: 244_825,
        },
    );
    assert_settlement_entry(
        &items[1],
        ExpectedSettlementAmounts {
            id: "stl_e2e_02",
            recipient_type: SettlementRecipientType::Platform,
            amount: 100_000,
            fee: 2_000,
            tax: 300,
            net: 97_700,
        },
    );
}

/// Verifies `GET /settlements/<id>` returns `Settlement` and checked `amount - fee - tax` `net()`.
#[rocket::async_test]
async fn retrieves_settlement_with_checked_net_breakdown() {
    let (mock_server, client) = super::spawn_test_client().await;
    let payload = settlement_json(SettlementFixture {
        id: "stl_e2e_01",
        recipient_type: "entity",
        amount: 250_000,
        fee: 4_500,
        tax: 675,
        reference: "STL-2026-11-01",
        settlement_count: 42,
    });
    mount_get_json(&mock_server, "/v1/settlements/stl_e2e_01", 200, payload).await;

    let response = client.get("/settlements/stl_e2e_01").dispatch().await;
    assert_eq!(response.status(), Status::Ok);

    let body: Value = response
        .into_json()
        .await
        .expect("valid settlement breakdown JSON");
    let settlement: Settlement = serde_json::from_value(body["settlement"].clone())
        .expect("valid Settlement in breakdown response");

    assert_eq!(settlement.id.as_str(), "stl_e2e_01");
    assert_eq!(settlement.amount, Amount::from_minor(250_000));
    assert_eq!(settlement.fee, Amount::from_minor(4_500));
    assert_eq!(settlement.tax, Amount::from_minor(675));
    assert_eq!(settlement.net(), Amount::from_minor(244_825));
    assert_eq!(settlement.checked_net(), Some(Amount::from_minor(244_825)));
    assert_eq!(body["net_amount"].as_i64(), Some(244_825));
}

/// Verifies `GET /settlements/<id>` reports `null` `net_amount` on `i64` underflow.
#[rocket::async_test]
async fn retrieves_settlement_handles_checked_net_underflow() {
    let (mock_server, client) = super::spawn_test_client().await;
    let payload = settlement_json(SettlementFixture {
        id: "stl_e2e_underflow",
        recipient_type: "entity",
        amount: i64::MIN,
        fee: 1,
        tax: 0,
        reference: "STL-2026-11-03",
        settlement_count: 1,
    });
    mount_get_json(
        &mock_server,
        "/v1/settlements/stl_e2e_underflow",
        200,
        payload,
    )
    .await;

    let response = client
        .get("/settlements/stl_e2e_underflow")
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Ok);

    let body: Value = response
        .into_json()
        .await
        .expect("valid settlement underflow JSON");
    let settlement: Settlement = serde_json::from_value(body["settlement"].clone())
        .expect("valid Settlement in underflow response");

    assert_eq!(settlement.checked_net(), None);
    assert_eq!(settlement.net(), Amount::from_minor(0));
    assert!(body["net_amount"].is_null());
}

/// Verifies `GET /settlements/<id>` propagates upstream 404 as redacted JSON error.
#[rocket::async_test]
async fn retrieves_settlement_propagates_not_found_error() {
    let (mock_server, client) = super::spawn_test_client().await;
    let error_payload = json!({
        "type": "record_not_found",
        "message": "Settlement not found",
    });
    mount_get_json(
        &mock_server,
        "/v1/settlements/stl_missing_99",
        404,
        error_payload,
    )
    .await;

    let response = client.get("/settlements/stl_missing_99").dispatch().await;
    assert_eq!(response.status(), Status::NotFound);

    let body: Value = response.into_json().await.expect("valid error JSON");
    assert_eq!(body["error_type"], "record_not_found");
}

/// Verifies `GET /settlements/balance` returns `MerchantBalance` in integer minor units.
#[rocket::async_test]
async fn fetches_merchant_balance_in_minor_units() {
    let (mock_server, client) = super::spawn_test_client().await;
    let payload = json!({
        "currency": "SAR",
        "available": 150_000,
        "pending": 25_000,
        "reserved": 5_000,
        "available_format": "1,500.00 SAR",
        "pending_format": "250.00 SAR",
        "reserved_format": "50.00 SAR",
    });
    mount_get_json(&mock_server, "/v1/settlements/balance", 200, payload).await;

    let response = client.get("/settlements/balance").dispatch().await;
    assert_eq!(response.status(), Status::Ok);

    let body = response
        .into_string()
        .await
        .expect("merchant balance response body");
    let balance: MerchantBalance = serde_json::from_str(&body).expect("valid MerchantBalance JSON");

    assert_eq!(balance.currency, Currency::SAR);
    assert_eq!(balance.available, Amount::from_minor(150_000));
    assert_eq!(balance.pending, Amount::from_minor(25_000));
    assert_eq!(balance.reserved, Amount::from_minor(5_000));
    assert_eq!(balance.available_format.as_deref(), Some("1,500.00 SAR"));
    assert_eq!(balance.pending_format.as_deref(), Some("250.00 SAR"));
    assert_eq!(balance.reserved_format.as_deref(), Some("50.00 SAR"));
}

/// Verifies `GET /settlements/balance` maps upstream 401 authentication failure to 401.
#[rocket::async_test]
async fn fetches_merchant_balance_propagates_unauthorized_error() {
    let (mock_server, client) = super::spawn_test_client().await;
    let error_payload = json!({
        "type": "authentication_error",
        "message": "Invalid authentication credentials",
    });
    mount_get_json(&mock_server, "/v1/settlements/balance", 401, error_payload).await;

    let response = client.get("/settlements/balance").dispatch().await;
    assert_eq!(response.status(), Status::Unauthorized);

    let body: Value = response.into_json().await.expect("valid error JSON");
    assert_eq!(body["error_type"], "authentication_error");
}
