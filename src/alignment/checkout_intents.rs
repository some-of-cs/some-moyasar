//! Payment Intents (`^pi_secret_`) and Invoices API (`invoice.url`) checkout alignment (US-9.1.0 Unit 5).

use core::fmt;

use crate::kernel::MoyasarError;

/// Required prefix for Moyasar Payment Intent client secrets (`pi_secret_`).
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::checkout_intents::PAYMENT_INTENT_SECRET_PREFIX;
///
/// assert_eq!(PAYMENT_INTENT_SECRET_PREFIX, "pi_secret_");
/// ```
pub const PAYMENT_INTENT_SECRET_PREFIX: &str = "pi_secret_";

const INTENT_NOT_FULFILLABLE_CODE: &str = "intent_not_fulfillable";

/// Validated hosted checkout outcome mapped from a Moyasar Invoice response.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::checkout_intents::{InvoiceCheckoutOutcome, map_invoice_checkout};
///
/// let outcome: InvoiceCheckoutOutcome = map_invoice_checkout(
///     "inv_0123456789",
///     Some("https://invoice.moyasar.com/inv_0123456789"),
///     15_000,
///     "SAR",
/// )?;
/// assert_eq!(outcome.invoice_id, "inv_0123456789");
/// assert_eq!(outcome.checkout_url, "https://invoice.moyasar.com/inv_0123456789");
/// assert_eq!(outcome.amount, 15_000);
/// assert_eq!(outcome.currency, "SAR");
/// # Ok::<(), some_moyasar::MoyasarError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceCheckoutOutcome {
    /// Unique Moyasar invoice identifier.
    pub invoice_id: String,
    /// Validated `http://` or `https://` hosted checkout URL (`invoice.url`).
    pub checkout_url: String,
    /// Positive invoice amount in minor currency units (e.g., halalas).
    pub amount: u64,
    /// Normalized 3-letter uppercase ASCII ISO 4217 currency code.
    pub currency: String,
}

/// Validated Payment Intent checkout outcome containing a redacted, zeroized `client_secret`.
///
/// `client_secret` is automatically wiped from memory on drop via [`zeroize::ZeroizeOnDrop`]
/// and is always rendered as `"[REDACTED]"` in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::checkout_intents::{IntentCheckoutOutcome, map_intent_result};
///
/// let outcome: IntentCheckoutOutcome = map_intent_result(
///     "pi_0123456789",
///     "pi_secret_abc123",
///     25_000,
///     "SAR",
/// )?;
/// let debug_output = format!("{outcome:?}");
/// assert!(debug_output.contains("[REDACTED]"));
/// assert!(!debug_output.contains("pi_secret_abc123"));
/// # Ok::<(), some_moyasar::MoyasarError>(())
/// ```
#[derive(Clone, PartialEq, Eq, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct IntentCheckoutOutcome {
    /// Unique Moyasar payment intent identifier.
    pub intent_id: String,
    /// Sensitive `pi_secret_`-prefixed client secret (redacted in `Debug`, zeroized on drop).
    pub client_secret: String,
    /// Positive payment intent amount in minor currency units (e.g., halalas).
    pub amount: u64,
    /// Normalized 3-letter uppercase ASCII ISO 4217 currency code.
    pub currency: String,
}

impl fmt::Debug for IntentCheckoutOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IntentCheckoutOutcome")
            .field("intent_id", &self.intent_id)
            .field("client_secret", &"[REDACTED]")
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .finish()
    }
}

/// Validates that a Moyasar Payment Intent `client_secret` starts with
/// [`PAYMENT_INTENT_SECRET_PREFIX`] (`"pi_secret_"`) and has a non-empty, whitespace-free suffix.
///
/// Error messages never include the secret value.
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with `InvalidRequestError` if `secret` does not start with
/// `"pi_secret_"`, has an empty suffix, or contains any whitespace.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::checkout_intents::validate_intent_secret;
///
/// assert!(validate_intent_secret("pi_secret_abc123").is_ok());
/// assert!(validate_intent_secret("pi_secret_").is_err());
/// assert!(validate_intent_secret("sk_test_abc123").is_err());
/// ```
pub fn validate_intent_secret(secret: &str) -> Result<(), MoyasarError> {
    let Some(suffix) = secret.strip_prefix(PAYMENT_INTENT_SECRET_PREFIX) else {
        return Err(MoyasarError::invalid_request(
            "payment intent client_secret must start with `pi_secret_`",
        ));
    };

    if suffix.is_empty() || suffix.chars().any(char::is_whitespace) {
        return Err(MoyasarError::invalid_request(
            "payment intent client_secret must have a non-empty, whitespace-free suffix",
        ));
    }

    Ok(())
}

