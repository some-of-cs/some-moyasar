//! E2E Rocket integration tests for transfers, internal transactions, sources, and Apple Pay.

use rocket::http::{ContentType, Status};
use rocket::local::asynchronous::{Client, LocalResponse};
use serde::Serialize;
use serde_json::{Value, json};
use some_moyasar::prelude::{
    Amount, ApplePayMerchantSession, Currency, FIXTURE_TIMESTAMP, SourceIssuerCardType, Transfer,
    TransferId,
};

use super::routes_ledger_sources::*;
use super::{MockResponseSpec, mount_json_endpoint, spawn_test_client};

/// Dispatches a JSON `POST` request to `uri` through the tracked Rocket client.
async fn post_json<'a, T: Serialize>(
    client: &'a Client,
    uri: &'a str,
    body: &T,
) -> LocalResponse<'a> {
    let payload = serde_json::to_string(body).expect("request DTO should serialize");
    let req = client.post(uri).header(ContentType::JSON).body(payload);
    req.dispatch().await
}

/// Builds a deterministic pagination metadata JSON object for mock list responses.
fn meta_json() -> Value {
    json!({
        "current_page": 1,
        "next_page": null,
        "prev_page": null,
        "total_pages": 1,
        "total_count": 1,
    })
}

/// Builds a deterministic [`Transfer`] JSON fixture for the given transfer identifier.
fn transfer_json(id: &str) -> Value {
    json!({
        "id": id,
        "recipient_type": "entity",
        "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
        "amount": 75_000,
        "fee": 1_500,
        "tax": 225,
        "currency": "SAR",
        "reference": "TRF-ROCKET-2026-11",
        "created_at": FIXTURE_TIMESTAMP,
    })
}

/// Builds a deterministic `InternalTransaction` JSON fixture for the given identifier.
fn internal_transaction_json(id: &str) -> Value {
    json!({
        "id": id,
        "recipient_type": "entity",
        "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
        "amount": 45_000,
        "currency": "SAR",
        "transfer_id": "trf_01",
        "settled_at": FIXTURE_TIMESTAMP,
        "created_at": FIXTURE_TIMESTAMP,
        "updated_at": FIXTURE_TIMESTAMP,
        "metadata": null,
    })
}

/// Builds a deterministic `IssuerMetadata` JSON fixture for the given BIN prefix.
fn issuer_json(first_digits: &str) -> Value {
    json!({
        "issuer_name": "AL RAJHI BANK",
        "issuer_country": "SA",
        "issuer_card_type": "debit",
        "issuer_card_category": "classic",
        "first_digits": first_digits,
        "last_digits": null,
    })
}

/// Builds a deterministic [`ApplePayMerchantSession`] JSON fixture in camelCase wire format.
fn apple_pay_session_json() -> Value {
    json!({
        "epochTimestamp": 1_700_000_000_000_u64,
        "expiresAt": 1_700_003_600_000_u64,
        "merchantSessionIdentifier": "SSH_ROCKET_01",
        "nonce": "abcdef12",
        "merchantIdentifier": "merchant.com.some_cs",
        "domainName": "checkout.example.com",
        "displayName": "Some Rocket Store",
        "signature": "MIAGCSqGSIb3DQEHAqCAMIACAQEx",
    })
}

/// Builds an [`ApplePayInitiateRequest`] for the given validation URL.
fn apple_pay_request(validation_url: &str) -> ApplePayInitiateRequest {
    ApplePayInitiateRequest {
        validation_url: validation_url.to_owned(),
        display_name: "Some Rocket Store".to_owned(),
        domain_name: "checkout.example.com".to_owned(),
        publishable_api_key: "pk_test_rocket_default_key".to_owned(),
    }
}

/// Verifies `GET /transfers` and `GET /transfers/<id>` list and retrieve transfers.
#[tokio::test]
async fn lists_and_retrieves_transfers() {
    let (server, client) = spawn_test_client().await;
    let list_body = json!({
        "transfers": [transfer_json("trf_01")],
        "meta": meta_json(),
    });
    mount_json_endpoint(
        &server,
        "GET",
        "/v1/transfers",
        MockResponseSpec {
            status: 200,
            body: list_body,
        },
    )
    .await;
    mount_json_endpoint(
        &server,
        "GET",
        "/v1/transfers/trf_01",
        MockResponseSpec {
            status: 200,
            body: transfer_json("trf_01"),
        },
    )
    .await;

    let list_resp = client.get("/transfers").dispatch().await;
    assert_eq!(list_resp.status(), Status::Ok);
    let listed: TransfersListResponse = list_resp.into_json().await.expect("transfers list JSON");
    assert_eq!(listed.total_count, Some(1));
    assert_eq!(listed.items[0].id, TransferId::new("trf_01"));

    let get_resp = client.get("/transfers/trf_01").dispatch().await;
    assert_eq!(get_resp.status(), Status::Ok);
    let fetched: Transfer = get_resp.into_json().await.expect("transfer JSON");
    let actual = (fetched.id, fetched.amount);
    let expected = (TransferId::new("trf_01"), Amount::from_minor(75_000));
    assert_eq!(actual, expected);
}

