//! E2E tests for Rocket settlements and merchant balance endpoints.
#![forbid(unsafe_code)]

use rocket::http::Status;
use serde_json::{Value, json};
use some_moyasar::prelude::{
    Amount, Currency, MerchantBalance, PaginatedList, Settlement, SettlementRecipientType,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// Deterministic ISO 8601 UTC timestamp used in mock settlement payloads.
const FIXTURE_CREATED_AT: &str = "2026-11-14T00:00:00Z";

/// Builds a deterministic [`Settlement`] JSON fixture using integer minor units.
fn settlement_json(
    id: &str,
    recipient_type: &str,
    amount: i64,
    fee: i64,
    tax: i64,
    reference: &str,
    settlement_count: u32,
) -> Value {
    json!({
        "id": id,
        "recipient_type": recipient_type,
        "currency": "SAR",
        "source_currency": "SAR",
        "amount": amount,
        "fee": fee,
        "tax": tax,
        "reference": reference,
        "settlement_count": settlement_count,
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

/// Verifies `GET /settlements` lists settlements with integer minor-unit `Amount`s.
#[rocket::async_test]
async fn lists_settlements_with_minor_unit_amounts() {
    let (client, mock_server) = super::spawn_test_client().await;
    let payload = json!({
        "settlements": [
            settlement_json("stl_e2e_01", "entity", 250_000, 4_500, 675, "STL-2026-11-01", 42),
            settlement_json("stl_e2e_02", "platform", 100_000, 2_000, 300, "STL-2026-11-02", 15),
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

    let body = response
        .into_string()
        .await
        .expect("settlement list response body");
    let page: PaginatedList<Settlement> =
        serde_json::from_str(&body).expect("valid PaginatedList<Settlement>");

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.meta.current_page, 1);
    assert_eq!(page.meta.total_count, 2);

    let first = &page.items[0];
    assert_eq!(first.id.as_str(), "stl_e2e_01");
    assert_eq!(first.recipient_type, SettlementRecipientType::Entity);
    assert_eq!(first.currency, Currency::SAR);
    assert_eq!(first.amount, Amount::from_minor(250_000));
    assert_eq!(first.fee, Amount::from_minor(4_500));
    assert_eq!(first.tax, Amount::from_minor(675));
    assert_eq!(first.net(), Amount::from_minor(244_825));

    let second = &page.items[1];
    assert_eq!(second.id.as_str(), "stl_e2e_02");
    assert_eq!(second.recipient_type, SettlementRecipientType::Platform);
    assert_eq!(second.net(), Amount::from_minor(97_700));
}

/// Verifies `GET /settlements/<id>` returns `Settlement` and checked `amount - fee - tax` `net()`.
#[rocket::async_test]
async fn retrieves_settlement_with_checked_net_breakdown() {
    let (client, mock_server) = super::spawn_test_client().await;
    let payload = settlement_json(
        "stl_e2e_01",
        "entity",
        250_000,
        4_500,
        675,
        "STL-2026-11-01",
        42,
    );
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

/// Verifies `GET /settlements/<id>` reports `null` `net_amount` on minor-unit underflow.
#[rocket::async_test]
async fn retrieves_settlement_handles_checked_net_underflow() {
    let (client, mock_server) = super::spawn_test_client().await;
    let payload = settlement_json(
        "stl_e2e_underflow",
        "entity",
        1_000,
        800,
        300,
        "STL-2026-11-03",
        1,
    );
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
    let (client, mock_server) = super::spawn_test_client().await;
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
    let (client, mock_server) = super::spawn_test_client().await;
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
    let (client, mock_server) = super::spawn_test_client().await;
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
