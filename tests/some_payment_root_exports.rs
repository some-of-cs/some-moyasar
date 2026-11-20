#![cfg(feature = "core")]
#![forbid(unsafe_code)]

use some_moyasar::{
    MoyasarClient, MoyasarClientBuilder, MoyasarClientConfig, MoyasarError, PublishableKey,
    SecretKey as MoyasarSecretKey,
};

#[test]
fn some_payment_quick_start_root_exports_build_secret_client() -> Result<(), MoyasarError> {
    let key = MoyasarSecretKey::new("sk_test_some_payment_demo")?;
    let builder: MoyasarClientBuilder<MoyasarSecretKey> = MoyasarClient::builder(key);
    let client = builder.build()?;

    assert_eq!(client.base_url(), "https://api.moyasar.com");
    Ok(())
}

#[test]
fn crate_root_exports_support_publishable_key_and_config() -> Result<(), MoyasarError> {
    let pk = PublishableKey::new("pk_test_some_payment_demo")?;
    let config = MoyasarClientConfig::default();
    let client = MoyasarClient::builder(pk)
        .base_url(config.base_url)
        .build()?;

    assert_eq!(client.base_url(), "https://api.moyasar.com");
    Ok(())
}
