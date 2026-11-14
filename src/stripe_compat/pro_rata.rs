//! Pure 128-bit integer ceiling pro-rata refund calculator.

use chrono::{DateTime, Utc};

use crate::kernel::error::MoyasarError;

/// Computes `ceil(amount * remaining_secs / total_secs)` using pure `u128` integer math.
///
/// Zero floating-point (`f32`/`f64`) arithmetic is used, ensuring exact halala precision
/// and consumer-favorable ceiling rounding for SAMA recurring-payment compliance.
///
/// # Examples
///
/// ```
/// use chrono::{Duration, TimeZone, Utc};
/// use some_moyasar::stripe_compat::pro_rata::pro_rata_refund;
///
/// let start = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
/// let end = start + Duration::days(30);
/// let canceled = start + Duration::days(10);
/// let refund = pro_rata_refund(29_900, start, end, canceled).unwrap();
/// assert_eq!(refund, 19_934);
/// ```
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidPayoutAmount`] when `period_end <= period_start`.
pub fn pro_rata_refund(
    amount: u64,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    canceled_at: DateTime<Utc>,
) -> Result<u64, MoyasarError> {
    let total = (period_end - period_start).num_seconds();
    if total <= 0 {
        return Err(MoyasarError::InvalidPayoutAmount);
    }
    let remaining = (period_end - canceled_at).num_seconds().clamp(0, total);
    if remaining == 0 {
        return Ok(0);
    }
    let total_u128 = u128::try_from(total).map_err(|_| MoyasarError::InvalidPayoutAmount)?;
    let rem_u128 = u128::try_from(remaining).map_err(|_| MoyasarError::InvalidPayoutAmount)?;
    let num = u128::from(amount).saturating_mul(rem_u128);
    let ceil_val = num.div_ceil(total_u128);
    u64::try_from(ceil_val).map_err(|_| MoyasarError::InvalidPayoutAmount)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    #[test]
    fn tc_8_2_0_03_thirty_day_365_day_and_leap_year_pro_rata_ceiling() {
        let start_30 = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let end_30 = start_30 + Duration::days(30);
        assert_eq!(
            pro_rata_refund(29_900, start_30, end_30, start_30 + Duration::days(10)).unwrap(),
            19_934
        );

        let start_365 = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let end_365 = start_365 + Duration::days(365);
        assert_eq!(
            pro_rata_refund(36_500, start_365, end_365, start_365 + Duration::days(100)).unwrap(),
            26_500
        );

        let leap_start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let leap_end = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        assert_eq!((leap_end - leap_start).num_days(), 366);
        assert_eq!(
            pro_rata_refund(36_600, leap_start, leap_end, leap_start + Duration::days(1)).unwrap(),
            36_500
        );
    }

    #[test]
    fn handles_boundary_and_large_u64_without_overflow() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let end = start + Duration::days(30);
        assert_eq!(pro_rata_refund(10_000, start, end, end).unwrap(), 0);
        assert_eq!(pro_rata_refund(10_000, start, end, start - Duration::days(1)).unwrap(), 10_000);
        let large = u64::MAX / 2;
        assert_eq!(pro_rata_refund(large, start, end, start + Duration::days(15)).unwrap(), large.div_ceil(2));
        assert!(pro_rata_refund(10_000, end, start, start).is_err());
    }
}
