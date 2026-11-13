//! Card tokenization service (`POST /v1/tokens`, `GET /v1/tokens/:id`, `DELETE /v1/tokens/:id`).

pub mod create;
pub mod delete;
pub mod models;
pub mod retrieve;

use crate::kernel::auth::AuthKey;
use crate::kernel::client::MoyasarClient;

/// Service handle for Moyasar card token operations.
#[derive(Debug, Clone, Copy)]
pub struct TokensService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> TokensService<'a, K> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a [`TokensService`] handle for card tokenization endpoints.
    #[must_use]
    pub const fn tokens(&self) -> TokensService<'_, K> {
        TokensService { client: self }
    }
}
