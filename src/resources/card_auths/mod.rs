//! Standalone 3D Secure card authentication service (`POST /v1/card_auths`, `GET /v1/card_auths/:id`).

pub mod create;
pub mod models;
pub mod retrieve;

use crate::kernel::auth::AuthKey;
use crate::kernel::client::MoyasarClient;

/// Service handle for Moyasar standalone 3DS card authentication endpoints.
#[derive(Debug, Clone, Copy)]
pub struct CardAuthsService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> CardAuthsService<'a, K> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a [`CardAuthsService`] handle for standalone 3DS card authentication endpoints.
    #[must_use]
    pub const fn card_auths(&self) -> CardAuthsService<'_, K> {
        CardAuthsService { client: self }
    }
}
