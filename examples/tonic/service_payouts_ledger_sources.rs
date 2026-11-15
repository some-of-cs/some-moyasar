//! gRPC handlers for payouts, settlements, transfers, internal transactions, sources, Apple Pay,
//! webhooks API, customers, and subscriptions.

#![allow(clippy::result_large_err)]
#![allow(dead_code)]

use std::sync::Arc;

use some_moyasar::kernel::{Currency, MoyasarClient, PublishableKey, SecretKey};
use some_moyasar::resources::payouts::accounts_list::ListPayoutAccountsParams;
use some_moyasar::resources::payouts::destinations::{BankDestination, PayoutDestination};
use some_moyasar::resources::payouts::models::PayoutAccountId;
use some_moyasar::resources::payouts::payouts_create::CreatePayoutParams;
use some_moyasar::resources::payouts::purpose::PayoutPurpose;
use some_moyasar::resources::settlements::internal_transactions::ListInternalTransactionsParams;
use some_moyasar::resources::settlements::models::SettlementId;
use some_moyasar::resources::settlements::transfers::ListTransfersParams;
use some_moyasar::resources::sources::{BinLookupParams, InitiateApplePayParams};
use some_moyasar::resources::tokens::TokenId;
use some_moyasar::resources::webhooks_api::{CreateWebhookParams, WebhookSecret};
use some_moyasar::stripe_compat::customers::{CustomerChargeParams, InMemoryCustomerVault};
use some_moyasar::stripe_compat::subscriptions_billing::SubscriptionCycleChargeParams;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::proto_services::*;
use super::status_mapper::to_tonic_status;

const DEFAULT_TONIC_PUBLISHABLE_KEY: &str = "pk_test_tonic_key";
const DEFAULT_SUBSCRIPTION_ID: &str = "sub_tonic_01";

/// Tonic gRPC service adapter over [`MoyasarClient<SecretKey>`],
/// [`MoyasarClient<PublishableKey>`], and [`InMemoryCustomerVault`] for Moyasar platform
/// resources (`PayoutAccountsService`, `PayoutsService`, `SettlementsService`,
/// `TransfersService`, `InternalTransactionsService`, `SourcesService`, `ApplePayService`,
/// `WebhooksService`, `CustomersService`, and `SubscriptionsService`).
#[derive(Debug, Clone)]
pub struct MoyasarPlatformResourcesService {
    /// Secret-key authenticated Moyasar SDK client for server-side resource operations.
    client: MoyasarClient<SecretKey>,
    /// Publishable-key authenticated Moyasar SDK client for Apple Pay session initiation.
    publishable_client: MoyasarClient<PublishableKey>,
    /// Thread-safe in-memory customer token vault for Stripe-compatible customer operations.
    vault: Arc<InMemoryCustomerVault>,
}

impl MoyasarPlatformResourcesService {
    /// Creates a new [`MoyasarPlatformResourcesService`] from a [`MoyasarClient<SecretKey>`],
    /// initializing a matching publishable-key client at `client.base_url()` and an empty
    /// [`InMemoryCustomerVault`].
    #[must_use]
    pub fn new(client: MoyasarClient<SecretKey>) -> Self {
        let pk = PublishableKey::new(DEFAULT_TONIC_PUBLISHABLE_KEY)
            .expect("valid default tonic publishable key");
        let publishable_client = MoyasarClient::publishable_builder(pk)
            .base_url(client.base_url())
            .build()
            .expect("publishable client builds from validated secret client base_url");
        Self {
            client,
            publishable_client,
            vault: Arc::new(InMemoryCustomerVault::new()),
        }
    }
}

/// Validates that a signed proto minor-unit amount is non-negative and converts it to `u64`.
fn parse_non_negative_amount(amount_minor: i64) -> Result<u64, Status> {
    u64::try_from(amount_minor)
        .map_err(|_| Status::invalid_argument("amount_minor must be non-negative"))
}

/// Serializes a domain model or collection into a JSON string, mapping errors to `INTERNAL`.
fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String, Status> {
    serde_json::to_string(value)
        .map_err(|err| Status::internal(format!("failed to serialize response JSON: {err}")))
}

/// Parses a three-letter ISO 4217 [`Currency`] code, mapping errors via [`to_tonic_status`].
fn parse_currency(raw: &str) -> Result<Currency, Status> {
    raw.parse::<Currency>().map_err(|err| to_tonic_status(&err))
}

