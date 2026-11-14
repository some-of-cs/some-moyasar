mod common;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde_json::json;
use some_moyasar::prelude::*;
use uuid::Uuid;

#[tokio::test]
async fn tokens_create_retrieve_delete_204_and_lifecycle_gates() {
    let (server, sk_client) = common::start_secret_client().await;
    let pk_client = MoyasarClient::builder(PublishableKey::new(common::TEST_PK).unwrap())
        .base_url(server.uri())
        .build()
        .unwrap();

    let token_id = TokenId::new("token_abc987654").unwrap();
    let token_json = json!({
        "id": token_id.as_str(),
        "status": "active",
        "number": "XXXX-XXXX-XXXX-1111",
        "company": "visa",
        "gateway_id": "gw_1",
        "created_at": "2026-11-14T10:00:00Z"
    });

    common::mount_json_endpoint(&server, "POST", "/v1/tokens", 201, token_json.clone()).await;
    let create_params =
        CreateTokenParams::new("MAHER MANSOUR", "4111111111111111", "12", "2029", "123")
            .callback_url("https://merchant.example.com/token-cb");
    let created = pk_client.tokens().create(&create_params).await.unwrap();
    assert!(is_token_chargeable(created.status));
    assert!(is_token_deletable(created.status));
    assert!(require_active_token(created.status).is_ok());

    common::mount_json_endpoint(
        &server,
        "GET",
        &format!("/v1/tokens/{}", token_id.as_str()),
        200,
        token_json,
    )
    .await;
    let retrieved = sk_client.tokens().retrieve(&token_id).await.unwrap();
    assert_eq!(retrieved.last_four(), Some("1111"));

    common::mount_raw_endpoint(
        &server,
        "DELETE",
        &format!("/v1/tokens/{}", token_id.as_str()),
        204,
        "",
    )
    .await;
    sk_client.tokens().delete(&token_id).await.unwrap();

    assert!(TokenId::new("invalid_prefix").is_err());
    assert!(TokenId::new("token_../bad").is_err());
    assert!(can_transition_token_status(
        TokenStatus::Initiated,
        TokenStatus::Active
    ));
    assert!(!can_transition_token_status(
        TokenStatus::Inactive,
        TokenStatus::Active
    ));
}

#[tokio::test]
async fn invoices_crud_bulk_cancel_put_and_sadad_expiry_sync() {
    let (server, client) = common::start_secret_client().await;
    let inv_id = Uuid::parse_str("880e8400-e29b-41d4-a716-446655440000").unwrap();
    let inv_json = json!({
        "id": inv_id,
        "status": "created",
        "amount": 50000,
        "currency": "SAR",
        "description": "SaaS Annual Plan",
        "logo_url": null,
        "amount_format": "500.00 SAR",
        "url": "https://moyasar.com/i/880e8400",
        "callback_url": "https://merchant.example.com/inv-cb",
        "expired_at": "2026-11-20T10:00:00Z",
        "back_url": null,
        "success_url": null,
        "created_at": "2026-11-14T10:00:00Z",
        "updated_at": "2026-11-14T10:00:00Z",
        "payments": []
    });

    common::mount_json_endpoint(&server, "POST", "/v1/invoices", 201, inv_json.clone()).await;
    let params =
        CreateInvoiceParams::new(Amount::from_minor(50000), Currency::SAR).description("SaaS Annual Plan");
    let created = client.invoices().create(&params).await.unwrap();
    assert_eq!(created.id, inv_id);
    assert!(validate_invoice_cancelable(&created).is_ok());

    common::mount_json_endpoint(
        &server,
        "POST",
        "/v1/invoices/bulk",
        201,
        json!([inv_json.clone()]),
    )
    .await;
    let bulk = client.invoices().create_bulk(vec![params]).await.unwrap();
    assert_eq!(bulk.len(), 1);

    let mut canceled_json = inv_json.clone();
    canceled_json["status"] = json!("canceled");
    common::mount_json_endpoint(
        &server,
        "PUT",
        &format!("/v1/invoices/{inv_id}/cancel"),
        200,
        canceled_json,
    )
    .await;
    let canceled = client.invoices().cancel(inv_id).await.unwrap();
    assert_eq!(canceled.status, InvoiceStatus::Canceled);
    assert!(validate_invoice_cancelable(&canceled).is_err());

    let now = Utc.with_ymd_and_hms(2026, 11, 14, 10, 0, 0).unwrap();
    let synced = sync_sadad_expiry(Some(now + ChronoDuration::days(10)), now);
    assert_eq!(synced, now + ChronoDuration::days(10));
    let capped = sync_sadad_expiry(Some(now + ChronoDuration::days(35)), now);
    assert_eq!(capped, now + ChronoDuration::days(MAX_SADAD_EXPIRY_DAYS));
}

#[tokio::test]
async fn card_auths_create_retrieve_mpi_bridge_and_cavv_redaction() {
    let (server, client) = common::start_secret_client().await;
    let ca_id = CardAuthId::new("ca_3ds_auth_001").unwrap();
    let ca_json = json!({
        "id": ca_id.as_str(),
        "status": "authenticated",
        "transaction_url": null,
        "result": {
            "eci": "05",
            "authentication_value": "AAABBBCCCDDDEEEFFF111",
            "ds_transaction_id": "ds-tx-01",
            "version": "2.2.0",
            "transaction_status": "Y",
            "auth_scheme": "visa",
            "is_frictionless": true
        }
    });

    common::mount_json_endpoint(&server, "POST", "/v1/card_auths", 201, ca_json).await;
    let create_params = CreateCardAuthParams::new(
        Amount::from_minor(15000),
        Currency::SAR,
        "https://merchant.example.com/3ds-cb",
        "MAHER MANSOUR",
        "4111111111111111".into(),
        "123".into(),
        12,
        2029,
    )
    .unwrap();
    let auth = client.card_auths().create(&create_params).await.unwrap();
    assert_eq!(auth.status, CardAuthStatus::Authenticated);

    let debug_str = format!("{auth:?}");
    assert!(!debug_str.contains("AAABBBCCCDDDEEEFFF111"));
    assert!(debug_str.contains("[REDACTED]"));

    let mpi = extract_mpi_from_auth(&auth).unwrap();
    assert_eq!(mpi.eci.as_deref(), Some("05"));
    let bridge = CardAuthBridgeData::from_mpi(mpi);
    assert_eq!(
        bridge.card_auth_data.as_ref().and_then(|d| d.eci.as_deref()),
        Some("05")
    );

    assert!(CardAuthId::new("bad_prefix").is_err());
    assert!(
        CreateCardAuthParams::new(
            Amount::from_minor(15000),
            Currency::SAR,
            "http://insecure.example.com/3ds-cb",
            "MAHER MANSOUR",
            "4111111111111111".into(),
            "123".into(),
            12,
            2029,
        )
        .is_err()
    );
}