/// Detects whether a Moyasar API error indicates `intent_not_fulfillable` (case-insensitive)
/// in either `error_type` or `error_code`.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::checkout_intents::is_intent_not_fulfillable;
///
/// assert!(is_intent_not_fulfillable(Some("intent_not_fulfillable"), None));
/// assert!(is_intent_not_fulfillable(None, Some("INTENT_NOT_FULFILLABLE")));
/// assert!(!is_intent_not_fulfillable(Some("invalid_request_error"), Some("400")));
/// ```
#[must_use]
pub fn is_intent_not_fulfillable(error_type: Option<&str>, error_code: Option<&str>) -> bool {
    matches_intent_not_fulfillable(error_type) || matches_intent_not_fulfillable(error_code)
}

/// Validates and maps a Moyasar Invoice response into an [`InvoiceCheckoutOutcome`].
///
/// Fails closed if `invoice_id` is empty, `amount` is zero, `currency` is not a 3-letter ASCII
/// code, or `invoice_url` is missing, empty, or not an `http://` / `https://` URL.
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with `InvalidRequestError` when any input field fails validation.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::checkout_intents::map_invoice_checkout;
///
/// let outcome = map_invoice_checkout(
///     "inv_123",
///     Some("https://invoice.moyasar.com/inv_123"),
///     10_000,
///     "SAR",
/// )?;
/// assert_eq!(outcome.checkout_url, "https://invoice.moyasar.com/inv_123");
/// # Ok::<(), some_moyasar::MoyasarError>(())
/// ```
pub fn map_invoice_checkout(
    invoice_id: &str,
    invoice_url: Option<&str>,
    amount: u64,
    currency: &str,
) -> Result<InvoiceCheckoutOutcome, MoyasarError> {
    let invoice_id = validate_non_empty_id(invoice_id, "invoice_id")?;
    let checkout_url = validate_checkout_url(invoice_url)?;
    let amount = validate_positive_amount(amount)?;
    let currency = validate_currency(currency)?;

    Ok(InvoiceCheckoutOutcome {
        invoice_id,
        checkout_url,
        amount,
        currency,
    })
}

/// Validates and maps a Moyasar Payment Intent response into an [`IntentCheckoutOutcome`].
///
/// Validates `intent_id`, enforces [`validate_intent_secret`] on `client_secret`, checks that
/// `amount > 0`, and validates a 3-letter ASCII currency code.
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with `InvalidRequestError` when any input field fails validation.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::checkout_intents::map_intent_result;
///
/// let outcome = map_intent_result("pi_123", "pi_secret_xyz789", 5_000, "SAR")?;
/// assert_eq!(outcome.intent_id, "pi_123");
/// assert_eq!(outcome.amount, 5_000);
/// # Ok::<(), some_moyasar::MoyasarError>(())
/// ```
pub fn map_intent_result(
    intent_id: &str,
    client_secret: &str,
    amount: u64,
    currency: &str,
) -> Result<IntentCheckoutOutcome, MoyasarError> {
    let intent_id = validate_non_empty_id(intent_id, "intent_id")?;
    validate_intent_secret(client_secret)?;
    let amount = validate_positive_amount(amount)?;
    let currency = validate_currency(currency)?;

    Ok(IntentCheckoutOutcome {
        intent_id,
        client_secret: client_secret.to_owned(),
        amount,
        currency,
    })
}

fn matches_intent_not_fulfillable(value: Option<&str>) -> bool {
    value.is_some_and(|raw| raw.trim().eq_ignore_ascii_case(INTENT_NOT_FULFILLABLE_CODE))
}

fn validate_non_empty_id(value: &str, field_name: &str) -> Result<String, MoyasarError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        let message = format!("{field_name} must not be empty");
        return Err(MoyasarError::invalid_request(message));
    }
    Ok(trimmed.to_owned())
}

fn validate_positive_amount(amount: u64) -> Result<u64, MoyasarError> {
    if amount == 0 {
        return Err(MoyasarError::invalid_request(
            "checkout amount must be greater than zero",
        ));
    }
    Ok(amount)
}

fn validate_currency(currency: &str) -> Result<String, MoyasarError> {
    let trimmed = currency.trim();
    if trimmed.len() != 3 || !trimmed.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(MoyasarError::invalid_request(
            "currency must be a 3-letter ASCII ISO 4217 code",
        ));
    }
    Ok(trimmed.to_ascii_uppercase())
}

