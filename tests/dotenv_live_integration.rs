mod common;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;
use some_moyasar::prelude::*;

static TEMP_ENV_SEQ: AtomicU64 = AtomicU64::new(1);

/// RAII guard that creates a unique temporary `.env.it_*` file via `create_new(true)`
/// and removes it on drop without ever overwriting pre-existing files.
struct TempDotenvFile {
    path: PathBuf,
}

impl TempDotenvFile {
    fn create(contents: &str) -> Self {
        let seq = TEMP_ENV_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(format!(".env.it_{}_{}", std::process::id(), seq));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .expect("create unique temporary .env file");
        file.write_all(contents.as_bytes())
            .expect("write temporary .env contents");
        Self { path }
    }
}

impl Drop for TempDotenvFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[tokio::test]
async fn dotenv_secret_and_publishable_keys_load_and_authenticate_requests() {
    let dotenv_body = "MOYASAR_DOTENV_IT_SK=sk_test_from_dotenv_file_777\nMOYASAR_DOTENV_IT_PK=pk_test_from_dotenv_file_888\n";
    let _guard = TempDotenvFile::create(dotenv_body);

    let sk = SecretKey::new("$MOYASAR_DOTENV_IT_SK").expect("resolve SK from temp .env file");
    let pk =
        PublishableKey::new("${MOYASAR_DOTENV_IT_PK}").expect("resolve PK from temp .env file");
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
        ("POST", "/v1/source/issuer"),
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
