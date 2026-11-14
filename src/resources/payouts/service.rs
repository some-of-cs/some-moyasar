//! Zero-clone `PayoutAccountsService` and `PayoutsService` handles (`SecretAuthKey` bound).

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::client::MoyasarClient;

/// Borrowed service handle for Moyasar Payout Accounts (`v1/payout_accounts`).
#[derive(Debug, Clone, Copy)]
pub struct PayoutAccountsService<'a, K: SecretAuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: SecretAuthKey> PayoutAccountsService<'a, K> {
    /// Constructs a new [`PayoutAccountsService`] borrowing the given client.
    pub const fn new(client: &'a MoyasarClient<K>) -> Self {
        Self { client }
    }

    /// Returns a reference to the underlying [`MoyasarClient`].
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

/// Borrowed service handle for Moyasar Payout Disbursements (`v1/payouts`).
#[derive(Debug, Clone, Copy)]
pub struct PayoutsService<'a, K: SecretAuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: SecretAuthKey> PayoutsService<'a, K> {
    /// Constructs a new [`PayoutsService`] borrowing the given client.
    pub const fn new(client: &'a MoyasarClient<K>) -> Self {
        Self { client }
    }

    /// Returns a reference to the underlying [`MoyasarClient`].
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: SecretAuthKey> MoyasarClient<K> {
    /// Returns a zero-clone [`PayoutAccountsService`] handle for managing payout source accounts.
    ///
    /// Restricted to [`SecretAuthKey`] clients (`sk_live_*` / `sk_test_*`).
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    ///
    /// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let _svc = client.payout_accounts();
    /// ```
    ///
    /// Calling `payout_accounts()` with a [`PublishableKey`](crate::kernel::auth::PublishableKey)
    /// fails to compile:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    ///
    /// let key = PublishableKey::new("pk_test_YOUR_KEY").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let _svc = client.payout_accounts();
    /// ```
    pub fn payout_accounts(&self) -> PayoutAccountsService<'_, K> {
        PayoutAccountsService::new(self)
    }

    /// Returns a zero-clone [`PayoutsService`] handle for single and bulk disbursements.
    ///
    /// Restricted to [`SecretAuthKey`] clients (`sk_live_*` / `sk_test_*`).
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    ///
    /// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let _svc = client.payouts();
    /// ```
    ///
    /// Calling `payouts()` with a [`PublishableKey`](crate::kernel::auth::PublishableKey)
    /// fails to compile:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    ///
    /// let key = PublishableKey::new("pk_test_YOUR_KEY").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let _svc = client.payouts();
    /// ```
    pub fn payouts(&self) -> PayoutsService<'_, K> {
        PayoutsService::new(self)
    }
}
