//! Feature-gated Moyasar API resource services.

#[cfg(feature = "payments")]
pub mod payments;

#[cfg(feature = "payment-intents")]
pub mod payment_intents;

#[cfg(feature = "tokens")]
pub mod tokens;

#[cfg(feature = "invoices")]
pub mod invoices;

#[cfg(feature = "card-auths")]
pub mod card_auths;
