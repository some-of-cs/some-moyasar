//! Pre-flight validation for UUIDs, DPAN/STC Pay manual capture, empty-body ops, metadata, and splits (US-9.1.0 Unit 3).

use std::collections::BTreeMap;

use crate::kernel::MoyasarError;

/// Maximum number of metadata entries permitted by the Moyasar API.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::preflight_rules::MAX_METADATA_ENTRIES;
///
/// assert_eq!(MAX_METADATA_ENTRIES, 30);
/// ```
pub const MAX_METADATA_ENTRIES: usize = 30;

/// Maximum byte length permitted for a single metadata key.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::preflight_rules::MAX_METADATA_KEY_LEN;
///
/// assert_eq!(MAX_METADATA_KEY_LEN, 40);
/// ```
pub const MAX_METADATA_KEY_LEN: usize = 40;

/// Maximum byte length permitted for a single metadata value.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::preflight_rules::MAX_METADATA_VALUE_LEN;
///
/// assert_eq!(MAX_METADATA_VALUE_LEN, 500);
/// ```
pub const MAX_METADATA_VALUE_LEN: usize = 500;

/// Maximum number of split payout items permitted on a single payment.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::preflight_rules::MAX_SPLITS;
///
/// assert_eq!(MAX_SPLITS, 50);
/// ```
pub const MAX_SPLITS: usize = 50;

const DISALLOWED_MANUAL_SOURCES: &[&str] = &["stcpay", "applepay", "samsungpay", "dpan", "sadad"];

/// Parses a Moyasar resource identifier as a canonical UUID, failing closed locally on empty or
/// malformed values.
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with
/// [`ApiErrorType::InvalidRequestError`](crate::kernel::ApiErrorType::InvalidRequestError) when
/// `id` is empty after trimming or cannot be parsed by [`uuid::Uuid::parse_str`].
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::preflight_rules::validate_uuid_id;
///
/// let parsed = validate_uuid_id("550e8400-e29b-41d4-a716-446655440000", "payment_id")?;
/// assert_eq!(parsed.to_string(), "550e8400-e29b-41d4-a716-446655440000");
/// assert!(validate_uuid_id("not-a-uuid", "payment_id").is_err());
/// # Ok::<(), some_moyasar::MoyasarError>(())
/// ```
pub fn validate_uuid_id(id: &str, field_name: &str) -> Result<uuid::Uuid, MoyasarError> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(MoyasarError::invalid_request(format!(
            "{field_name} must be a non-empty canonical UUID"
        )));
    }
    uuid::Uuid::parse_str(trimmed).map_err(|_| {
        MoyasarError::invalid_request(format!("{field_name} must be a valid canonical UUID"))
    })
}

/// Validates that `manual: true` capture is not requested for Moyasar payment sources that do not
/// support manual capture (`stcpay`, `applepay`, `samsungpay`, `dpan`, `sadad`).
///
/// Matching is case-insensitive and ignores leading/trailing ASCII whitespace.
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with
/// [`ApiErrorType::InvalidRequestError`](crate::kernel::ApiErrorType::InvalidRequestError) when
/// `manual` is `true` and `source_type` matches a disallowed source.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::preflight_rules::validate_manual_capture_source;
///
/// assert!(validate_manual_capture_source(true, Some("creditcard")).is_ok());
/// assert!(validate_manual_capture_source(true, Some("token")).is_ok());
/// assert!(validate_manual_capture_source(true, Some("applepay")).is_err());
/// assert!(validate_manual_capture_source(false, Some("stcpay")).is_ok());
/// ```
pub fn validate_manual_capture_source(
    manual: bool,
    source_type: Option<&str>,
) -> Result<(), MoyasarError> {
    if !manual {
        return Ok(());
    }
    let Some(raw_source) = source_type else {
        return Ok(());
    };
    let source = raw_source.trim();
    if is_disallowed_manual_source(source) {
        return Err(MoyasarError::invalid_request(format!(
            "manual capture is not supported for source type '{source}'"
        )));
    }
    Ok(())
}

fn is_disallowed_manual_source(source: &str) -> bool {
    DISALLOWED_MANUAL_SOURCES
        .iter()
        .any(|disallowed| disallowed.eq_ignore_ascii_case(source))
}

