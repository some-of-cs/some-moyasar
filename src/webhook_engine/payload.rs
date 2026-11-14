//! Typed [`WebhookEvent`] payload extractors (US-7.3.0 Unit 5).

use serde::de::DeserializeOwned;

use super::event_types::{WebhookError, WebhookEvent};

impl WebhookEvent {
    /// Deserializes the raw JSON [`WebhookEvent::data`] payload into a strongly typed `T`.
    ///
    /// Operates directly on a shared reference to [`Self::data`] without cloning
    /// the underlying [`serde_json::Value`] tree.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookError::InvalidJson`] if `self.data` cannot be deserialized
    /// into `T` (for example, due to missing required fields or type mismatches).
    pub fn extract_data<T: DeserializeOwned>(&self) -> Result<T, WebhookError> {
        Ok(T::deserialize(&self.data)?)
    }

    /// Extracts the webhook payload as a typed
    /// [`Payment`](crate::resources::payments::Payment).
    ///
    /// # Errors
    ///
    /// Returns [`WebhookError::InvalidJson`] if `self.data` does not conform to
    /// the Moyasar Payment resource schema.
    #[cfg(feature = "payments")]
    pub fn into_payment(&self) -> Result<crate::resources::payments::Payment, WebhookError> {
        self.extract_data()
    }

    /// Extracts the webhook payload as a typed
    /// [`CardAuthentication`](crate::resources::card_auths::CardAuthentication).
    ///
    /// # Errors
    ///
    /// Returns [`WebhookError::InvalidJson`] if `self.data` does not conform to
    /// the Moyasar standalone 3DS Card Authentication schema.
    #[cfg(feature = "card-auths")]
    pub fn into_card_auth(
        &self,
    ) -> Result<crate::resources::card_auths::CardAuthentication, WebhookError> {
        self.extract_data()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::{DateTime, Duration as ChronoDuration, Utc};
    use serde::Deserialize;
    use serde_json::json;
    use uuid::Uuid;

    use super::super::event_types::WebhookEventType;
    use super::super::timestamp::REPLAY_TOLERANCE;
    use super::super::verifier::ConstantTimeSecretVerifier;

    const TEST_SECRET: &str = "whsec_live_moyasar_test_secret_32b";

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct CustomPayload {
        id: String,
        amount: u64,
        currency: String,
    }

    fn base_created_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-14T00:00:00Z")
            .expect("valid RFC3339 timestamp")
            .with_timezone(&Utc)
    }

    fn make_event(event_type: WebhookEventType, data: serde_json::Value) -> WebhookEvent {
        WebhookEvent {
            id: Uuid::nil(),
            event_type,
            created_at: base_created_at(),
            secret_token: TEST_SECRET.to_owned(),
            account_name: Some("some_cs_merchant".to_owned()),
            live: false,
            data,
        }
    }

    fn sample_payment_json() -> serde_json::Value {
        json!({
            "id": "pay_01J9XYZ",
            "status": "paid",
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 350,
            "fee_format": "3.50 SAR",
            "refunded": 0,
            "refunded_format": "0.00 SAR",
            "captured": 15000,
            "captured_format": "150.00 SAR",
            "currency": "SAR",
            "description": "Order #1042",
            "source": { "type": "creditcard", "company": "mada" },
            "metadata": { "order_id": "ORD-1042" },
            "callback_url": "https://merchant.example/callback",
            "invoice_id": null,
            "ip": "192.0.2.10",
            "captured_at": "2026-11-14T00:00:00Z",
            "refunded_at": null,
            "voided_at": null,
            "created_at": "2026-11-14T00:00:00Z",
            "updated_at": "2026-11-14T00:00:00Z"
        })
    }

    fn sample_card_auth_json() -> serde_json::Value {
        json!({
            "id": "ca_9a8b7c6d5e",
            "status": "authenticated",
            "transaction_url": "https://api.moyasar.com/v1/card_auth/ca_9a8b7c6d5e/prepare",
            "result": {
                "eci": "05",
                "authentication_value": "kH9z8x7c6v5b4n3m2l1k0j==",
                "ds_transaction_id": "11111111-2222-3333-4444-555555555555",
                "version": "2.2.0",
                "transaction_status": "Y",
                "auth_scheme": "visa",
                "is_frictionless": true
            }
        })
    }

