mod common;

use serde_json::json;
use some_moyasar::prelude::*;
use uuid::Uuid;

#[tokio::test]
async fn settlements_retrieve_and_url_redaction_workflow() {
    let (server, client) = common::start_secret_client().await;
    let set_id = SettlementId::new("set_001");

    let settlement_json = json!({
        "id": set_id.as_str(),
        "recipient_type": "entity",
        "currency": "SAR",
        "source_currency": "SAR",
        "amount": 95000,
        "fee": 4000,
        "tax": 600,
        "reference": "SET-REF-2026-01",
        "settlement_count": 12,
        "invoice_url": "https://s3.amazonaws.com/moyasar/invoice.pdf?X-Amz-Signature=secret1",
        "csv_list_url": "https://s3.amazonaws.com/moyasar/list.csv?X-Amz-Signature=secret2",
        "pdf_list_url": null,
        "created_at": "2026-11-14T10:00:00Z"
    });

    let set_path = format!("/v1/settlements/{}", set_id.as_str());
    common::mount_json_endpoint(&server, ("GET", &set_path), 200, settlement_json).await;

    let settlement = client.settlements().retrieve(&set_id).await.unwrap();
    assert_eq!(settlement.net(), Amount::from_minor(90400));
    assert_eq!(settlement.checked_net(), Some(Amount::from_minor(90400)));

    let debug_str = format!("{settlement:?}");
    assert!(!debug_str.contains("X-Amz-Signature=secret1"));
    assert!(debug_str.contains("[REDACTED]"));
}

#[tokio::test]
async fn settlements_lines_and_merchant_balance_workflow() {
    let (server, client) = common::start_secret_client().await;
    let set_id = SettlementId::new("set_001");
    let rec_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();

    let lines_path = format!("/v1/settlements/{}/lines", set_id.as_str());
    common::mount_json_endpoint(
        &server,
        ("GET", &lines_path),
        200,
        common::paginated_envelope(
            "lines",
            vec![json!({
                "payment_id": rec_uuid,
                "type": "Payment",
                "currency": "SAR",
                "amount": 10000,
                "fee": 200,
                "tax": 30,
                "settled_at": "2026-11-14T09:00:00Z"
            })],
            1,
            None,
        ),
    )
    .await;
    let lines = client
        .settlements()
        .list_lines(&set_id, &ListSettlementLinesParams::new())
        .await
        .unwrap();
    assert_eq!(lines.items.len(), 1);
    assert_eq!(lines.items[0].line_type.as_deref(), Some("Payment"));

    common::mount_json_endpoint(
        &server,
        ("GET", "/v1/settlements/balance"),
        200,
        json!({
            "balances": [
                {
                    "currency": "SAR",
                    "available": 500000,
                    "pending": 120000,
                    "reserved": 0
                }
            ]
        }),
    )
    .await;
    let balance = client.settlements().fetch_balance().await.unwrap();
    assert_eq!(balance.available, Amount::from_minor(500000));
    assert_eq!(balance.pending, Amount::from_minor(120000));
}

#[tokio::test]
async fn transfers_and_internal_transactions_read_and_negative_guards() {
    let (server, client) = common::start_secret_client().await;
    let tr_id = TransferId::new("tr_001");
    let rec_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();

    let tr_path = format!("/v1/transfers/{}", tr_id.as_str());
    common::mount_json_endpoint(
        &server,
        ("GET", &tr_path),
        200,
        json!({
            "id": tr_id.as_str(),
            "recipient_type": "platform",
            "recipient_id": rec_uuid,
            "currency": "SAR",
            "amount": 20000,
            "fee": 100,
            "tax": 15,
            "reference": "TR-REF-01",
            "created_at": "2026-11-14T10:00:00Z"
        }),
    )
    .await;
    let transfer = client.transfers().retrieve(&tr_id).await.unwrap();
    assert_eq!(
        transfer.recipient_type,
        Some(SettlementRecipientType::Platform)
    );

    common::mount_json_endpoint(
        &server,
        ("GET", "/v1/internal_transactions"),
        200,
        common::paginated_envelope(
            "internal_transactions",
            vec![json!({
                "id": "itx_001",
                "recipient_type": "entity",
                "recipient_id": rec_uuid,
                "amount": 1500,
                "currency": "SAR",
                "transfer_id": "tr_001",
                "settled_at": "2026-11-14T10:00:00Z",
                "metadata": { "secret_note": "private" },
                "created_at": "2026-11-14T10:00:00Z",
                "updated_at": "2026-11-14T10:00:00Z"
            })],
            1,
            None,
        ),
    )
    .await;
    let itx_list = client
        .internal_transactions()
        .list(&ListInternalTransactionsParams::new().currency(Currency::SAR))
        .await
        .unwrap();
    assert_eq!(itx_list.items.len(), 1);
    assert!(itx_list.items[0].is_settled());
    assert!(!format!("{:?}", itx_list.items[0]).contains("private"));

    assert!(SettlementId::new("set_../bad").validate().is_err());
    assert!(TransferId::new("tr_../bad").validate().is_err());
}