/// Validates Moyasar metadata limits: at most [`MAX_METADATA_ENTRIES`] entries, non-empty keys of
/// at most [`MAX_METADATA_KEY_LEN`] bytes, and values of at most [`MAX_METADATA_VALUE_LEN`] bytes.
///
/// # Errors
///
/// Returns [`MoyasarError::MetadataTooManyKeys`], [`MoyasarError::MetadataKeyTooLong`],
/// [`MoyasarError::MetadataValueTooLong`], or an invalid-request error when a key is empty.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use some_moyasar::alignment::preflight_rules::validate_metadata_limits;
///
/// let mut metadata = BTreeMap::new();
/// metadata.insert("order_id".to_string(), "ord_12345".to_string());
/// assert!(validate_metadata_limits(&metadata).is_ok());
/// ```
pub fn validate_metadata_limits(metadata: &BTreeMap<String, String>) -> Result<(), MoyasarError> {
    if metadata.len() > MAX_METADATA_ENTRIES {
        return Err(MoyasarError::MetadataTooManyKeys);
    }
    for (key, value) in metadata {
        validate_metadata_entry(key, value)?;
    }
    Ok(())
}

fn validate_metadata_entry(key: &str, value: &str) -> Result<(), MoyasarError> {
    if key.is_empty() {
        return Err(MoyasarError::invalid_request(
            "metadata key must not be empty",
        ));
    }
    if key.len() > MAX_METADATA_KEY_LEN {
        return Err(MoyasarError::MetadataKeyTooLong);
    }
    if value.len() > MAX_METADATA_VALUE_LEN {
        return Err(MoyasarError::MetadataValueTooLong);
    }
    Ok(())
}

/// Lightweight split item descriptor for pre-flight split sum validation.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::preflight_rules::SplitCheckItem;
///
/// let item = SplitCheckItem {
///     recipient_id: "acct_merchant_1".to_string(),
///     amount: 2_500,
/// };
/// assert_eq!(item.amount, 2_500);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitCheckItem {
    /// Non-empty identifier of the split recipient.
    pub recipient_id: String,
    /// Strictly positive split amount in minor currency units (e.g., halalas).
    pub amount: u64,
}

/// Validates split count, per-split recipient/amount invariants, and checked integer summation
/// against `total_amount`.
///
/// # Errors
///
/// Returns:
/// - [`MoyasarError::InvalidSplitAmount`] when any split has `amount == 0`.
/// - [`MoyasarError::SplitTotalExceedsAmount`] when the checked sum overflows `u64` or exceeds
///   `total_amount`.
/// - [`MoyasarError::Api`] with
///   [`ApiErrorType::InvalidRequestError`](crate::kernel::ApiErrorType::InvalidRequestError) when
///   `splits.len() > MAX_SPLITS` or any `recipient_id` is empty/whitespace.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::preflight_rules::{SplitCheckItem, validate_splits_sum};
///
/// let splits = vec![
///     SplitCheckItem {
///         recipient_id: "rec_1".to_string(),
///         amount: 600,
///     },
///     SplitCheckItem {
///         recipient_id: "rec_2".to_string(),
///         amount: 400,
///     },
/// ];
/// assert!(validate_splits_sum(&splits, 1_000).is_ok());
/// assert!(validate_splits_sum(&splits, 999).is_err());
/// ```
pub fn validate_splits_sum(
    splits: &[SplitCheckItem],
    total_amount: u64,
) -> Result<(), MoyasarError> {
    if splits.len() > MAX_SPLITS {
        return Err(MoyasarError::invalid_request(format!(
            "splits count exceeds maximum of {MAX_SPLITS}"
        )));
    }
    let running_total = splits.iter().try_fold(0_u64, |acc, split| {
        let amount = validate_split_item(split)?;
        acc.checked_add(amount)
            .ok_or(MoyasarError::SplitTotalExceedsAmount)
    })?;
    if running_total > total_amount {
        return Err(MoyasarError::SplitTotalExceedsAmount);
    }
    Ok(())
}

fn validate_split_item(item: &SplitCheckItem) -> Result<u64, MoyasarError> {
    if item.recipient_id.trim().is_empty() {
        return Err(MoyasarError::invalid_request(
            "split recipient_id must not be empty",
        ));
    }
    if item.amount == 0 {
        return Err(MoyasarError::InvalidSplitAmount);
    }
    Ok(item.amount)
}

