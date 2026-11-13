//! Service facade for Moyasar Payment Intents endpoints.

use crate::kernel::auth::AuthKey;
use crate::kernel::client::MoyasarClient;

/// Service handle for `/v1/payment_intents` operations.
#[derive(Debug, Clone, Copy)]
pub struct PaymentIntentsService<'a, K: AuthKey> {
    pub(crate) client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> PaymentIntentsService<'a, K> {
    /// Creates a new [`PaymentIntentsService`] bound to `client`.
    pub const fn new(client: &'a MoyasarClient<K>) -> Self {
        Self { client }
    }

    /// Returns a reference to the underlying [`MoyasarClient`].
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a [`PaymentIntentsService`] for managing and fulfilling payment intents.
    pub const fn payment_intents(&self) -> PaymentIntentsService<'_, K> {
        PaymentIntentsService::new(self)
    }
}
