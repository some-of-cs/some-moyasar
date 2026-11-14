//! Moyasar Payouts & Payout Accounts API (`v1/payout_accounts`, `v1/payouts`).

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

pub use accounts_create::CreatePayoutAccountParams;
pub use accounts_list::ListPayoutAccountsParams;
pub use accounts_retrieve::validate_sa_iban;
pub use destinations::{BankDestination, PayoutDestination, WalletDestination, validate_sa_mobile};
pub use models::{PayoutAccount, PayoutAccountId, PayoutAccountProperties, PayoutAccountType};
pub use payout_model::{Payout, PayoutChannel, PayoutId, PayoutStatus};
pub use payouts_bulk::{BulkPayoutEntry, BulkPayoutResponse, CreateBulkPayoutParams};
pub use payouts_create::{
    CreatePayoutParams, PAYOUT_SEQUENCE_NUMBER_LEN, validate_sequence_number,
};
pub use payouts_list::ListPayoutsParams;
pub use purpose::{KNOWN_PURPOSES, ParsePayoutPurposeError, PayoutPurpose};
pub use service::{PayoutAccountsService, PayoutsService};
