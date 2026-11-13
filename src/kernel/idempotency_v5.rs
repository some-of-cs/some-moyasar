//! Deterministic UUIDv5 idempotency key derivation for recurring billing cycles.

use uuid::Uuid;

/// Fixed UUIDv5 namespace for `some-moyasar` subscription cycle idempotency.
pub const MOYASAR_SUBSCRIPTION_NAMESPACE: Uuid =
    Uuid::from_u128(0x6ba7_b810_9dad_11d1_80b4_00c0_4fd4_30c8);

/// Derives a deterministic `given_id` for a subscription billing cycle.
///
/// # Examples
///
/// ```
/// use some_moyasar::prelude::derive_subscription_given_id;
///
/// let first = derive_subscription_given_id("sub_42", 7);
/// let second = derive_subscription_given_id("sub_42", 7);
/// assert_eq!(first, second);
/// ```
pub fn derive_subscription_given_id(sub_id: &str, cycle_index: u64) -> Uuid {
    let scope = format!("{sub_id}:{cycle_index}");
    Uuid::new_v5(&MOYASAR_SUBSCRIPTION_NAMESPACE, scope.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_across_repeated_calls() {
        let first = derive_subscription_given_id("sub_42", 7);
        let second = derive_subscription_given_id("sub_42", 7);
        let next = derive_subscription_given_id("sub_42", 8);
        assert_eq!(first, second);
        assert_ne!(first, next);
    }
}
