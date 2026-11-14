mod common;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures::TryStreamExt;
use serde_json::json;
use some_moyasar::prelude::*;

#[test]
fn auth_keys_enforce_prefixes_suffix_and_redacted_debug() {
    let sk_live = SecretKey::new(common::LIVE_SK).unwrap();
    assert_eq!(sk_live.mode(), AuthMode::Live);
    assert_eq!(sk_live.expose_secret(), common::LIVE_SK);
    assert_eq!(format!("{sk_live:?}"), "SecretKey([REDACTED])");

    let pk_test = PublishableKey::new(common::TEST_PK).unwrap();
    assert_eq!(pk_test.mode(), AuthMode::Test);
    assert_eq!(format!("{pk_test:?}"), "PublishableKey([REDACTED])");

    for invalid_sk in ["", "sk_test_", "pk_test_12345"] {
        assert!(matches!(
            SecretKey::new(invalid_sk),
            Err(MoyasarError::InvalidKey)
        ));
    }
    for invalid_pk in ["pk_live_", "sk_live_12345"] {
        assert!(matches!(
            PublishableKey::new(invalid_pk),
            Err(MoyasarError::InvalidKey)
        ));
    }
    let mismatch = MoyasarError::KeyTypeMismatch;
    assert!(!mismatch.is_retryable());
}

#[test]
fn env_keys_reject_missing_environment_variables() {
    let missing = SecretKey::new("$MOYASAR_NONEXISTENT_INTEGRATION_VAR_999");
    assert!(matches!(missing, Err(MoyasarError::InvalidKey)));
}

#[test]
fn client_builder_enforces_https_and_loopback_test_mode_rules() {
    let test_sk = SecretKey::new(common::TEST_SK).unwrap();
    let live_sk = SecretKey::new(common::LIVE_SK).unwrap();

    for loopback in [
        "http://127.0.0.1:9999/",
        "http://localhost:9999",
        "http://[::1]:9999",
    ] {
        assert!(
            MoyasarClient::builder(test_sk.clone())
                .base_url(loopback)
                .build()
                .is_ok()
        );
    }

    for invalid_url in [
        "http://api.moyasar.com",
        "https://api.moyasar.com?foo=bar",
        "https://api.moyasar.com#frag",
    ] {
        let res = MoyasarClient::builder(test_sk.clone())
            .base_url(invalid_url)
            .build();
        assert!(matches!(res, Err(MoyasarError::InvalidBaseUrl)));
    }

    let live_loopback = MoyasarClient::builder(live_sk)
        .base_url("http://127.0.0.1:9999")
        .build();
    assert!(matches!(live_loopback, Err(MoyasarError::InvalidBaseUrl)));
}

#[test]
fn basic_auth_and_default_headers_follow_rfc7617_and_version_pinning() {
    let sk = SecretKey::new("sk_test_abc123").unwrap();
    let auth_header = encode_basic_auth(&sk).unwrap();
    let expected = format!("Basic {}", STANDARD.encode("sk_test_abc123:"));
    assert_eq!(auth_header.to_str().unwrap(), expected);

    let headers = build_default_headers(&sk).unwrap();
    assert_eq!(headers.get(MOYASAR_VERSION_HEADER).unwrap(), API_VERSION);
    assert_eq!(headers.get("authorization").unwrap(), &auth_header);
}

#[test]
fn idempotency_random_and_deterministic_uuid_v5_policies_behave_consistently() {
    let mut payload = json!({ "amount": 1000 });
    let generated = inject_given_id(&mut payload, &GivenIdPolicy::Random)
        .unwrap()
        .unwrap();
    assert_eq!(payload["given_id"], generated.to_string());

    let second = inject_given_id(&mut payload, &GivenIdPolicy::Random)
        .unwrap()
        .unwrap();
    assert_eq!(generated, second);

    let sub_a1 = derive_subscription_given_id("sub_100", 1);
    let sub_a1_repeat = derive_subscription_given_id("sub_100", 1);
    let sub_a2 = derive_subscription_given_id("sub_100", 2);
    assert_eq!(sub_a1, sub_a1_repeat);
    assert_ne!(sub_a1, sub_a2);

    let mut non_obj = json!(["not", "an", "object"]);
    assert_eq!(
        inject_given_id(&mut non_obj, &GivenIdPolicy::Random).unwrap(),
        None
    );

    let mut bad_uuid = json!({ "given_id": "not-a-uuid" });
    assert!(inject_given_id(&mut bad_uuid, &GivenIdPolicy::Random).is_err());
}