/// Validates an optional partial mutation amount for capture or refund operations, rejecting
/// `Some(0)` while allowing `None` (full operation) and `Some(amount)` where `amount > 0`.
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with
/// [`ApiErrorType::InvalidRequestError`](crate::kernel::ApiErrorType::InvalidRequestError) when
/// `amount` is `Some(0)`.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::preflight_rules::validate_partial_amount;
///
/// assert_eq!(validate_partial_amount(None)?, None);
/// assert_eq!(validate_partial_amount(Some(500))?, Some(500));
/// assert!(validate_partial_amount(Some(0)).is_err());
/// # Ok::<(), some_moyasar::MoyasarError>(())
/// ```
pub fn validate_partial_amount(amount: Option<u64>) -> Result<Option<u64>, MoyasarError> {
    if matches!(amount, Some(0)) {
        return Err(MoyasarError::invalid_request(
            "partial mutation amount must be greater than zero",
        ));
    }
    Ok(amount)
}

/// Builds the HTTP JSON body for Moyasar payment mutations (capture, refund, void).
///
/// Returns `None` (empty HTTP body) for full capture, full refund, and void (`partial_amount` is
/// `None`), cleanly omitting unsupported canonical fields such as `refund.reason`. Returns
/// `Some({"amount": amt})` for partial capture or partial refund when `amt > 0`.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use some_moyasar::alignment::preflight_rules::build_mutation_body;
///
/// assert_eq!(build_mutation_body(None), None);
/// assert_eq!(build_mutation_body(Some(750)), Some(json!({ "amount": 750 })));
/// ```
#[must_use]
pub fn build_mutation_body(partial_amount: Option<u64>) -> Option<serde_json::Value> {
    partial_amount
        .filter(|&amt| amt > 0)
        .map(|amt| serde_json::json!({ "amount": amt }))
}

/// Aggregated pre-flight validation input for a Moyasar payment request.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use some_moyasar::alignment::preflight_rules::{
///     MoyasarPreflightInput, SplitCheckItem, validate_moyasar_preflight,
/// };
///
/// let metadata = BTreeMap::new();
/// let splits = [SplitCheckItem {
///     recipient_id: "rec_1".to_string(),
///     amount: 500,
/// }];
/// let input = MoyasarPreflightInput {
///     payment_id: Some("550e8400-e29b-41d4-a716-446655440000"),
///     amount: 1_000,
///     manual_capture: true,
///     source_type: Some("creditcard"),
///     metadata: &metadata,
///     splits: &splits,
/// };
/// assert!(validate_moyasar_preflight(&input).is_ok());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoyasarPreflightInput<'a> {
    /// Optional payment identifier to validate as a canonical UUID.
    pub payment_id: Option<&'a str>,
    /// Total payment amount in minor currency units (must be strictly positive).
    pub amount: u64,
    /// Whether manual capture is requested (`manual: true`).
    pub manual_capture: bool,
    /// Optional Moyasar source type (e.g., `"creditcard"`, `"token"`, `"applepay"`, `"stcpay"`).
    pub source_type: Option<&'a str>,
    /// Key-value metadata map attached to the payment request.
    pub metadata: &'a BTreeMap<String, String>,
    /// Payment split items to validate against `amount`.
    pub splits: &'a [SplitCheckItem],
}