/// Parses a caller-supplied UUID `given_id`, returning `INVALID_ARGUMENT` if malformed.
fn parse_given_id(raw: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|_| Status::invalid_argument("given_id must be a valid UUID"))
}

/// Validates and builds [`CreatePayoutParams`] for a Saudi bank IBAN transfer in Riyadh.
fn build_payout_params(req: CreatePayoutRpcRequest) -> Result<CreatePayoutParams, Status> {
    let source_id = PayoutAccountId::new(req.source_id).map_err(|err| to_tonic_status(&err))?;
    let amount = parse_non_negative_amount(req.amount_minor)?;
    let bank = BankDestination {
        iban: req.iban,
        name: req.name,
        mobile: req.mobile,
        country: "SA".to_owned(),
        city: "Riyadh".to_owned(),
    };
    let params = CreatePayoutParams {
        source_id,
        sequence_number: None,
        amount,
        purpose: PayoutPurpose::PaymentToMerchant,
        destination: PayoutDestination::Bank(bank),
        comment: None,
        metadata: None,
    };
    params.validate().map_err(|err| to_tonic_status(&err))?;
    Ok(params)
}

#[tonic::async_trait]
impl MoyasarPlatformResources for MoyasarPlatformResourcesService {
    /// Lists registered payout accounts via [`MoyasarClient::payout_accounts`].
    async fn list_payout_accounts(
        &self,
        request: Request<ListPageRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let req = request.into_inner();
        let params = ListPayoutAccountsParams {
            page: req.page,
            per: None,
        };
        let list = self
            .client
            .payout_accounts()
            .list(&params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: format!("count:{}", list.items.len()),
            json: to_json_string(&list.items)?,
        }))
    }

    /// Initiates a bank IBAN payout transfer via [`MoyasarClient::payouts`].
    async fn create_payout(
        &self,
        request: Request<CreatePayoutRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let params = build_payout_params(request.into_inner())?;
        let payout = self
            .client
            .payouts()
            .create(&params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: payout.id.as_str().to_owned(),
            json: to_json_string(&payout)?,
        }))
    }

    /// Fetches current merchant settlement balances via [`MoyasarClient::settlements`].
    async fn fetch_settlement_balance(
        &self,
        _request: Request<ListPageRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let balance = self
            .client
            .settlements()
            .fetch_balance()
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: balance.currency.as_str().to_owned(),
            json: to_json_string(&balance)?,
        }))
    }

    /// Retrieves a single settlement by ID via [`MoyasarClient::settlements`].
    async fn retrieve_settlement(
        &self,
        request: Request<RetrieveIdRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let req = request.into_inner();
        let settlement_id = SettlementId::new(req.id);
        settlement_id
            .validate()
            .map_err(|err| to_tonic_status(&err))?;
        let settlement = self
            .client
            .settlements()
            .retrieve(&settlement_id)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: settlement.id.as_str().to_owned(),
            json: to_json_string(&settlement)?,
        }))
    }

    /// Lists aggregation transfers via [`MoyasarClient::transfers`].
    async fn list_transfers(
        &self,
        request: Request<ListPageRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let req = request.into_inner();
        let params = ListTransfersParams {
            page: req.page,
            ..Default::default()
        };
        let list = self
            .client
            .transfers()
            .list(&params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: format!("count:{}", list.items.len()),
            json: to_json_string(&list.items)?,
        }))
    }

    /// Lists internal ledger transactions via [`MoyasarClient::internal_transactions`].
    async fn list_internal_transactions(
        &self,
        _request: Request<ListPageRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let list = self
            .client
            .internal_transactions()
            .list(&ListInternalTransactionsParams::new())
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: format!("count:{}", list.items.len()),
            json: to_json_string(&list.items)?,
        }))
    }

    /// Looks up card BIN/IIN issuer metadata via [`MoyasarClient::sources`].
    async fn lookup_issuer(
        &self,
        request: Request<BinLookupRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let req = request.into_inner();
        let params = BinLookupParams::new(req.first_digits).map_err(|err| to_tonic_status(&err))?;
        let issuer = self
            .client
            .sources()
            .get_issuer(&params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: issuer.first_digits.clone(),
            json: format!("{:?}", issuer.issuer_card_type),
        }))
    }

    /// Initiates a signed Apple Pay merchant session via [`MoyasarClient::apple_pay`].
    async fn initiate_apple_pay_session(
        &self,
        request: Request<ApplePaySessionRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let req = request.into_inner();
        let params = InitiateApplePayParams::new(
            req.validation_url,
            req.display_name,
            req.domain_name,
            DEFAULT_TONIC_PUBLISHABLE_KEY,
        )
        .map_err(|err| to_tonic_status(&err))?;
        let session = self
            .publishable_client
            .apple_pay()
            .initiate_session(&params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: session.merchant_identifier.clone(),
            json: to_json_string(&session)?,
        }))
    }

    /// Registers a webhook endpoint via [`MoyasarClient::webhooks`], redacting `shared_secret`.
    async fn manage_webhook_endpoint(
        &self,
        request: Request<CreateWebhookEndpointRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let req = request.into_inner();
        let secret = WebhookSecret::new(req.shared_secret);
        let params =
            CreateWebhookParams::new(req.url, secret).map_err(|err| to_tonic_status(&err))?;
        let ep = self
            .client
            .webhooks()
            .create(&params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: ep.id.to_string(),
            json: ep.url,
        }))
    }

    /// Attaches a vaulted token to a customer, charges the customer, and runs subscription cycle 1.
    async fn charge_customer_and_subscription(
        &self,
        request: Request<CustomerChargeRpcRequest>,
    ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
        let req = request.into_inner();
        let token_id = TokenId::new(req.token_id).map_err(|err| to_tonic_status(&err))?;
        let amount = parse_non_negative_amount(req.amount_minor)?;
        let currency = parse_currency(&req.currency)?;
        let given_id = parse_given_id(&req.given_id)?;

        self.client
            .customers(self.vault.as_ref())
            .attach_token(&req.customer_id, token_id.clone())
            .await
            .map_err(|err| to_tonic_status(&err))?;

        let cust_params =
            CustomerChargeParams::new(&req.customer_id, amount, currency).given_id(given_id);
        let cust_payment = self
            .client
            .customers(self.vault.as_ref())
            .charge(&cust_params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        let sub_params =
            SubscriptionCycleChargeParams::new(DEFAULT_SUBSCRIPTION_ID, 1, amount, &token_id)
                .currency(currency);
        let sub_payment = self
            .client
            .subscriptions()
            .charge_cycle(&sub_params)
            .await
            .map_err(|err| to_tonic_status(&err))?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: sub_payment.id,
            json: cust_payment.id,
        }))
    }
}

