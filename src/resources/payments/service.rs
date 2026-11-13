//! Zero-clone `PaymentsService` handle and `MoyasarClient::payments()` entry point.

use crate::kernel::auth::AuthKey;
use crate::kernel::client::MoyasarClient;

/// Borrowed service handle for Moyasar Payments API operations (`v1/payments`).
///
/// Implements `Copy` so obtaining `client.payments()` incurs zero heap allocation.
#[derive(Debug, Clone, Copy)]
pub struct PaymentsService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> PaymentsService<'a, K> {
    /// Constructs a new `PaymentsService` borrowing the given client.
    pub const fn new(client: &'a MoyasarClient<K>) -> Self {
        Self { client }
    }

    /// Returns a reference to the underlying `MoyasarClient`.
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a zero-clone [`PaymentsService`] handle for payment operations.
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    ///
    /// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let _payments = client.payments();
    /// ```
    pub fn payments(&self) -> PaymentsService<'_, K> {
        PaymentsService::new(self)
    }
}
