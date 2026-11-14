mod common;

use std::fs;
use std::path::Path;

use serde_json::json;
use some_moyasar::prelude::*;

/// RAII guard that creates a temporary `.env.integration_test` file and removes it on drop.
struct TempDotenvFile {
    path: &'static str,
}

impl TempDotenvFile {
    fn create(path: &'static str, contents: &str) -> Self {
        fs::write(path, contents).expect("write temp .env.integration_test");
        Self { path }
    }
}

impl Drop for TempDotenvFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(Path::new(self.path));
    }
}

#[tokio::test]
async fn dotenv_secret_and_publishable_keys_load_and_authenticate_requests() {
    let _guard = TempDotenvFile::create(
        ".env.integration_test",
        "MOYASAR_DOTENV_IT_SK="sk_test_from_dotenv_file_777"
MOYASAR_DOTENV_IT_PK='pk_test_from_dotenv_file_888'
",
    );

    let sk = SecretKey::new("$MOYASAR_DOTENV_IT_SK").expect("resolve SK from .env.integration_test");
    let pk = PublishableKey::new("${MOYASAR_DOTENV_IT_PK}")
        .expect("resolve PK from .env.integration_test");
    assert_eq!(sk.mode(), AuthMode::Test);
    assert_eq!(pk.mode(), AuthMode::Test);
    assert_eq!(sk.expose_secret(), "sk_test_from_dotenv_file_777");
    assert_eq!(pk.expose_secret(), "pk_test_from_dotenv_file_888");

    let (server, _) = common::start_secret_client().await;
    let dotenv_client = MoyasarClient::builder(sk)
        .base_url(server.uri())
        .build()
        .unwrap();

    common::mount_json_endpoint(
        &server,
        "POST",
        "/v1/source/issuer",
        200,
        json!({
            "issuer_name": "Saudi National Bank",
            "issuer_country": "SA",
            "issuer_card_type": "debit",
            "issuer_card_category": "CLASSIC",
            "first_digits": "468540",
            "last_digits": null
        }),
    )
    .await;

    let bin = BinLookupParams::new("468540").unwrap();
    let issuer = dotenv_client.sources().get_issuer(&bin).await.unwrap();
    assert_eq!(issuer.issuer_name.as_deref(), Some("Saudi National Bank"));
}

#[tokio::test]
async fn dotenv_real_secret_key_live_api_verification() {
    let Some(client) = common::try_dotenv_secret_client() else {
        return;
    };

    if !common::is_live_dotenv_test_enabled() {
        assert!(!client.base_url().is_empty());
        return;
    }

    let payments = client
        .payments()
        .list(&ListParams::new().page(1))
        .await
        .expect("GET /v1/payments with real .env secret key should succeed");
    assert!(payments.meta.current_page >= 1);

    let invoices = client
        .invoices()
        .list(&ListInvoicesParams::new().page(1))
        .await
        .expect("GET /v1/invoices with real .env secret key should succeed");
    assert!(invoices.meta.current_page >= 1);

    let webhooks = client
        .webhooks()
        .available_events()
        .await
        .expect("GET /v1/webhooks/available_events with real .env secret key should succeed");
    assert!(!webhooks.is_empty());

    let nonexistent = PaymentId::new("pay_00000000-0000-0000-0000-000000000000").unwrap();
    let err = client
        .payments()
        .retrieve(&nonexistent)
        .await
        .expect_err("non-existent payment ID must return 404/400 API error");
    assert!(matches!(err, MoyasarError::Api { .. }));
}
