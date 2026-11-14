//! Stripe-compatible virtual services for Moyasar (`RefundsService`, `CustomerVault`, `SubscriptionsService`).

pub mod customers;
pub mod in_memory_vault;
pub mod pro_rata;
pub mod refund_strategy;
pub mod refunds;
pub mod subscription_plan;
pub mod subscriptions_billing;
pub mod subscriptions_model;
pub mod subscriptions_refund;
