//! SADAD 29-day maximum expiry validator and sandbox vectors (US-2.4.0 Unit 4).

use chrono::{DateTime, Duration, Utc};

use crate::kernel::error::MoyasarError;

/// Maximum lifetime allowed by SADAD for a generated bill (29 days).
pub const SADAD_MAX_EXPIRY_DAYS: i64 = 29;

/// Sandbox STC Pay mobile number that simulates a successful OTP payment.
pub const SANDBOX_STCPAY_SUCCESS: &str = "0515555555";

/// Sandbox STC Pay mobile number that simulates insufficient funds.
pub const SANDBOX_STCPAY_INSUFFICIENT_FUNDS: &str = "0515555558";

/// Sandbox STC Pay mobile number that simulates a rejected transaction.
pub const SANDBOX_STCPAY_REJECTED: &str = "0515555559";

/// Sandbox SADAD biller code accepted in test mode.
pub const SANDBOX_SADAD_BILLER_CODE: &str = "007";

/// Validates that an optional SADAD bill `expired_at` timestamp does not exceed
/// the 29-day provider limit relative to `now`.
///
/// # Errors
///
/// Returns [`MoyasarError::SadadExpiryTooFar`] if `expired_at` is more than
/// [`SADAD_MAX_EXPIRY_DAYS`] days after `now`.
pub fn validate_sadad_expiry(
    expired_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<(), MoyasarError> {
    if let Some(exp) = expired_at {
        let max_allowed = now + Duration::days(SADAD_MAX_EXPIRY_DAYS);
        if exp > max_allowed {
            return Err(MoyasarError::SadadExpiryTooFar);
        }
    }
    Ok(())
}

/// Validates SADAD bill expiry against both the 29-day provider limit and an
/// optional linked invoice expiration timestamp (SAMA e-invoicing consistency).
///
/// # Errors
///
/// Returns [`MoyasarError::SadadExpiryTooFar`] if `expired_at` exceeds either
/// `now + 29 days` or `invoice_expiry`.
pub fn validate_sadad_expiry_with_invoice(
    expired_at: Option<DateTime<Utc>>,
    invoice_expiry: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<(), MoyasarError> {
    validate_sadad_expiry(expired_at, now)?;
    if let (Some(bill_exp), Some(inv_exp)) = (expired_at, invoice_expiry)
        && bill_exp > inv_exp
    {
        return Err(MoyasarError::SadadExpiryTooFar);
    }
    Ok(())
}

/// Computes the effective SADAD bill expiration timestamp, defaulting to
/// `now + 29 days` and capping at `invoice_expiry` when an invoice expires earlier.
#[must_use]
pub fn effective_sadad_expiry(
    requested: Option<DateTime<Utc>>,
    invoice_expiry: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> DateTime<Utc> {
    let provider_cap = now + Duration::days(SADAD_MAX_EXPIRY_DAYS);
    let base = requested.unwrap_or(provider_cap).min(provider_cap);
    match invoice_expiry {
        Some(inv_exp) => base.min(inv_exp),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_2_4_0_05_rejects_45_days_and_accepts_20_and_29_days() {
        let now = Utc::now();
        let forty_five_days = now + Duration::days(45);
        let err = validate_sadad_expiry(Some(forty_five_days), now)
            .expect_err("45 days must be rejected");
        assert!(matches!(err, MoyasarError::SadadExpiryTooFar));

        let twenty_days = now + Duration::days(20);
        validate_sadad_expiry(Some(twenty_days), now).expect("20 days accepted");

        let exact_cap = now + Duration::days(29);
        validate_sadad_expiry(Some(exact_cap), now).expect("29 days exact accepted");

        validate_sadad_expiry(None, now).expect("None defaults cleanly");
    }

    #[test]
    fn tc_2_4_0_law_enforces_sama_invoice_expiry_consistency() {
        let now = Utc::now();
        let invoice_exp = now + Duration::days(10);
        let bill_exp_too_long = now + Duration::days(15);

        let err =
            validate_sadad_expiry_with_invoice(Some(bill_exp_too_long), Some(invoice_exp), now)
                .expect_err("bill expiry cannot outlive invoice expiry");
        assert!(matches!(err, MoyasarError::SadadExpiryTooFar));

        let effective = effective_sadad_expiry(None, Some(invoice_exp), now);
        assert_eq!(effective, invoice_exp);
        assert!(effective <= now + Duration::days(SADAD_MAX_EXPIRY_DAYS));
    }

    #[test]
    fn exposes_sandbox_test_vectors() {
        assert_eq!(SANDBOX_STCPAY_SUCCESS, "0515555555");
        assert_eq!(SANDBOX_STCPAY_INSUFFICIENT_FUNDS, "0515555558");
        assert_eq!(SANDBOX_STCPAY_REJECTED, "0515555559");
        assert_eq!(SANDBOX_SADAD_BILLER_CODE, "007");
    }
}