#[cfg(test)]
mod tests {
    use tonic::Code;

    use super::*;

    #[test]
    fn helpers_validate_amounts_currency_uuid_and_json() {
        assert_eq!(parse_non_negative_amount(0).expect("zero valid"), 0);
        assert_eq!(
            parse_non_negative_amount(-1)
                .expect_err("negative invalid")
                .code(),
            Code::InvalidArgument
        );
        assert_eq!(
            parse_currency("SAR").expect("valid currency"),
            Currency::SAR
        );
        assert_eq!(
            parse_currency("INVALID").expect_err("bad currency").code(),
            Code::InvalidArgument
        );
        assert!(parse_given_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert_eq!(
            parse_given_id("bad-uuid").expect_err("bad uuid").code(),
            Code::InvalidArgument
        );
        assert_eq!(to_json_string(&[1_u32, 2]).expect("json"), "[1,2]");
    }

    #[test]
    fn build_payout_params_validates_iban_mobile_and_amount() {
        let valid = build_payout_params(CreatePayoutRpcRequest {
            source_id: "pa_01J9PAYACC0001".into(),
            amount_minor: 25_000,
            iban: "SA0380000000608010167519".into(),
            name: "Acme Merchant".into(),
            mobile: "+966500000001".into(),
        })
        .expect("valid payout params");
        assert_eq!(valid.amount, 25_000);
        assert_eq!(valid.purpose, PayoutPurpose::PaymentToMerchant);

        let err = build_payout_params(CreatePayoutRpcRequest {
            source_id: "pa_01J9PAYACC0001".into(),
            amount_minor: 0,
            iban: "SA0380000000608010167519".into(),
            name: "Acme Merchant".into(),
            mobile: "+966500000001".into(),
        })
        .expect_err("zero payout amount must fail");
        assert_eq!(err.code(), Code::InvalidArgument);
    }
}