    fn build_raw_webhook_body(
        event_type: &str,
        secret_token: &str,
        created_at: DateTime<Utc>,
        data: serde_json::Value,
    ) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "type": event_type,
            "created_at": created_at.to_rfc3339(),
            "secret_token": secret_token,
            "account_name": "some_cs_merchant",
            "live": true,
            "data": data
        }))
        .expect("serializes test webhook JSON")
    }

    #[test]
    fn extract_data_deserializes_custom_struct() {
        let event = make_event(
            WebhookEventType::PaymentPaid,
            json!({
                "id": "pay_custom_01",
                "amount": 2500,
                "currency": "SAR"
            }),
        );

        let extracted: CustomPayload = event.extract_data().expect("custom payload should extract");
        assert_eq!(
            extracted,
            CustomPayload {
                id: "pay_custom_01".to_owned(),
                amount: 2500,
                currency: "SAR".to_owned(),
            }
        );
    }

    #[test]
    fn extract_data_returns_invalid_json_on_schema_mismatch() {
        let event = make_event(
            WebhookEventType::PaymentPaid,
            json!({ "id": "pay_missing_fields" }),
        );

        let err = event
            .extract_data::<CustomPayload>()
            .expect_err("missing fields must return InvalidJson");
        assert!(matches!(err, WebhookError::InvalidJson(_)));
    }

    #[cfg(feature = "payments")]
    #[test]
    fn into_payment_extracts_typed_payment_resource() {
        use crate::kernel::types::Currency;
        use crate::resources::payments::PaymentStatus;

        let event = make_event(WebhookEventType::PaymentPaid, sample_payment_json());
        let payment = event.into_payment().expect("valid payment payload");

        assert_eq!(payment.id, "pay_01J9XYZ");
        assert_eq!(payment.status, PaymentStatus::Paid);
        assert_eq!(payment.amount, 15000);
        assert_eq!(payment.currency, Currency::SAR);
        assert_eq!(payment.metadata.get("order_id"), Some("ORD-1042"));
    }

    #[cfg(feature = "payments")]
    #[test]
    fn into_payment_rejects_non_payment_payload() {
        let event = make_event(WebhookEventType::PaymentFailed, sample_card_auth_json());
        let err = event
            .into_payment()
            .expect_err("card auth payload must fail Payment extraction");
        assert!(matches!(err, WebhookError::InvalidJson(_)));
    }

    #[cfg(feature = "card-auths")]
    #[test]
    fn into_card_auth_extracts_typed_card_authentication() {
        use crate::resources::card_auths::CardAuthStatus;

        let event = make_event(
            WebhookEventType::CardAuthAuthenticated,
            sample_card_auth_json(),
        );
        let auth = event
            .into_card_auth()
            .expect("valid card authentication payload");

        assert_eq!(auth.id.as_str(), "ca_9a8b7c6d5e");
        assert_eq!(auth.status, CardAuthStatus::Authenticated);
        assert!(auth.result.is_some());
    }

    #[cfg(feature = "card-auths")]
    #[test]
    fn into_card_auth_rejects_invalid_id_prefix() {
        let event = make_event(WebhookEventType::CardAuthFailed, sample_payment_json());
        let err = event
            .into_card_auth()
            .expect_err("payment payload must fail CardAuthentication extraction");
        assert!(matches!(err, WebhookError::InvalidJson(_)));
    }

    #[test]
    fn replay_ladder_accepts_all_retry_offsets_up_to_221m() {
        let created_at = base_created_at();
        let verifier = ConstantTimeSecretVerifier::new(TEST_SECRET, REPLAY_TOLERANCE);
        let raw = build_raw_webhook_body(
            "payment_paid",
            TEST_SECRET,
            created_at,
            sample_payment_json(),
        );

        for offset_mins in [0_i64, 1, 5, 11, 15, 41, 60, 101, 221] {
            let now = created_at + ChronoDuration::minutes(offset_mins);
            let event = verifier
                .verify_and_construct_at(&raw, now)
                .expect("retry offset within 4h tolerance must pass");
            assert_eq!(event.event_type, WebhookEventType::PaymentPaid);
        }

        let expired_now = created_at + ChronoDuration::seconds(4 * 3600 + 1);
        let expired_err = verifier
            .verify_and_construct_at(&raw, expired_now)
            .expect_err("timestamp older than 4h must be rejected");
        assert!(matches!(expired_err, WebhookError::TimestampOutOfRange));

        let future_skew_now = created_at - ChronoDuration::seconds(301);
        let future_err = verifier
            .verify_and_construct_at(&raw, future_skew_now)
            .expect_err("timestamp >5m in the future must be rejected");
        assert!(matches!(future_err, WebhookError::TimestampOutOfRange));
    }

    #[test]
    fn constant_time_verifier_rejects_mismatched_secrets_of_varying_lengths() {
        let created_at = base_created_at();
        let verifier = ConstantTimeSecretVerifier::new(TEST_SECRET, REPLAY_TOLERANCE);

        for bad_secret in [
            "",
            "x",
            "whsec_wrong_secret_same_len_32by",
            "whsec_very_long_forged_secret_token_exceeding_expected_length_64_bytes",
        ] {
            let raw = build_raw_webhook_body(
                "payment_faild",
                bad_secret,
                created_at,
                sample_payment_json(),
            );
            let err = verifier
                .verify_and_construct_at(&raw, created_at)
                .expect_err("forged secret_token must return SecretMismatch");
            assert!(matches!(err, WebhookError::SecretMismatch));
        }
    }

    #[test]
    fn verified_webhook_chains_into_typed_extractors_and_redacts_debug() {
        let created_at = base_created_at();
        let verifier = ConstantTimeSecretVerifier::new(TEST_SECRET, REPLAY_TOLERANCE);
        let raw = build_raw_webhook_body(
            "payment_faild",
            TEST_SECRET,
            created_at,
            sample_payment_json(),
        );

        let event = verifier
            .verify_and_construct_at(&raw, created_at)
            .expect("valid webhook must verify");
        assert_eq!(event.event_type, WebhookEventType::PaymentFailed);

        let debug_str = format!("{event:?}");
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains(TEST_SECRET));

        #[cfg(feature = "payments")]
        {
            let payment = event.into_payment().expect("verified payment extracts");
            assert_eq!(payment.id, "pay_01J9XYZ");
        }
    }
}