fn validate_checkout_url(invoice_url: Option<&str>) -> Result<String, MoyasarError> {
    let raw_url = invoice_url
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| MoyasarError::invalid_request("invoice checkout URL is required"))?;

    let remainder = raw_url
        .strip_prefix("https://")
        .or_else(|| raw_url.strip_prefix("http://"))
        .ok_or_else(|| {
            MoyasarError::invalid_request(
                "invoice checkout URL must start with `http://` or `https://`",
            )
        })?;

    let has_whitespace = raw_url.chars().any(char::is_whitespace);
    if remainder.is_empty() || remainder.starts_with('/') || has_whitespace {
        return Err(MoyasarError::invalid_request(
            "invoice checkout URL must include a valid host without whitespace",
        ));
    }

    Ok(raw_url.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_9_1_5_accepts_valid_pi_secret_and_rejects_invalid_variants() {
        assert!(validate_intent_secret("pi_secret_abc123").is_ok());

        let bare_err = validate_intent_secret("pi_secret_").expect_err("bare prefix must fail");
        assert!(!format!("{bare_err} | {bare_err:?}").contains("pi_secret_abc123"));

        let wrong_prefix = "sk_test_super_secret_canary";
        let wrong_err = validate_intent_secret(wrong_prefix).expect_err("wrong prefix must fail");
        let rendered = format!("{wrong_err} | {wrong_err:?}");
        assert!(!rendered.contains(wrong_prefix));

        let whitespace_secret = "pi_secret_abc 123";
        let ws_err =
            validate_intent_secret(whitespace_secret).expect_err("whitespace suffix must fail");
        assert!(!format!("{ws_err} | {ws_err:?}").contains(whitespace_secret));

        assert!(validate_intent_secret(" pi_secret_abc123").is_err());
        assert!(validate_intent_secret("").is_err());
    }

    #[test]
    fn tc_9_1_5_detects_intent_not_fulfillable_case_insensitively() {
        assert!(is_intent_not_fulfillable(Some("intent_not_fulfillable"), None));
        assert!(is_intent_not_fulfillable(Some("INTENT_NOT_FULFILLABLE"), None));
        assert!(is_intent_not_fulfillable(None, Some("Intent_Not_Fulfillable")));
        assert!(is_intent_not_fulfillable(
            Some("api_error"),
            Some(" intent_not_fulfillable "),
        ));
        assert!(!is_intent_not_fulfillable(Some("intent_not_cancelable"), Some("400")));
        assert!(!is_intent_not_fulfillable(None, None));
    }

    #[test]
    fn tc_9_1_5_redacts_intent_client_secret_in_debug_output() {
        let canary = "pi_secret_canary_do_not_leak_991";
        let outcome = map_intent_result("pi_1001", canary, 12_500, "sar")
            .expect("valid intent checkout must succeed");
        assert_eq!(outcome.intent_id, "pi_1001");
        assert_eq!(outcome.client_secret, canary);
        assert_eq!(outcome.amount, 12_500);
        assert_eq!(outcome.currency, "SAR");

        let debug_output = format!("{outcome:?}");
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains(canary));
    }

    #[test]
    fn tc_9_1_5_maps_valid_invoice_url_and_rejects_missing_or_invalid_urls() {
        let https_outcome = map_invoice_checkout(
            "inv_9001",
            Some("https://invoice.moyasar.com/inv_9001"),
            45_000,
            "SAR",
        )
        .expect("valid https invoice URL must succeed");
        assert_eq!(
            https_outcome,
            InvoiceCheckoutOutcome {
                invoice_id: "inv_9001".to_owned(),
                checkout_url: "https://invoice.moyasar.com/inv_9001".to_owned(),
                amount: 45_000,
                currency: "SAR".to_owned(),
            }
        );

        let http_outcome = map_invoice_checkout(
            "inv_9002",
            Some("http://localhost:8080/invoices/inv_9002"),
            100,
            "usd",
        )
        .expect("valid http invoice URL must succeed");
        assert_eq!(http_outcome.currency, "USD");

        let invalid_urls = [
            None,
            Some(""),
            Some("   "),
            Some("ftp://invoice.moyasar.com/1"),
            Some("javascript:alert(1)"),
            Some("https://"),
            Some("https:///path"),
            Some("https://example.com/a b"),
        ];
        for bad_url in invalid_urls {
            assert!(map_invoice_checkout("inv_1", bad_url, 100, "SAR").is_err());
        }
    }

    #[test]
    fn tc_9_1_5_validates_id_amount_and_currency_for_both_mappers() {
        let valid_url = Some("https://invoice.moyasar.com/inv_1");
        assert!(map_invoice_checkout("", valid_url, 100, "SAR").is_err());
        assert!(map_invoice_checkout("inv_1", valid_url, 0, "SAR").is_err());
        assert!(map_invoice_checkout("inv_1", valid_url, 100, "SA").is_err());
        assert!(map_invoice_checkout("inv_1", valid_url, 100, "123").is_err());

        assert!(map_intent_result("", "pi_secret_abc", 100, "SAR").is_err());
        assert!(map_intent_result("pi_1", "pi_secret_", 100, "SAR").is_err());
        assert!(map_intent_result("pi_1", "pi_secret_abc", 0, "SAR").is_err());
        assert!(map_intent_result("pi_1", "pi_secret_abc", 100, "EURO").is_err());
    }
}
