//! Service facades for Settlements, Transfers, and Internal Transactions.

use crate::kernel::auth::AuthKey;
use crate::kernel::client::MoyasarClient;

/// Service handle for Moyasar settlements and merchant balance endpoints (`/v1/settlements`).
#[derive(Debug, Clone, Copy)]
pub struct SettlementsService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> SettlementsService<'a, K> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

/// Service handle for Moyasar aggregation transfer endpoints (`/v1/transfers`).
#[derive(Debug, Clone, Copy)]
pub struct TransfersService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> TransfersService<'a, K> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

/// Service handle for Moyasar internal transaction ledger endpoints (`/v1/internal_transactions`).
#[derive(Debug, Clone, Copy)]
pub struct InternalTransactionsService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> InternalTransactionsService<'a, K> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a [`SettlementsService`] handle for settlement and balance endpoints.
    #[must_use]
    pub const fn settlements(&self) -> SettlementsService<'_, K> {
        SettlementsService { client: self }
    }

    /// Returns a [`TransfersService`] handle for aggregation transfer endpoints.
    #[must_use]
    pub const fn transfers(&self) -> TransfersService<'_, K> {
        TransfersService { client: self }
    }

    /// Returns an [`InternalTransactionsService`] handle for internal transaction ledger endpoints.
    #[must_use]
    pub const fn internal_transactions(&self) -> InternalTransactionsService<'_, K> {
        InternalTransactionsService { client: self }
    }
}
