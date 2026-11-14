//! Moyasar Webhooks management API service (`/v1/webhooks`).

pub mod attempts;
pub mod crud;
pub mod events;
pub mod models;
pub mod retrieve_delete;

pub use attempts::ListWebhookAttemptsParams;
pub use crud::{CreateWebhookParams, validate_https_url};
pub use models::{WebhookAttempt, WebhookEndpoint, WebhookSecret};

use crate::kernel::auth::AuthKey;
use crate::kernel::client::MoyasarClient;

/// Service handle for Moyasar Webhooks management endpoints (`/v1/webhooks`).
#[derive(Debug, Clone, Copy)]
pub struct WebhooksService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> WebhooksService<'a, K> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a [`WebhooksService`] handle for managing webhook endpoints and attempts.
    #[must_use]
    pub const fn webhooks(&self) -> WebhooksService<'_, K> {
        WebhooksService { client: self }
    }
}
