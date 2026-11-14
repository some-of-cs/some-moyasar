//! `SubscriptionPlan`, `PlanInterval`, and UTC billing period bounds calculator.

use chrono::{DateTime, Datelike, Duration, Months, Utc};
use serde::{Deserialize, Serialize};

use crate::kernel::types::Currency;

/// Recurring billing cadence for a [`SubscriptionPlan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanInterval {
    /// Daily billing cycle.
    Day,
    /// Weekly billing cycle (7 days).
    Week,
    /// Calendar monthly billing cycle.
    Month,
    /// Calendar annual billing cycle.
    Year,
}

/// Reusable subscription pricing plan definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionPlan {
    /// Merchant plan identifier.
    pub id: String,
    /// Minor-unit recurring amount.
    pub amount: u64,
    /// ISO 4217 currency.
    pub currency: Currency,
    /// Billing cadence.
    pub interval: PlanInterval,
}

impl SubscriptionPlan {
    /// Creates a new [`SubscriptionPlan`].
    #[must_use]
    pub fn new(id: impl Into<String>, amount: u64, currency: Currency, interval: PlanInterval) -> Self {
        Self {
            id: id.into(),
            amount,
            currency,
            interval,
        }
    }

    /// Computes `(current_period_start, current_period_end)` starting at `period_start`.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    /// use some_moyasar::prelude::Currency;
    /// use some_moyasar::stripe_compat::subscription_plan::{PlanInterval, SubscriptionPlan};
    ///
    /// let plan = SubscriptionPlan::new("plan_pro", 29_900, Currency::SAR, PlanInterval::Month);
    /// let start = Utc.with_ymd_and_hms(2024, 1, 31, 0, 0, 0).unwrap();
    /// let (s, e) = plan.next_period_bounds(start);
    /// assert_eq!(s, start);
    /// assert_eq!(e, Utc.with_ymd_and_hms(2024, 2, 29, 0, 0, 0).unwrap());
    /// ```
    #[must_use]
    pub fn next_period_bounds(&self, period_start: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
        next_period_bounds(period_start, self.interval)
    }
}

/// Computes exact UTC `(period_start, period_end)` for `interval`.
#[must_use]
pub fn next_period_bounds(
    period_start: DateTime<Utc>,
    interval: PlanInterval,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let end = match interval {
        PlanInterval::Day => period_start + Duration::days(1),
        PlanInterval::Week => period_start + Duration::weeks(1),
        PlanInterval::Month => period_start.checked_add_months(Months::new(1)).unwrap_or(period_start + Duration::days(30)),
        PlanInterval::Year => period_start.checked_add_months(Months::new(12)).unwrap_or_else(|| {
            let days = if is_leap_year(period_start.year()) { 366 } else { 365 };
            period_start + Duration::days(days)
        }),
    };
    (period_start, end)
}

const fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn tc_8_2_0_05_computes_bounds_across_all_intervals_and_leap_years() {
        let start = Utc.with_ymd_and_hms(2024, 1, 31, 12, 0, 0).unwrap();
        let (_, day_end) = next_period_bounds(start, PlanInterval::Day);
        assert_eq!(day_end, Utc.with_ymd_and_hms(2024, 2, 1, 12, 0, 0).unwrap());

        let (_, week_end) = next_period_bounds(start, PlanInterval::Week);
        assert_eq!(week_end, Utc.with_ymd_and_hms(2024, 2, 7, 12, 0, 0).unwrap());

        let (_, month_end) = next_period_bounds(start, PlanInterval::Month);
        assert_eq!(month_end, Utc.with_ymd_and_hms(2024, 2, 29, 12, 0, 0).unwrap());

        let (_, year_end) = next_period_bounds(start, PlanInterval::Year);
        assert_eq!(year_end, Utc.with_ymd_and_hms(2025, 1, 31, 12, 0, 0).unwrap());
    }
}
