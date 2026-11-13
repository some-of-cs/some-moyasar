//! Hosted e-invoice service (`POST /v1/invoices`, `POST /v1/invoices/bulk`, `GET /v1/invoices`, `PUT /v1/invoices/:id`, `PUT /v1/invoices/:id/cancel`).

pub mod cancel;
pub mod create;
pub mod models;
pub mod retrieve_list;
pub mod update;

pub use cancel::{MAX_SADAD_EXPIRY_DAYS, sync_sadad_expiry, validate_invoice_cancelable};
pub use create::CreateInvoiceParams;
pub use models::{Invoice, InvoiceStatus};
pub use retrieve_list::ListInvoicesParams;
pub use update::UpdateInvoiceParams;

use crate::kernel::auth::AuthKey;
use crate::kernel::client::MoyasarClient;

/// Service handle for Moyasar hosted e-invoice operations.
#[derive(Debug, Clone, Copy)]
pub struct InvoicesService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> InvoicesService<'a, K> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns an [`InvoicesService`] handle for hosted invoice endpoints.
    #[must_use]
    pub const fn invoices(&self) -> InvoicesService<'_, K> {
        InvoicesService { client: self }
    }
}
