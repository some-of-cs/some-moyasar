//! gRPC handlers for payments, payment intents, tokens, invoices, and 3DS card auths.

#![allow(clippy::result_large_err)]
#![allow(dead_code)]

use some_moyasar::prelude::{
    Amount, CardAuthId, CreateInvoiceParams, Currency, IntentId, Invoice, MoyasarClient,
    PaymentId, PaymentIntent, PaymentIntentStatus, RefundCreateParams, SecretKey, TokenId,
};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::proto_services::{
    CardAuthRpcResponse, CreateInvoiceRpcRequest, IntentJsonRpcResponse, InvoiceRpcResponse,
    MoyasarCoreResources, PaymentJsonRpcResponse, RetrieveIdRpcRequest, SmartRefundRpcRequest,
    SmartRefundRpcResponse, TokenRpcResponse,
};
use super::status_mapper::to_tonic_status;

const INTENT_STATUS_LABELS: &[(PaymentIntentStatus, &str)] = &[
    (PaymentIntentStatus::Created, "created"),
    (PaymentIntentStatus::Fulfilled, "fulfilled"),
    (PaymentIntentStatus::Canceled, "canceled"),
    (PaymentIntentStatus::Unknown, "unknown"),
];

/// Tonic gRPC service adapter over [`MoyasarClient<SecretKey>`] for core Moyasar resources
/// (`PaymentsService`, `RefundsService`, `PaymentIntentsService`, `TokensService`,
/// `InvoicesService`, and `CardAuthsService`).
#[derive(Debug, Clone)]
pub struct MoyasarCoreResourcesService {
    /// Authenticated Moyasar SDK client used to execute core resource operations.
    client: MoyasarClient<SecretKey>,
}

impl MoyasarCoreResourcesService {
    /// Creates a new [`MoyasarCoreResourcesService`] wrapping `client`.
    #[must_use]
    pub fn new(client: MoyasarClient<SecretKey>) -> Self {
        Self { client }
    }

    /// Returns a reference to the underlying [`MoyasarClient<SecretKey>`].
    #[must_use]
    pub const fn client(&self) -> &MoyasarClient<SecretKey> {
        &self.client
    }
}

/// Validates that a signed proto minor-unit amount is non-negative and converts it to `u64`.
fn parse_non_negative_amount(amount_minor: i64) -> Result<u64, Status> {
    u64::try_from(amount_minor)
        .map_err(|_| Status::invalid_argument("amount_minor must be non-negative"))
}

/// Converts an unsigned minor-unit amount into a signed proto `i64`, failing on overflow.
fn to_proto_amount(amount_minor: u64) -> Result<i64, Status> {
    i64::try_from(amount_minor).map_err(|_| Status::internal("amount exceeds i64 range"))
}

/// Returns the lowercase wire status string for a [`PaymentIntentStatus`].
fn intent_status_str(status: PaymentIntentStatus) -> &'static str {
    INTENT_STATUS_LABELS
        .iter()
        .find(|(candidate, _)| *candidate == status)
        .map_or("unknown", |(_, label)| *label)
}

/// Maps a [`PaymentIntent`] into an [`IntentJsonRpcResponse`].
fn map_intent_response(intent: &PaymentIntent) -> Result<IntentJsonRpcResponse, Status> {
    let usable = intent.is_usable();
    Ok(IntentJsonRpcResponse {
        id: intent.id.as_str().to_owned(),
        status: intent_status_str(intent.status).to_owned(),
        amount_minor: to_proto_amount(intent.amount)?,
        usable,
    })
}

/// Builds [`RefundCreateParams`] with unsettled-void preference and optional partial amount.
fn build_refund_params(req: SmartRefundRpcRequest) -> Result<RefundCreateParams, Status> {
    let mut params = RefundCreateParams::new(req.payment_id).prefer_void_if_unsettled(true);
    if let Some(amount_minor) = req.amount_minor {
        params = params.amount(parse_non_negative_amount(amount_minor)?);
    }
    Ok(params)
}

/// Parses a three-letter ISO 4217 [`Currency`] code, mapping errors to [`Status`].
fn parse_currency(raw: &str) -> Result<Currency, Status> {
    raw.parse::<Currency>().map_err(|err| to_tonic_status(&err))
}

