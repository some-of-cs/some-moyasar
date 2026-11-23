//! Settlement net-amount and line-item reconciliation helpers.

use crate::kernel::error::MoyasarError;
use crate::kernel::types::Amount;
use crate::resources::settlements::lines::SettlementLine;
use crate::resources::settlements::models::Settlement;

/// Reconciles a [`Settlement`] against its constituent [`SettlementLine`] items using checked
/// `i64` minor-unit arithmetic (`sum(line.amount - line.fee - line.tax) == settlement.checked_net()`).
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] (`InvalidRequestError`) if arithmetic overflow/underflow occurs,
/// if any line currency differs from `settlement.currency`, or if the summed line net amounts do
/// not match the settlement's net amount.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::types::{Amount, Currency};
/// use some_moyasar::resources::settlements::lines::SettlementLine;
/// use some_moyasar::resources::settlements::models::Settlement;
/// use some_moyasar::resources::settlements::reconciliation::reconcile_settlement;
///
/// let settlement: Settlement = serde_json::from_str(
///     r#"{
///         "id": "stl_01J9XYZ",
///         "recipient_type": "entity",
///         "currency": "SAR",
///         "source_currency": "SAR",
///         "amount": 10000,
///         "fee": 200,
///         "tax": 30,
///         "created_at": "2026-11-14T00:00:00Z"
///     }"#,
/// )
/// .unwrap();
/// let line = SettlementLine {
///     payment_id: None,
///     line_type: Some("payment".to_owned()),
///     currency: Currency::SAR,
///     amount: Amount::from_minor(10_000),
///     fee: Amount::from_minor(200),
///     tax: Amount::from_minor(30),
///     reference: None,
///     settled_at: None,
/// };
/// assert_eq!(
///     reconcile_settlement(&settlement, &[line]).unwrap(),
///     Amount::from_minor(9_770)
/// );
/// ```
pub fn reconcile_settlement(
    settlement: &Settlement,
    lines: &[SettlementLine],
) -> Result<Amount, MoyasarError> {
    let expected_net = settlement.checked_net().ok_or_else(|| {
        MoyasarError::invalid_request("settlement net calculation overflowed or underflowed")
    })?;
    let mut sum: i64 = 0;
    for line in lines {
        if line.currency != settlement.currency {
            return Err(MoyasarError::invalid_request(
                "settlement line currency does not match settlement currency",
            ));
        }
        let line_net = checked_line_net(line).ok_or_else(|| {
            MoyasarError::invalid_request("settlement line net calculation overflowed")
        })?;
        sum = sum
            .checked_add(line_net)
            .ok_or_else(|| MoyasarError::invalid_request("settlement lines sum overflowed i64"))?;
    }
    if Amount::from_minor(sum) != expected_net {
        return Err(MoyasarError::invalid_request(
            "settlement lines net sum does not match settlement net amount",
        ));
    }
    Ok(expected_net)
}

fn checked_line_net(line: &SettlementLine) -> Option<i64> {
    line.amount
        .minor_units()
        .checked_sub(line.fee.minor_units())
        .and_then(|v| v.checked_sub(line.tax.minor_units()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::error::ApiErrorType;
    use crate::kernel::types::Currency;

    #[test]
    fn rejects_settlement_line_with_mismatched_currency() {
        let settlement: Settlement = serde_json::from_str(
            r#"{
                "id": "stl_currency_check",
                "recipient_type": "entity",
                "currency": "SAR",
                "source_currency": "SAR",
                "amount": 10000,
                "fee": 200,
                "tax": 30,
                "created_at": "2026-11-14T00:00:00Z"
            }"#,
        )
        .expect("valid settlement fixture");
        let mismatched_line = SettlementLine {
            payment_id: None,
            line_type: Some("payment".to_owned()),
            currency: Currency::USD,
            amount: Amount::from_minor(10_000),
            fee: Amount::from_minor(200),
            tax: Amount::from_minor(30),
            reference: None,
            settled_at: None,
        };

        let err = reconcile_settlement(&settlement, &[mismatched_line])
            .expect_err("mismatched currency must fail reconciliation");
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }
}
