//! Stripe-compatible virtual services for Moyasar (`RefundsService`, `CustomerVault`, `SubscriptionsService`).

/// Stripe-compatible customer service and metadata helpers.
pub mod customers;
/// In-memory `CustomerVault` implementation for testing and local use.
pub mod in_memory_vault;
/// Deterministic integer-ceiling pro-rata refund calculations.
pub mod pro_rata;
/// Smart 2-hour void vs. refund routing policy.
pub mod refund_strategy;
/// Stripe-compatible refund service and receipt types.
pub mod refunds;
/// Subscription plan intervals and billing period boundary helpers.
pub mod subscription_plan;
/// Subscription cycle billing and idempotent recurring charge orchestration.
pub mod subscriptions_billing;
/// Subscription, cycle, and refund strategy data models.
pub mod subscriptions_model;
/// Subscription cycle refund and void-fallback orchestration.
pub mod subscriptions_refund;

pub use customers::{
    CustomerChargeParams, CustomerVault, CustomersService, inject_customer_metadata,
};
pub use in_memory_vault::InMemoryCustomerVault;
pub use pro_rata::pro_rata_refund;
pub use refund_strategy::{RefundRoute, select_refund_route};
pub use refunds::{RefundCreateParams, RefundReceipt, RefundsService};
pub use subscription_plan::{PlanInterval, SubscriptionPlan, next_period_bounds};
pub use subscriptions_billing::SubscriptionsService;
pub use subscriptions_model::{
    RefundStrategy, ReversalAction, Subscription, SubscriptionCycle, SubscriptionCycleId,
    SubscriptionCycleRefundParams, SubscriptionCycleRefundResult,
};