/// Builds [`CreateInvoiceParams`] from a [`CreateInvoiceRpcRequest`].
fn build_invoice_params(req: CreateInvoiceRpcRequest) -> Result<CreateInvoiceParams, Status> {
    if req.amount_minor < 0 {
        return Err(Status::invalid_argument(
            "amount_minor must be non-negative",
        ));
    }
    let amount = Amount::from_minor(req.amount_minor);
    let currency = parse_currency(&req.currency)?;
    let description = req.description.unwrap_or_else(|| "Invoice".to_owned());
    Ok(CreateInvoiceParams::new(amount, currency).description(description))
}

/// Maps an [`Invoice`] domain model into an [`InvoiceRpcResponse`].
fn map_invoice_response(inv: Invoice) -> InvoiceRpcResponse {
    InvoiceRpcResponse {
        id: inv.id.to_string(),
        status: inv.status.as_str().to_owned(),
        amount_minor: inv.amount.minor_units(),
        currency: inv.currency.as_str().to_owned(),
    }
}

/// Parses a raw invoice UUID string, returning `INVALID_ARGUMENT` if malformed.
fn parse_invoice_uuid(raw: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|_| Status::invalid_argument("invoice id must be a valid UUID"))
}

#[tonic::async_trait]
impl MoyasarCoreResources for MoyasarCoreResourcesService {
    /// Retrieves a payment by ID via [`MoyasarClient::payments`].
    async fn retrieve_payment(
        &self,
        request: Request<RetrieveIdRpcRequest>,
    ) -> Result<Response<PaymentJsonRpcResponse>, Status> {
        let req = request.into_inner();
        let id = PaymentId::new(req.id).map_err(|err| to_tonic_status(&err))?;
        let p = self
            .client()
            .payments()
            .retrieve(&id)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(PaymentJsonRpcResponse {
            id: p.id,
            status: p.status.as_str().to_owned(),
            amount_minor: to_proto_amount(p.amount)?,
            currency: p.currency.as_str().to_owned(),
        }))
    }

    /// Refunds a captured payment or voids an unsettled payment via [`MoyasarClient::refunds`].
    async fn smart_refund(
        &self,
        request: Request<SmartRefundRpcRequest>,
    ) -> Result<Response<SmartRefundRpcResponse>, Status> {
        let req = request.into_inner();
        let params = build_refund_params(req)?;
        let receipt = self
            .client()
            .refunds()
            .create(params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(SmartRefundRpcResponse {
            payment_id: receipt.payment_id,
            action: receipt.status.as_str().to_owned(),
            amount_minor: to_proto_amount(receipt.refunded_amount)?,
        }))
    }

    /// Retrieves a payment intent by ID via [`MoyasarClient::payment_intents`].
    async fn retrieve_intent(
        &self,
        request: Request<RetrieveIdRpcRequest>,
    ) -> Result<Response<IntentJsonRpcResponse>, Status> {
        let req = request.into_inner();
        let id = IntentId::new(req.id).map_err(|err| to_tonic_status(&err))?;
        let intent = self
            .client()
            .payment_intents()
            .retrieve(&id)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(map_intent_response(&intent)?))
    }

    /// Cancels an active payment intent by ID via [`MoyasarClient::payment_intents`].
    async fn cancel_intent(
        &self,
        request: Request<RetrieveIdRpcRequest>,
    ) -> Result<Response<IntentJsonRpcResponse>, Status> {
        let req = request.into_inner();
        let id = IntentId::new(req.id).map_err(|err| to_tonic_status(&err))?;
        let intent = self
            .client()
            .payment_intents()
            .cancel(&id)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(map_intent_response(&intent)?))
    }

    /// Retrieves a vaulted card token by ID via [`MoyasarClient::tokens`].
    async fn retrieve_token(
        &self,
        request: Request<RetrieveIdRpcRequest>,
    ) -> Result<Response<TokenRpcResponse>, Status> {
        let req = request.into_inner();
        let id = TokenId::new(req.id).map_err(|err| to_tonic_status(&err))?;
        let t = self
            .client()
            .tokens()
            .retrieve(&id)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(TokenRpcResponse {
            id: t.id.as_str().to_owned(),
            status: t.status.as_str().to_owned(),
            brand: t.company.unwrap_or_default(),
            deleted: false,
        }))
    }

    /// Deletes a vaulted card token by ID via [`MoyasarClient::tokens`].
    async fn delete_token(
        &self,
        request: Request<RetrieveIdRpcRequest>,
    ) -> Result<Response<TokenRpcResponse>, Status> {
        let req = request.into_inner();
        let id = TokenId::new(req.id).map_err(|err| to_tonic_status(&err))?;
        self.client()
            .tokens()
            .delete(&id)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(TokenRpcResponse {
            id: id.as_str().to_owned(),
            status: "deleted".to_owned(),
            brand: String::new(),
            deleted: true,
        }))
    }

    /// Creates a hosted e-invoice in minor currency units via [`MoyasarClient::invoices`].
    async fn create_invoice(
        &self,
        request: Request<CreateInvoiceRpcRequest>,
    ) -> Result<Response<InvoiceRpcResponse>, Status> {
        let req = request.into_inner();
        let params = build_invoice_params(req)?;
        let inv = self
            .client()
            .invoices()
            .create(&params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(map_invoice_response(inv)))
    }

    /// Cancels an active hosted e-invoice by UUID via [`MoyasarClient::invoices`].
    async fn cancel_invoice(
        &self,
        request: Request<RetrieveIdRpcRequest>,
    ) -> Result<Response<InvoiceRpcResponse>, Status> {
        let req = request.into_inner();
        let uuid = parse_invoice_uuid(&req.id)?;
        let inv = self
            .client()
            .invoices()
            .cancel(uuid)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(map_invoice_response(inv)))
    }

    /// Retrieves a standalone 3DS card authentication session by ID, redacting CAVV values.
    async fn retrieve_card_auth(
        &self,
        request: Request<RetrieveIdRpcRequest>,
    ) -> Result<Response<CardAuthRpcResponse>, Status> {
        let req = request.into_inner();
        let id = CardAuthId::new(req.id).map_err(|err| to_tonic_status(&err))?;
        let auth = self
            .client()
            .card_auths()
            .retrieve(&id)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(CardAuthRpcResponse {
            id: auth.id.as_str().to_owned(),
            status: auth.status.as_str().to_owned(),
            transaction_url: auth.transaction_url,
        }))
    }
}