/// Orchestrates all Moyasar pre-flight validation rules in a single fail-closed pass:
/// positive payment amount, optional UUID parsing, manual capture source eligibility,
/// metadata entry/length limits, and checked split summation.
///
/// # Errors
///
/// Returns the first [`MoyasarError`] encountered across amount, UUID, manual-capture source,
/// metadata, and splits checks.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use some_moyasar::alignment::preflight_rules::{
///     MoyasarPreflightInput, validate_moyasar_preflight,
/// };
///
/// let metadata = BTreeMap::new();
/// let input = MoyasarPreflightInput {
///     payment_id: None,
///     amount: 1_500,
///     manual_capture: false,
///     source_type: Some("stcpay"),
///     metadata: &metadata,
///     splits: &[],
/// };
/// assert!(validate_moyasar_preflight(&input).is_ok());
/// ```
pub fn validate_moyasar_preflight(input: &MoyasarPreflightInput<'_>) -> Result<(), MoyasarError> {
    if input.amount == 0 {
        return Err(MoyasarError::invalid_request(
            "payment amount must be greater than zero",
        ));
    }
    if let Some(payment_id) = input.payment_id {
        validate_uuid_id(payment_id, "payment_id")?;
    }
    validate_manual_capture_source(input.manual_capture, input.source_type)?;
    validate_metadata_limits(input.metadata)?;
    validate_splits_sum(input.splits, input.amount)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;

    #[test]
    fn tc_9_1_3_uuid_validation_accepts_canonical_and_rejects_invalid() {
        let valid = "550e8400-e29b-41d4-a716-446655440000";
        let parsed = validate_uuid_id(valid, "payment_id").expect("valid UUID must pass");
        assert_eq!(parsed.to_string(), valid);

        let padded = "  550e8400-e29b-41d4-a716-446655440000  ";
        assert_eq!(
            validate_uuid_id(padded, "payment_id")
                .expect("trimmed UUID must pass")
                .to_string(),
            valid
        );

        assert!(validate_uuid_id("", "payment_id").is_err());
        assert!(validate_uuid_id("   ", "payment_id").is_err());
        assert!(validate_uuid_id("pay_12345", "payment_id").is_err());
        assert!(validate_uuid_id("550e8400-e29b-41d4-a716-44665544000z", "payment_id").is_err());
    }

    #[test]
    fn tc_9_1_3_manual_capture_rejects_dpan_stcpay_sadad_and_allows_card_or_token() {
        let disallowed_sources = [
            "stcpay",
            "STCPAY",
            "applepay",
            "ApplePay",
            "samsungpay",
            "SamsungPay",
            "dpan",
            "DPAN",
            "sadad",
            "SADAD",
            "  applepay  ",
        ];
        for disallowed in disallowed_sources {
            assert!(
                validate_manual_capture_source(true, Some(disallowed)).is_err(),
                "manual capture must fail for {disallowed}"
            );
            assert!(
                validate_manual_capture_source(false, Some(disallowed)).is_ok(),
                "auto capture must be allowed for {disallowed}"
            );
        }

        assert!(validate_manual_capture_source(true, Some("creditcard")).is_ok());
        assert!(validate_manual_capture_source(true, Some("token")).is_ok());
        assert!(validate_manual_capture_source(true, None).is_ok());
    }

    #[test]
    fn tc_9_1_3_empty_body_for_full_ops_and_partial_amount_validation() {
        assert_eq!(build_mutation_body(None), None);
        assert_eq!(build_mutation_body(Some(0)), None);
        assert_eq!(build_mutation_body(Some(1_250)), Some(json!({ "amount": 1_250 })));

        assert_eq!(validate_partial_amount(None).expect("None is valid"), None);
        assert_eq!(validate_partial_amount(Some(1)).expect("Some(1) is valid"), Some(1));
        assert!(validate_partial_amount(Some(0)).is_err());
    }

    #[test]
    fn tc_9_1_3_metadata_boundaries() {
        let mut ok_30 = BTreeMap::new();
        for idx in 0..MAX_METADATA_ENTRIES {
            ok_30.insert(format!("k{idx:02}"), "v".to_string());
        }
        assert!(validate_metadata_limits(&ok_30).is_ok());

        ok_30.insert("k30".to_string(), "v".to_string());
        assert!(matches!(
            validate_metadata_limits(&ok_30),
            Err(MoyasarError::MetadataTooManyKeys)
        ));

        let mut empty_key = BTreeMap::new();
        empty_key.insert(String::new(), "val".to_string());
        assert!(validate_metadata_limits(&empty_key).is_err());

        let mut key_40 = BTreeMap::new();
        key_40.insert("k".repeat(MAX_METADATA_KEY_LEN), "ok".to_string());
        assert!(validate_metadata_limits(&key_40).is_ok());

        let mut key_41 = BTreeMap::new();
        key_41.insert("k".repeat(MAX_METADATA_KEY_LEN + 1), "ok".to_string());
        assert!(matches!(
            validate_metadata_limits(&key_41),
            Err(MoyasarError::MetadataKeyTooLong)
        ));

        let mut val_500 = BTreeMap::new();
        val_500.insert("key".to_string(), "v".repeat(MAX_METADATA_VALUE_LEN));
        assert!(validate_metadata_limits(&val_500).is_ok());

        let mut val_501 = BTreeMap::new();
        val_501.insert("key".to_string(), "v".repeat(MAX_METADATA_VALUE_LEN + 1));
        assert!(matches!(
            validate_metadata_limits(&val_501),
            Err(MoyasarError::MetadataValueTooLong)
        ));
    }

    #[test]
    fn tc_9_1_3_splits_sum_boundaries_and_overflow() {
        let exact = vec![
            SplitCheckItem {
                recipient_id: "rec_a".to_string(),
                amount: 700,
            },
            SplitCheckItem {
                recipient_id: "rec_b".to_string(),
                amount: 300,
            },
        ];
        assert!(validate_splits_sum(&exact, 1_000).is_ok());
        assert!(validate_splits_sum(&exact, 1_200).is_ok());
        assert!(matches!(
            validate_splits_sum(&exact, 999),
            Err(MoyasarError::SplitTotalExceedsAmount)
        ));

        let overflow = vec![
            SplitCheckItem {
                recipient_id: "rec_a".to_string(),
                amount: u64::MAX,
            },
            SplitCheckItem {
                recipient_id: "rec_b".to_string(),
                amount: 1,
            },
        ];
        assert!(matches!(
            validate_splits_sum(&overflow, u64::MAX),
            Err(MoyasarError::SplitTotalExceedsAmount)
        ));

        let zero_amount = vec![SplitCheckItem {
            recipient_id: "rec_a".to_string(),
            amount: 0,
        }];
        assert!(matches!(
            validate_splits_sum(&zero_amount, 1_000),
            Err(MoyasarError::InvalidSplitAmount)
        ));

        let empty_recipient = vec![SplitCheckItem {
            recipient_id: "   ".to_string(),
            amount: 100,
        }];
        assert!(validate_splits_sum(&empty_recipient, 1_000).is_err());

        let max_splits: Vec<SplitCheckItem> = (0..MAX_SPLITS)
            .map(|idx| SplitCheckItem {
                recipient_id: format!("rec_{idx}"),
                amount: 10,
            })
            .collect();
        assert!(validate_splits_sum(&max_splits, 500).is_ok());

        let mut too_many = max_splits;
        too_many.push(SplitCheckItem {
            recipient_id: "rec_50".to_string(),
            amount: 10,
        });
        assert!(validate_splits_sum(&too_many, 1_000).is_err());
    }

    #[test]
    fn tc_9_1_3_preflight_orchestrator_checks_all_invariants() {
        let metadata = BTreeMap::from([("order_id".to_string(), "ord_1".to_string())]);
        let splits = vec![SplitCheckItem {
            recipient_id: "rec_1".to_string(),
            amount: 500,
        }];
        let base = MoyasarPreflightInput {
            payment_id: Some("550e8400-e29b-41d4-a716-446655440000"),
            amount: 1_000,
            manual_capture: true,
            source_type: Some("creditcard"),
            metadata: &metadata,
            splits: &splits,
        };
        assert!(validate_moyasar_preflight(&base).is_ok());

        let zero_amt = MoyasarPreflightInput { amount: 0, ..base };
        assert!(validate_moyasar_preflight(&zero_amt).is_err());

        let bad_uuid = MoyasarPreflightInput {
            payment_id: Some("invalid-id"),
            ..base
        };
        assert!(validate_moyasar_preflight(&bad_uuid).is_err());

        let bad_manual = MoyasarPreflightInput {
            source_type: Some("applepay"),
            ..base
        };
        assert!(validate_moyasar_preflight(&bad_manual).is_err());

        let bad_splits = [SplitCheckItem {
            recipient_id: "rec_1".to_string(),
            amount: 1_001,
        }];
        let over_split = MoyasarPreflightInput {
            splits: &bad_splits,
            ..base
        };
        assert!(matches!(
            validate_moyasar_preflight(&over_split),
            Err(MoyasarError::SplitTotalExceedsAmount)
        ));
    }
}