#[test]
fn metadata_enforces_30_keys_40_char_key_500_char_value_unset_and_remove() {
    let mut meta = Metadata::new();
    for idx in 0..METADATA_MAX_KEYS {
        let key = format!("key_{idx}");
        meta.insert(&key, "val").unwrap();
    }
    assert_eq!(meta.len(), 30);

    meta.insert("key_0", "updated").unwrap();
    assert_eq!(meta.get("key_0"), Some("updated"));

    assert!(matches!(
        meta.insert("key_31", "overflow"),
        Err(MoyasarError::MetadataTooManyKeys)
    ));

    let mut fresh = Metadata::new();
    let long_key = "k".repeat(41);
    let long_val = "v".repeat(501);
    assert!(matches!(
        fresh.insert(&long_key, "v"),
        Err(MoyasarError::MetadataKeyTooLong)
    ));
    assert!(matches!(
        fresh.insert("valid_key", &long_val),
        Err(MoyasarError::MetadataValueTooLong)
    ));

    fresh.unset("remove_me").unwrap();
    let serialized = serde_json::to_value(&fresh).unwrap();
    assert_eq!(serialized["remove_me"], "");
    assert_eq!(format!("{fresh:?}"), "Metadata { keys_count: 1 }");
    assert_eq!(fresh.remove("remove_me"), Some(MetadataValue::Unset));
    assert!(fresh.is_empty());
}

#[test]
fn currency_exponents_and_minor_major_amount_conversions_are_exact() {
    assert_eq!(Currency::SAR.exponent(), 2);
    assert_eq!(Currency::KWD.exponent(), 3);
    assert_eq!(Currency::JPY.exponent(), 0);

    assert_eq!(
        Amount::from_major(15, Currency::SAR),
        Amount::from_minor(1500)
    );
    assert_eq!(
        Amount::from_major(15, Currency::KWD),
        Amount::from_minor(15000)
    );
    assert_eq!(
        Amount::from_major(15, Currency::JPY),
        Amount::from_minor(15)
    );
}

#[test]
fn error_envelope_parses_401_404_422_array_and_map_errors_plus_response_codes() {
    let auth_err = MoyasarError::from_api_response(
        401,
        r#"{"type":"authentication_error","message":"Invalid API key"}"#,
    );
    assert!(matches!(
        auth_err,
        MoyasarError::Api {
            error_type: ApiErrorType::AuthenticationError,
            ..
        }
    ));

    let not_found = MoyasarError::from_api_response(
        404,
        r#"{"type":"record_not_found","message":"Payment not found"}"#,
    );
    assert!(matches!(
        not_found,
        MoyasarError::Api {
            error_type: ApiErrorType::RecordNotFound,
            ..
        }
    ));

    let array_json = json!({
        "type": "invalid_request_error",
        "message": "Validation failed",
        "errors": [{ "field": "source.number", "message": "is invalid" }],
        "response_code": "51"
    });
    let env_arr: ApiErrorPayload = serde_json::from_value(array_json).unwrap();
    assert_eq!(env_arr.error_type, ApiErrorType::InvalidRequestError);
    assert_eq!(env_arr.response_code, Some(ResponseCode::InsufficientFunds));
    assert_eq!(env_arr.errors.len(), 1);

    let map_json = json!({
        "type": "invalid_request_error",
        "message": "Validation failed",
        "errors": {
            "amount": ["must be greater than or equal to 100"],
            "currency": "is not supported"
        },
        "response_code": "00"
    });
    let env_map: ApiErrorPayload = serde_json::from_value(map_json).unwrap();
    assert_eq!(env_map.errors.len(), 2);
    assert_eq!(env_map.response_code, Some(ResponseCode::Approved));
}

#[tokio::test]
async fn paginator_streams_multiple_pages_and_propagates_mid_stream_errors() {
    let stream = Paginator::new(1, |page| async move {
        let (items, next_page, prev_page) = if page == 1 {
            (vec![10, 20], Some(2), None)
        } else {
            (vec![30], None, Some(1))
        };
        Ok::<_, MoyasarError>(PaginatedList {
            items,
            meta: Meta {
                current_page: page,
                next_page,
                prev_page,
                total_pages: 2,
                total_count: 3,
            },
        })
    })
    .into_stream();

    let collected: Vec<i32> = stream.try_collect().await.unwrap();
    assert_eq!(collected, vec![10, 20, 30]);

    let failing_stream = Paginator::new(1, |page| async move {
        if page == 1 {
            Ok(PaginatedList {
                items: vec![1],
                meta: Meta {
                    current_page: 1,
                    next_page: Some(2),
                    prev_page: None,
                    total_pages: 2,
                    total_count: 2,
                },
            })
        } else {
            Err(MoyasarError::ServerError {
                status: 502,
                message: "Bad Gateway".to_string(),
            })
        }
    })
    .into_stream();

    let result: Result<Vec<i32>, MoyasarError> = failing_stream.try_collect().await;
    assert!(matches!(
        result,
        Err(MoyasarError::ServerError { status: 502, .. })
    ));
}

#[tokio::test]
async fn transport_exhausts_5xx_retries_and_returns_server_error_503() {
    let (server, client) = common::start_secret_client_with_retries(1).await;
    common::mount_raw_endpoint(&server, ("GET", "/v1/payments"), 503, "Service Unavailable").await;

    let err = client
        .payments()
        .list(&ListParams::new().page(1))
        .await
        .expect_err("persistent 503 must exhaust retries");
    assert!(matches!(
        err,
        MoyasarError::ServerError { status: 503, .. }
    ));
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