#[cfg(test)]
mod tests {
    use tonic::Code;

    use super::*;

    #[test]
    fn helpers_validate_amounts_currency_and_uuid() {
        assert_eq!(parse_non_negative_amount(0).expect("zero valid"), 0);
        assert_eq!(
            parse_non_negative_amount(-1)
                .expect_err("negative invalid")
                .code(),
            Code::InvalidArgument
        );
        assert_eq!(to_proto_amount(10_000).expect("valid amount"), 10_000);
        assert_eq!(
            to_proto_amount(u64::MAX).expect_err("overflow").code(),
            Code::Internal
        );
        assert_eq!(parse_currency("SAR").expect("valid currency"), Currency::SAR);
        assert_eq!(
            parse_currency("INVALID").expect_err("bad currency").code(),
            Code::InvalidArgument
        );
        assert!(parse_invoice_uuid("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert_eq!(
            parse_invoice_uuid("bad-uuid").expect_err("bad uuid").code(),
            Code::InvalidArgument
        );
    }

    #[test]
    fn builders_and_intent_status_labels_map_expected_fields() {
        assert_eq!(intent_status_str(PaymentIntentStatus::Created), "created");
        assert_eq!(
            intent_status_str(PaymentIntentStatus::Fulfilled),
            "fulfilled"
        );
        assert_eq!(intent_status_str(PaymentIntentStatus::Canceled), "canceled");
        assert_eq!(intent_status_str(PaymentIntentStatus::Unknown), "unknown");

        let refund = build_refund_params(SmartRefundRpcRequest {
            payment_id: "pay_123".into(),
            amount_minor: Some(5_000),
        })
        .expect("valid refund params");
        assert_eq!(refund.payment_id, "pay_123");
        assert_eq!(refund.amount, Some(5_000));
        assert!(refund.prefer_void_if_unsettled);

        let inv = build_invoice_params(CreateInvoiceRpcRequest {
            amount_minor: 25_000,
            currency: "SAR".into(),
            description: None,
        })
        .expect("valid invoice params");
        assert_eq!(inv.amount.minor_units(), 25_000);
        assert_eq!(inv.currency, Currency::SAR);
        assert_eq!(inv.description.as_deref(), Some("Invoice"));
    }
}