/// Verifies `GET /internal_transactions` lists internal ledger transactions with pagination.
#[tokio::test]
async fn lists_internal_transactions() {
    let (server, client) = spawn_test_client().await;
    let list_body = json!({
        "internal_transactions": [internal_transaction_json("itxn_01")],
        "meta": meta_json(),
    });
    mount_json_endpoint(
        &server,
        "GET",
        "/v1/internal_transactions",
        MockResponseSpec {
            status: 200,
            body: list_body,
        },
    )
    .await;

    let response = client.get("/internal_transactions").dispatch().await;
    assert_eq!(response.status(), Status::Ok);
    let listed: InternalTransactionsListResponse = response
        .into_json()
        .await
        .expect("internal transactions JSON");

    assert_eq!(listed.total_count, Some(1));
    assert_eq!(listed.current_page, Some(1));
    let item = &listed.items[0];
    assert_eq!(item.id.as_str(), "itxn_01");
    let actual = (item.amount, item.currency, item.is_settled());
    let expected = (Amount::from_minor(45_000), Currency::SAR, true);
    assert_eq!(actual, expected);
}

/// Verifies `POST /sources/issuer` looks up card issuer metadata and rejects invalid BINs.
#[tokio::test]
async fn looks_up_card_issuer_and_rejects_invalid_bin() {
    let (server, client) = spawn_test_client().await;
    mount_json_endpoint(
        &server,
        "POST",
        "/v1/source/issuer",
        MockResponseSpec {
            status: 200,
            body: issuer_json("420132"),
        },
    )
    .await;

    let valid_req = IssuerLookupRequest {
        first_digits: "420132".to_owned(),
    };
    let ok_resp = post_json(&client, "/sources/issuer", &valid_req).await;
    assert_eq!(ok_resp.status(), Status::Ok);
    let issuer: IssuerResponse = ok_resp.into_json().await.expect("issuer JSON");
    assert_eq!(issuer.issuer_name.as_deref(), Some("AL RAJHI BANK"));
    let actual = (
        issuer.issuer_country.as_deref(),
        issuer.issuer_card_type,
        issuer.first_digits.as_str(),
    );
    let expected = (Some("SA"), SourceIssuerCardType::Debit, "420132");
    assert_eq!(actual, expected);

    let invalid_req = IssuerLookupRequest {
        first_digits: "42013a".to_owned(),
    };
    let bad_resp = post_json(&client, "/sources/issuer", &invalid_req).await;
    assert_eq!(bad_resp.status(), Status::BadRequest);
    let err_body: Value = bad_resp.into_json().await.expect("error JSON");
    assert_eq!(err_body["error_type"], "invalid_request_error");
}

/// Verifies `POST /apple_pay/session` initiates a session and rejects `http://` URLs.
#[tokio::test]
async fn initiates_apple_pay_session_and_rejects_http_validation_url() {
    let (server, client) = spawn_test_client().await;
    mount_json_endpoint(
        &server,
        "POST",
        "/v1/applepay/initiate",
        MockResponseSpec {
            status: 200,
            body: apple_pay_session_json(),
        },
    )
    .await;

    let valid_req =
        apple_pay_request("https://apple-pay-gateway.apple.com/paymentservices/paymentSession");
    let ok_resp = post_json(&client, "/apple_pay/session", &valid_req).await;
    assert_eq!(ok_resp.status(), Status::Ok);
    let session: ApplePayMerchantSession = ok_resp.into_json().await.expect("apple pay JSON");
    assert_eq!(session.merchant_session_identifier, "SSH_ROCKET_01");
    let actual = (session.domain_name.as_str(), session.display_name.as_str());
    let expected = ("checkout.example.com", "Some Rocket Store");
    assert_eq!(actual, expected);

    let invalid_req =
        apple_pay_request("http://apple-pay-gateway.apple.com/paymentservices/paymentSession");
    let bad_resp = post_json(&client, "/apple_pay/session", &invalid_req).await;
    assert_eq!(bad_resp.status(), Status::BadRequest);
    let err_body: Value = bad_resp.into_json().await.expect("error JSON");
    assert_eq!(err_body["error_type"], "invalid_request_error");
}
