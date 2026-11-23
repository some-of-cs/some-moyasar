//! gRPC handlers for payouts, settlements, transfers, internal transactions, sources, Apple Pay,
//! webhooks API, customers, and subscriptions.

#![allow(clippy::result_large_err)]
#![allow(dead_code)]

use std::sync::Arc;

use some_moyasar::kernel::{Currency, MoyasarClient, MoyasarError, PublishableKey, SecretKey};
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

/// Maps a [`Result<T, MoyasarError>`] into a gRPC [`Result<T, Status>`].
fn map_sdk<T>(result: Result<T, MoyasarError>) -> Result<T, Status> {
    result.map_err(|err| to_tonic_status(&err))
}

/// Validates that a signed proto minor-unit amount is non-negative and converts it to `u64`.
fn parse_non_negative_amount(amount_minor: i64) -> Result<u64, Status> {
    u64::try_from(amount_minor)
        .map_err(|_| Status::invalid_argument("amount_minor must be non-negative"))
}

/// Serializes a domain model or collection into a JSON string, mapping errors to `INTERNAL`.
fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String, Status> {
    serde_json::to_string(value).map_err(|_| Status::internal("failed to serialize response"))
}

/// Builds a [`Response<JsonPayloadRpcResponse>`] from `id` and a serializable `value`.
fn ok_json_response<T: serde::Serialize>(
    id: String,
    value: &T,
) -> Result<Response<JsonPayloadRpcResponse>, Status> {
    Ok(Response::new(JsonPayloadRpcResponse {
        id,
        json: to_json_string(value)?,
    }))
}

/// Builds a [`Response<JsonPayloadRpcResponse>`] with a `count:<len>` identifier for a slice.
fn ok_count_json_response<T: serde::Serialize>(
    items: &[T],
) -> Result<Response<JsonPayloadRpcResponse>, Status> {
    ok_json_response(format!("count:{}", items.len()), &items)
}

/// Parses a three-letter ISO 4217 [`Currency`] code, mapping errors via [`to_tonic_status`].
fn parse_currency(raw: &str) -> Result<Currency, Status> {
    map_sdk(raw.parse::<Currency>())
}

/// Parses a caller-supplied UUID `given_id`, returning `INVALID_ARGUMENT` if malformed.
fn parse_given_id(raw: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|_| Status::invalid_argument("given_id must be a valid UUID"))
}

/// Validates and builds [`CreatePayoutParams`] for a Saudi bank IBAN transfer in Riyadh.
fn build_payout_params(req: CreatePayoutRpcRequest) -> Result<CreatePayoutParams, Status> {
    let source_id = map_sdk(PayoutAccountId::new(req.source_id))?;
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
    map_sdk(params.validate())?;
    Ok(params)
}

