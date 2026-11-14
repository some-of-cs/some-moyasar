//! Moyasar Payouts & Payout Accounts API (`v1/payout_accounts`, `v1/payouts`).
//!
//! Pre-scaffolded module layout for US-5.1.0 (4 units) and US-5.2.0 (6 units).

pub mod accounts_create;
pub mod accounts_list;
pub mod accounts_retrieve;
pub mod destinations;
pub mod models;
pub mod payout_model;
pub mod payouts_bulk;
pub mod payouts_create;
pub mod payouts_list;
pub mod purpose;
pub mod service;

pub use service::{PayoutAccountsService, PayoutsService};
