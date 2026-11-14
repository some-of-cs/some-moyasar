mod common;

use std::collections::BTreeMap;

use serde_json::json;
use some_moyasar::prelude::*;

#[tokio::test]
async fn payout_accounts_create_and_retrieve_lifecycle() {
    let (server, client) = common::start_secret_client().await;
    let acct_id = PayoutAccountId::new("pa_acct_001").unwrap();

    let acct_json = json!({
        "id": acct_id.as_str(),
        "account_type": "bank",
        "currency": "SAR",
        "properties": {
            "iban": "SA4420000001234567891234",
            "name": "Some CS LLC",
            "bank_name": "Riyad Bank"
        },
        "created_at": "2026-11-14T10:00:00Z"
    });
    common::mount_json_endpoint(
        &server,
        ("POST", "/v1/payout_accounts"),
        201,
        acct_json.clone(),
    )
    .await;
    let acct_get_path = format!("/v1/payout_accounts/{}", acct_id.as_str());
    common::mount_json_endpoint(&server, ("GET", &acct_get_path), 200, acct_json).await;

    let acct_params = CreatePayoutAccountParams::new(
        PayoutAccountType::Bank,
        PayoutAccountProperties {
            iban: Some("SA4420000001234567891234".to_string()),
            name: Some("Some CS LLC".to_string()),
            bank_name: Some("Riyad Bank".to_string()),
            extra: BTreeMap::new(),
        },
        "super_secret_val".into(),
    )
    .unwrap();
    assert!(!format!("{acct_params:?}").contains("super_secret_val"));

    let created_acct = client.payout_accounts().create(&acct_params).await.unwrap();
    assert_eq!(created_acct.id, acct_id);
    let fetched_acct = client.payout_accounts().retrieve(&acct_id).await.unwrap();
    assert_eq!(fetched_acct.currency, Currency::SAR);
}

#[tokio::test]
async fn payouts_single_and_bulk_creation_lifecycle() {
    let (server, client) = common::start_secret_client().await;
    let acct_id = PayoutAccountId::new("pa_acct_001").unwrap();
    let payout_id = PayoutId::new("po_payout_001").unwrap();

    let payout_json = json!({
        "id": payout_id.as_str(),
        "channel": "sarie",
        "status": "paid",
        "amount": 100000,
        "message": "Success",
        "failure_reason": null
    });
    common::mount_json_endpoint(&server, ("POST", "/v1/payouts"), 201, payout_json.clone()).await;

    let bank_dest = PayoutDestination::Bank(BankDestination {
        iban: "SA4420000001234567891234".to_string(),
        name: "Some CS LLC".to_string(),
        mobile: "+966551234567".to_string(),
        country: "SA".to_string(),
        city: "Riyadh".to_string(),
    });
    let payout_params = CreatePayoutParams {
        source_id: acct_id.clone(),
        sequence_number: Some("1234567890123456".to_string()),
        amount: 100000,
        purpose: PayoutPurpose::PaymentToMerchant,
        destination: bank_dest,
        comment: Some("Vendor settlement".to_string()),
        metadata: None,
    };

    let created_payout = client.payouts().create(&payout_params).await.unwrap();
    assert_eq!(created_payout.status, PayoutStatus::Paid);
    assert_eq!(created_payout.channel, PayoutChannel::Sarie);

    common::mount_json_endpoint(
        &server,
        ("POST", "/v1/payouts/bulk"),
        201,
        json!({ "payouts": [payout_json] }),
    )
    .await;
    let entry = BulkPayoutEntry::new(
        100000,
        PayoutPurpose::PayrollBenefits,
        PayoutDestination::Wallet(WalletDestination {
            mobile: "0551234567".to_string(),
        }),
    )
    .sequence_number("1234567890123456");
    let bulk_params = CreateBulkPayoutParams::new(acct_id, vec![entry]);
    let bulk_res = client.payouts().create_bulk(&bulk_params).await.unwrap();
    assert_eq!(bulk_res.payouts.len(), 1);
}

#[test]
fn payouts_negative_validations_enforce_sa_iban_16_digit_seq_and_20_purposes() {
    assert_eq!(KNOWN_PURPOSES.len(), 20);
    for &(raw, expected) in KNOWN_PURPOSES {
        let parsed: PayoutPurpose = serde_json::from_value(json!(raw)).unwrap();
        assert_eq!(parsed, expected);
        assert_eq!(parsed.as_str(), raw);
    }
    let bad_purpose: Result<PayoutPurpose, _> =
        serde_json::from_value(json!("unsupported_purpose"));
    assert!(bad_purpose.is_err());

    assert!(matches!(
        validate_sa_iban("US1234567890123456789012"),
        Err(MoyasarError::InvalidIban)
    ));
    assert!(matches!(
        validate_sa_iban("SA12345"),
        Err(MoyasarError::InvalidIban)
    ));
    assert!(matches!(
        validate_sa_iban("SA442000000123456789123A"),
        Err(MoyasarError::InvalidIban)
    ));

    assert!(validate_sequence_number("1234567890123456").is_ok());
    assert!(validate_sequence_number("12345").is_err());
    assert!(validate_sequence_number("123456789012345a").is_err());

    let dest = PayoutDestination::Wallet(WalletDestination {
        mobile: "0551234567".to_string(),
    });
    let zero_entry = BulkPayoutEntry::new(0, PayoutPurpose::Personal, dest);
    assert!(matches!(
        zero_entry.validate(),
        Err(MoyasarError::InvalidPayoutAmount)
    ));
    assert!(PayoutAccountId::new("pa_../bad").is_err());
    assert!(PayoutId::new("po_../bad").is_err());
}
