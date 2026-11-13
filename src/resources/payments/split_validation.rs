//! Marketplace payment split validation (`checked_add`, `fee_source`) (US-2.5.0 Unit 2).

use crate::kernel::error::MoyasarError;
use crate::resources::payments::splits::PaymentSplitRequest;

/// Validates a slice of [`PaymentSplitRequest`] entries against the payment `total`.
///
/// Enforces:
/// 1. Every split has `amount > 0` (`MoyasarError::InvalidSplitAmount`).
/// 2. At most one split specifies `fee_source: Some(true)` (`MoyasarError::MultipleFeeSources`).
/// 3. The checked sum of all split amounts does not exceed `total` (`MoyasarError::SplitTotalExceedsAmount`).
pub fn validate_splits(splits: &[PaymentSplitRequest], total: u64) -> Result<(), MoyasarError> {
    if splits.iter().any(|s| s.amount == 0) {
        return Err(MoyasarError::InvalidSplitAmount);
    }
    if splits.iter().filter(|s| s.fee_source == Some(true)).count() > 1 {
        return Err(MoyasarError::MultipleFeeSources);
    }
    let sum = splits
        .iter()
        .try_fold(0u64, |acc, s| acc.checked_add(s.amount))
        .ok_or(MoyasarError::SplitTotalExceedsAmount)?;
    if sum > total {
        return Err(MoyasarError::SplitTotalExceedsAmount);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::payments::splits::SplitRecipientType;
    use uuid::Uuid;

    #[test]
    fn tc_2_5_0_02_rejects_sum_exceeding_total_and_accepts_exact_total() {
        let over = vec![
            PaymentSplitRequest::new(7_000, SplitRecipientType::Beneficiary, Uuid::nil()),
            PaymentSplitRequest::new(5_000, SplitRecipientType::Platform, Uuid::nil()),
        ];
        assert!(matches!(
            validate_splits(&over, 10_000),
            Err(MoyasarError::SplitTotalExceedsAmount)
        ));

        let exact = vec![
            PaymentSplitRequest::new(8_500, SplitRecipientType::Beneficiary, Uuid::nil()),
            PaymentSplitRequest::new(1_500, SplitRecipientType::Platform, Uuid::nil()),
        ];
        assert!(validate_splits(&exact, 10_000).is_ok());
    }

    #[test]
    fn tc_2_5_0_02_checked_add_overflow_is_rejected() {
        let overflow = vec![
            PaymentSplitRequest::new(u64::MAX, SplitRecipientType::Beneficiary, Uuid::nil()),
            PaymentSplitRequest::new(1, SplitRecipientType::Platform, Uuid::nil()),
        ];
        assert!(matches!(
            validate_splits(&overflow, u64::MAX),
            Err(MoyasarError::SplitTotalExceedsAmount)
        ));
    }

    #[test]
    fn tc_2_5_0_03_rejects_zero_split_amount() {
        let zero = vec![PaymentSplitRequest::new(
            0,
            SplitRecipientType::Beneficiary,
            Uuid::nil(),
        )];
        assert!(matches!(
            validate_splits(&zero, 10_000),
            Err(MoyasarError::InvalidSplitAmount)
        ));
    }

    #[test]
    fn rejects_multiple_fee_sources() {
        let multi_fee = vec![
            PaymentSplitRequest::new(5_000, SplitRecipientType::Beneficiary, Uuid::nil())
                .with_fee_source(true),
            PaymentSplitRequest::new(5_000, SplitRecipientType::Platform, Uuid::nil())
                .with_fee_source(true),
        ];
        assert!(matches!(
            validate_splits(&multi_fee, 10_000),
            Err(MoyasarError::MultipleFeeSources)
        ));
    }
}
