//! E2E tests for `SourcesService`, `ApplePayService`, and `WebhooksService`.
#![forbid(unsafe_code)]

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use some_moyasar::test_utils::fixtures::FIXTURE_TIMESTAMP;
use tower::ServiceExt;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::spawn_test_app;

/// Sends an HTTP request through the Axum router via `tower::ServiceExt::oneshot` and parses JSON.
async fn dispatch_json(
    app: Router,
    http_method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(http_method).uri(uri);
    let payload = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            let bytes = serde_json::to_vec(&value).expect("serialize request json");
            Body::from(bytes)
        }
        None => Body::empty(),
    };
    let req = builder.body(payload).expect("build HTTP request");
    let resp = app.oneshot(req).await.expect("oneshot request succeeds");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let json = serde_json::from_slice(&bytes).expect("parse response json");
    (status, json)
}

/// Sends an HTTP request with an empty response body through the Axum router.
async fn dispatch_status(app: Router, http_method: Method, uri: &str) -> StatusCode {
    let req = Request::builder()
        .method(http_method)
        .uri(uri)
        .body(Body::empty())
        .expect("build HTTP request");
    let resp = app.oneshot(req).await.expect("oneshot request succeeds");
    resp.status()
}

/// Builds a deterministic Moyasar `WebhookEndpoint` JSON fixture.
fn webhook_endpoint_json(id: &str) -> Value {
    json!({
        "id": id,
        "http_method": "post",
        "url": "https://merchant.example/webhooks",
        "shared_secret": "whsec_test_axum_01",
        "events": ["payment_paid", "payment_failed"],
        "created_at": FIXTURE_TIMESTAMP
    })
}

/// Verifies `POST /sources/issuer` looks up card BIN/IIN issuer metadata via `SourcesService`.
#[tokio::test]
async fn e2e_sources_get_issuer_lookup() {
    let (mock, router) = spawn_test_app().await;
    let upstream = json!({
        "issuer_name": "AL RAJHI BANK",
        "issuer_country": "SA",
        "issuer_card_type": "debit",
        "issuer_card_category": "classic",
        "first_digits": "420132",
        "last_digits": null
    });
    Mock::given(method("POST"))
        .and(path("/v1/source/issuer"))
        .and(body_partial_json(json!({ "first_digits": "420132" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(upstream))
        .expect(1)
        .mount(mock.server())
        .await;
    let (status, body) = dispatch_json(
        router,
        Method::POST,
        "/sources/issuer",
        Some(json!({ "first_digits": "420132" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["issuer_name"], "AL RAJHI BANK");
    assert_eq!(body["issuer_country"], "SA");
    assert_eq!(body["issuer_card_type"], "debit");
    assert_eq!(body["first_digits"], "420132");
}

/// Verifies `POST /apple_pay/session` initiates an Apple Pay Web Merchant session via `ApplePayService`.
#[tokio::test]
async fn e2e_apple_pay_initiate_session() {
    let (mock, router) = spawn_test_app().await;
    let upstream = json!({
        "epochTimestamp": 1700000000000_u64,
        "expiresAt": 1700003600000_u64,
        "merchantSessionIdentifier": "SSH_AXUM_01",
        "nonce": "abcdef12",
        "merchantIdentifier": "merchant.com.some_cs",
        "domainName": "checkout.example.com",
        "displayName": "Some Store",
        "signature": "MIAGCSqGSIb3DQEHAqCAMIACAQEx"
    });
    Mock::given(method("POST"))
        .and(path("/v1/applepay/initiate"))
        .and(body_partial_json(json!({
            "domain_name": "checkout.example.com",
            "display_name": "Some Store"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(upstream))
        .expect(1)
        .mount(mock.server())
        .await;
    let req_body = json!({
        "validation_url": "https://apple-pay-gateway.apple.com/paymentservices/paymentSession",
        "display_name": "Some Store",
        "domain_name": "checkout.example.com",
        "publishable_api_key": "pk_test_axum_key"
    });
    let (status, body) =
        dispatch_json(router, Method::POST, "/apple_pay/session", Some(req_body)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["merchantSessionIdentifier"], "SSH_AXUM_01");
    assert_eq!(body["domainName"], "checkout.example.com");
    assert_eq!(body["displayName"], "Some Store");
}

/// Verifies `POST /webhooks_api` registers a webhook endpoint and redacts `shared_secret`.
#[tokio::test]
async fn e2e_webhooks_api_create_endpoint() {
    let (mock, router) = spawn_test_app().await;
    let webhook_id = "550e8400-e29b-41d4-a716-446655440000";
    let upstream = webhook_endpoint_json(webhook_id);
    Mock::given(method("POST"))
        .and(path("/v1/webhooks"))
        .and(body_partial_json(json!({
            "url": "https://merchant.example/webhooks",
            "http_method": "post"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(upstream))
        .expect(1)
        .mount(mock.server())
        .await;
    let req_body = json!({
        "url": "https://merchant.example/webhooks",
        "shared_secret": "whsec_test_axum_01",
        "http_method": "post",
        "events": ["payment_paid", "payment_failed"]
    });
    let (status, body) = dispatch_json(router, Method::POST, "/webhooks_api", Some(req_body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["id"], webhook_id);
    assert_eq!(body["url"], "https://merchant.example/webhooks");
    assert_eq!(body["http_method"], "post");
    assert!(body.get("shared_secret").is_none());
}

/// Verifies `GET /webhooks_api` lists registered webhook endpoints with pagination metadata.
#[tokio::test]
async fn e2e_webhooks_api_list_endpoints() {
    let (mock, router) = spawn_test_app().await;
    let webhook_id = "550e8400-e29b-41d4-a716-446655440000";
    let upstream = json!({
        "webhooks": [webhook_endpoint_json(webhook_id)],
        "meta": {
            "current_page": 1,
            "next_page": null,
            "prev_page": null,
            "total_pages": 1,
            "total_count": 1
        }
    });
    Mock::given(method("GET"))
        .and(path("/v1/webhooks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(upstream))
        .expect(1)
        .mount(mock.server())
        .await;
    let (status, body) = dispatch_json(router, Method::GET, "/webhooks_api", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["webhooks"][0]["id"], webhook_id);
    assert_eq!(body["meta"]["current_page"], 1);
    assert_eq!(body["meta"]["total_count"], 1);
}

/// Verifies `GET /webhooks_api/events` lists supported Moyasar webhook event types.
#[tokio::test]
async fn e2e_webhooks_api_available_events() {
    let (mock, router) = spawn_test_app().await;
    let upstream = json!({
        "available_events": ["payment_paid", "payment_failed", "payment_refunded"]
    });
    Mock::given(method("GET"))
        .and(path("/v1/webhooks/available_events"))
        .respond_with(ResponseTemplate::new(200).set_body_json(upstream))
        .expect(1)
        .mount(mock.server())
        .await;
    let (status, body) = dispatch_json(router, Method::GET, "/webhooks_api/events", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!(["payment_paid", "payment_failed", "payment_refunded"])
    );
}

/// Verifies `DELETE /webhooks_api/{id}` deletes a webhook endpoint and returns `204 No Content`.
#[tokio::test]
async fn e2e_webhooks_api_delete_endpoint() {
    let (mock, router) = spawn_test_app().await;
    let webhook_id = "550e8400-e29b-41d4-a716-446655440000";
    Mock::given(method("DELETE"))
        .and(path("/v1/webhooks/550e8400-e29b-41d4-a716-446655440000"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(mock.server())
        .await;
    let status = dispatch_status(
        router,
        Method::DELETE,
        &format!("/webhooks_api/{webhook_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}