macro_rules! impl_platform_resources {
    ($(
        $(#[$doc:meta])*
        $method:ident($this:ident, $req:ident : $req_ty:ty) => $body:block;
    )+) => {
        #[tonic::async_trait]
        impl MoyasarPlatformResources for MoyasarPlatformResourcesService {
            $(
                $(#[$doc])*
                async fn $method(
                    &self,
                    request: Request<$req_ty>,
                ) -> Result<Response<JsonPayloadRpcResponse>, Status> {
                    let $this = self;
                    let $req = request.into_inner();
                    $body
                }
            )+
        }
    };
}

impl_platform_resources! {
    /// Lists registered payout accounts via [`MoyasarClient::payout_accounts`].
    list_payout_accounts(this, req: ListPageRpcRequest) => {
        let params = ListPayoutAccountsParams {
            page: req.page,
            per: None,
        };
        let list = map_sdk(this.client.payout_accounts().list(&params).await)?;
        ok_count_json_response(&list.items)
    };
    /// Initiates a bank IBAN payout transfer via [`MoyasarClient::payouts`].
    create_payout(this, req: CreatePayoutRpcRequest) => {
        let params = build_payout_params(req)?;
        let payout = map_sdk(this.client.payouts().create(&params).await)?;
        ok_json_response(payout.id.as_str().to_owned(), &payout)
    };
    /// Fetches current merchant settlement balances via [`MoyasarClient::settlements`].
    fetch_settlement_balance(this, _req: ListPageRpcRequest) => {
        let balance = map_sdk(this.client.settlements().fetch_balance().await)?;
        ok_json_response(balance.currency.as_str().to_owned(), &balance)
    };
    /// Retrieves a single settlement by ID via [`MoyasarClient::settlements`].
    retrieve_settlement(this, req: RetrieveIdRpcRequest) => {
        let settlement_id = SettlementId::new(req.id);
        map_sdk(settlement_id.validate())?;
        let settlement = map_sdk(this.client.settlements().retrieve(&settlement_id).await)?;
        ok_json_response(settlement.id.as_str().to_owned(), &settlement)
    };
    /// Lists aggregation transfers via [`MoyasarClient::transfers`].
    list_transfers(this, req: ListPageRpcRequest) => {
        let params = ListTransfersParams {
            page: req.page,
            ..Default::default()
        };
        let list = map_sdk(this.client.transfers().list(&params).await)?;
        ok_count_json_response(&list.items)
    };
    /// Lists internal ledger transactions via [`MoyasarClient::internal_transactions`].
    list_internal_transactions(this, _req: ListPageRpcRequest) => {
        let params = ListInternalTransactionsParams::new();
        let list = map_sdk(this.client.internal_transactions().list(&params).await)?;
        ok_count_json_response(&list.items)
    };
    /// Looks up card BIN/IIN issuer metadata via [`MoyasarClient::sources`].
    lookup_issuer(this, req: BinLookupRpcRequest) => {
        let params = map_sdk(BinLookupParams::new(req.first_digits))?;
        let issuer = map_sdk(this.client.sources().get_issuer(&params).await)?;
        Ok(Response::new(JsonPayloadRpcResponse {
            id: issuer.first_digits,
            json: format!("{:?}", issuer.issuer_card_type),
        }))
    };
    /// Initiates a signed Apple Pay merchant session via [`MoyasarClient::apple_pay`].
    initiate_apple_pay_session(this, req: ApplePaySessionRpcRequest) => {
        let params = map_sdk(InitiateApplePayParams::new(
            req.validation_url,
            req.display_name,
            req.domain_name,
            DEFAULT_TONIC_PUBLISHABLE_KEY,
        ))?;
        let session = map_sdk(
            this.publishable_client
                .apple_pay()
                .initiate_session(&params)
                .await,
        )?;
        ok_json_response(session.merchant_identifier.clone(), &session)
    };
    /// Registers a webhook endpoint via [`MoyasarClient::webhooks`], redacting `shared_secret`.
    manage_webhook_endpoint(this, req: CreateWebhookEndpointRpcRequest) => {
        let secret = WebhookSecret::new(req.shared_secret);
        let params = map_sdk(CreateWebhookParams::new(req.url, secret))?;
        let ep = map_sdk(this.client.webhooks().create(&params).await)?;
        Ok(Response::new(JsonPayloadRpcResponse {
            id: ep.id.to_string(),
            json: ep.url,
        }))
    };
    /// Attaches a vaulted token to a customer, charges the customer, and runs subscription cycle 1.
    charge_customer_and_subscription(this, req: CustomerChargeRpcRequest) => {
        let token_id = map_sdk(TokenId::new(req.token_id))?;
        let amount = parse_non_negative_amount(req.amount_minor)?;
        let currency = parse_currency(&req.currency)?;
        let given_id = parse_given_id(&req.given_id)?;

        map_sdk(
            this.client
                .customers(this.vault.as_ref())
                .attach_token(&req.customer_id, token_id.clone())
                .await,
        )?;

        let cust_params =
            CustomerChargeParams::new(&req.customer_id, amount, currency).given_id(given_id);
        let cust_payment = map_sdk(
            this.client
                .customers(this.vault.as_ref())
                .charge(&cust_params)
                .await,
        )?;

        let sub_params =
            SubscriptionCycleChargeParams::new(DEFAULT_SUBSCRIPTION_ID, 1, amount, &token_id)
                .currency(currency);
        let sub_payment = map_sdk(this.client.subscriptions().charge_cycle(&sub_params).await)?;

        Ok(Response::new(JsonPayloadRpcResponse {
            id: sub_payment.id,
            json: cust_payment.id,
        }))
    };
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
        let count_resp = ok_count_json_response(&[1_u32, 2])
            .expect("count response")
            .into_inner();
        assert_eq!(count_resp.id, "count:2");
        assert_eq!(count_resp.json, "[1,2]");
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
