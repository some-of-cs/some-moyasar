//! Zero-`protoc` Prost DTOs and Tonic service/client definitions for all crate resource services.

#![allow(clippy::result_large_err)]
#![allow(dead_code)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// Request to retrieve, cancel, or delete a resource by its Moyasar identifier.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RetrieveIdRpcRequest {
    /// Target resource identifier.
    #[prost(string, tag = "1")]
    pub id: String,
}

/// Response representing a Moyasar payment resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaymentJsonRpcResponse {
    /// Unique Moyasar payment identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current payment status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Payment amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "4")]
    pub currency: String,
}

/// Request to execute a smart refund or void on a payment.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SmartRefundRpcRequest {
    /// Target Moyasar payment identifier.
    #[prost(string, tag = "1")]
    pub payment_id: String,
    /// Optional partial refund amount in minor currency units.
    #[prost(int64, optional, tag = "2")]
    pub amount_minor: Option<i64>,
}

/// Response returned after executing a smart refund or void operation.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SmartRefundRpcResponse {
    /// Target Moyasar payment identifier.
    #[prost(string, tag = "1")]
    pub payment_id: String,
    /// Reversal action performed (`"refunded"` or `"voided"`).
    #[prost(string, tag = "2")]
    pub action: String,
    /// Reversed amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
}

/// Response representing a Moyasar payment intent resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IntentJsonRpcResponse {
    /// Unique Moyasar payment intent identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current payment intent status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Intent amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Whether the payment intent is currently usable for checkout.
    #[prost(bool, tag = "4")]
    pub usable: bool,
}

/// Response representing a Moyasar vaulted card token resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TokenRpcResponse {
    /// Unique Moyasar token identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current token status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Card network brand associated with the token.
    #[prost(string, tag = "3")]
    pub brand: String,
    /// Whether the token has been deleted.
    #[prost(bool, tag = "4")]
    pub deleted: bool,
}

/// Request to create a hosted Moyasar invoice in minor currency units.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateInvoiceRpcRequest {
    /// Invoice amount in minor currency units.
    #[prost(int64, tag = "1")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "2")]
    pub currency: String,
    /// Optional human-readable invoice description.
    #[prost(string, optional, tag = "3")]
    pub description: Option<String>,
}

/// Response representing a Moyasar invoice resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InvoiceRpcResponse {
    /// Unique Moyasar invoice identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current invoice status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Invoice amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "4")]
    pub currency: String,
}

/// Response representing a 3-D Secure card authentication challenge resource.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CardAuthRpcResponse {
    /// Unique 3DS card authentication identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Current 3DS authentication status.
    #[prost(string, tag = "2")]
    pub status: String,
    /// Optional ACS challenge redirect URL.
    #[prost(string, optional, tag = "3")]
    pub transaction_url: Option<String>,
}

/// Request to list paginated resources or fetch a ledger summary.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListPageRpcRequest {
    /// Optional 1-based page number.
    #[prost(uint32, optional, tag = "1")]
    pub page: Option<u32>,
}

/// Response carrying a resource identifier and its serialized JSON payload.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct JsonPayloadRpcResponse {
    /// Primary resource or summary identifier.
    #[prost(string, tag = "1")]
    pub id: String,
    /// Serialized JSON representation of the resource payload.
    #[prost(string, tag = "2")]
    pub json: String,
}

/// Request to initiate a bank IBAN payout transfer in minor currency units.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreatePayoutRpcRequest {
    /// Source payout account identifier.
    #[prost(string, tag = "1")]
    pub source_id: String,
    /// Payout amount in minor currency units.
    #[prost(int64, tag = "2")]
    pub amount_minor: i64,
    /// Beneficiary IBAN destination.
    #[prost(string, tag = "3")]
    pub iban: String,
    /// Beneficiary full legal name.
    #[prost(string, tag = "4")]
    pub name: String,
    /// Beneficiary mobile phone number.
    #[prost(string, tag = "5")]
    pub mobile: String,
}

/// Request to look up card issuer metadata by BIN prefix.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BinLookupRpcRequest {
    /// First 6 to 8 digits of the payment card number.
    #[prost(string, tag = "1")]
    pub first_digits: String,
}

/// Request to initiate an Apple Pay merchant validation session.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ApplePaySessionRpcRequest {
    /// Apple Pay merchant validation URL.
    #[prost(string, tag = "1")]
    pub validation_url: String,
    /// Merchant display name shown on the Apple Pay sheet.
    #[prost(string, tag = "2")]
    pub display_name: String,
    /// Verified merchant web domain name.
    #[prost(string, tag = "3")]
    pub domain_name: String,
}

/// Request to register a webhook endpoint and verify delivery.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateWebhookEndpointRpcRequest {
    /// HTTPS destination URL for webhook deliveries.
    #[prost(string, tag = "1")]
    pub url: String,
    /// Shared secret used to sign webhook payloads.
    #[prost(string, tag = "2")]
    pub shared_secret: String,
}

/// Request to upsert a customer profile, charge a vaulted token, and run a subscription cycle.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CustomerChargeRpcRequest {
    /// Logical customer identifier.
    #[prost(string, tag = "1")]
    pub customer_id: String,
    /// Vaulted card token identifier (`token_...`).
    #[prost(string, tag = "2")]
    pub token_id: String,
    /// Charge amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Three-letter ISO 4217 currency code.
    #[prost(string, tag = "4")]
    pub currency: String,
    /// Caller-supplied idempotency key (`given_id`).
    #[prost(string, tag = "5")]
    pub given_id: String,
}

/// Request to execute a recurring subscription billing cycle.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscriptionCycleRpcRequest {
    /// Logical subscription identifier.
    #[prost(string, tag = "1")]
    pub subscription_id: String,
    /// Zero-based or one-based billing cycle index.
    #[prost(uint64, tag = "2")]
    pub cycle_index: u64,
    /// Billing cycle amount in minor currency units.
    #[prost(int64, tag = "3")]
    pub amount_minor: i64,
    /// Vaulted card token identifier (`token_...`).
    #[prost(string, tag = "4")]
    pub token_id: String,
}

/// gRPC service trait for `moyasar.v1.MoyasarCoreResources`.
#[tonic::async_trait]
pub trait MoyasarCoreResources: Send + Sync + 'static {
    /// Retrieves a payment by ID.
    async fn retrieve_payment(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<PaymentJsonRpcResponse>, tonic::Status>;

    /// Refunds a captured payment or voids an authorized payment.
    async fn smart_refund(
        &self,
        request: tonic::Request<SmartRefundRpcRequest>,
    ) -> Result<tonic::Response<SmartRefundRpcResponse>, tonic::Status>;

    /// Retrieves a payment intent by ID.
    async fn retrieve_intent(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<IntentJsonRpcResponse>, tonic::Status>;

    /// Cancels an active payment intent by ID.
    async fn cancel_intent(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<IntentJsonRpcResponse>, tonic::Status>;

    /// Retrieves a vaulted card token by ID.
    async fn retrieve_token(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<TokenRpcResponse>, tonic::Status>;

    /// Deletes a vaulted card token by ID.
    async fn delete_token(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<TokenRpcResponse>, tonic::Status>;

    /// Creates a hosted invoice in minor currency units.
    async fn create_invoice(
        &self,
        request: tonic::Request<CreateInvoiceRpcRequest>,
    ) -> Result<tonic::Response<InvoiceRpcResponse>, tonic::Status>;

    /// Cancels an active invoice by ID.
    async fn cancel_invoice(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<InvoiceRpcResponse>, tonic::Status>;

    /// Retrieves a 3DS card authentication session by ID.
    async fn retrieve_card_auth(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<CardAuthRpcResponse>, tonic::Status>;
}

/// gRPC service trait for `moyasar.v1.MoyasarPlatformResources`.
#[tonic::async_trait]
pub trait MoyasarPlatformResources: Send + Sync + 'static {
    /// Lists registered payout accounts.
    async fn list_payout_accounts(
        &self,
        request: tonic::Request<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Creates a bank IBAN payout transfer.
    async fn create_payout(
        &self,
        request: tonic::Request<CreatePayoutRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Fetches aggregated settlement balances.
    async fn fetch_settlement_balance(
        &self,
        request: tonic::Request<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Retrieves a settlement and its lines by ID.
    async fn retrieve_settlement(
        &self,
        request: tonic::Request<RetrieveIdRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Lists platform transfers.
    async fn list_transfers(
        &self,
        request: tonic::Request<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Lists internal ledger transactions.
    async fn list_internal_transactions(
        &self,
        request: tonic::Request<ListPageRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Looks up card issuer metadata by BIN prefix.
    async fn lookup_issuer(
        &self,
        request: tonic::Request<BinLookupRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Initiates an Apple Pay merchant validation session.
    async fn initiate_apple_pay_session(
        &self,
        request: tonic::Request<ApplePaySessionRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Creates and manages a webhook endpoint subscription.
    async fn manage_webhook_endpoint(
        &self,
        request: tonic::Request<CreateWebhookEndpointRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;

    /// Charges a customer profile and runs a recurring subscription cycle.
    async fn charge_customer_and_subscription(
        &self,
        request: tonic::Request<CustomerChargeRpcRequest>,
    ) -> Result<tonic::Response<JsonPayloadRpcResponse>, tonic::Status>;
}

type BoxFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'static>>;
type HttpBody = tonic::body::BoxBody;
type HttpRequest = tonic::codegen::http::Request<HttpBody>;
type HttpResponse = tonic::codegen::http::Response<HttpBody>;

fn dispatch_unary<S, Req, Res>(
    svc: S,
    req: HttpRequest,
) -> BoxFuture<HttpResponse, std::convert::Infallible>
where
    S: tonic::server::UnaryService<Req, Response = Res> + Send + 'static,
    S::Future: Send + 'static,
    Req: prost::Message + Default + Send + 'static,
    Res: prost::Message + Send + 'static,
{
    Box::pin(async move {
        let mut grpc = tonic::server::Grpc::new(tonic::codec::ProstCodec::default());
        Ok(grpc.unary(svc, req).await)
    })
}

fn unimplemented_response() -> HttpResponse {
    let mut res = tonic::codegen::http::Response::new(tonic::body::empty_body());
    let headers = res.headers_mut();
    headers.insert(
        tonic::codegen::http::header::CONTENT_TYPE,
        tonic::codegen::http::HeaderValue::from_static("application/grpc"),
    );
    headers.insert(
        "grpc-status",
        tonic::codegen::http::HeaderValue::from_static("12"),
    );
    res
}

macro_rules! define_unary_svc {
    ($trait_bound:ident { $($svc:ident : $req:ty => $res:ty => $method:ident;)+ }) => {
        $(
            struct $svc<T>(Arc<T>);

            impl<T: $trait_bound> tonic::server::UnaryService<$req> for $svc<T> {
                type Response = $res;
                type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

                fn call(&mut self, request: tonic::Request<$req>) -> Self::Future {
                    let inner = Arc::clone(&self.0);
                    Box::pin(async move { inner.$method(request).await })
                }
            }
        )+
    };
}

macro_rules! define_grpc_server {
    (
        $(#[$doc:meta])*
        $server:ident : $trait_bound:ident => $service_name:expr;
        $dispatch_primary:ident { $($path_a:pat => $svc_a:ident;)+ }
        $dispatch_secondary:ident { $($path_b:pat => $svc_b:ident;)+ }
    ) => {
        $(#[$doc])*
        #[derive(Debug)]
        pub struct $server<T> {
            inner: Arc<T>,
        }

        impl<T> $server<T> {
            /// Creates a new gRPC server wrapper around `inner`.
            pub fn new(inner: T) -> Self {
                Self {
                    inner: Arc::new(inner),
                }
            }
        }

        impl<T> Clone for $server<T> {
            fn clone(&self) -> Self {
                Self {
                    inner: Arc::clone(&self.inner),
                }
            }
        }

        impl<T: $trait_bound> tonic::server::NamedService for $server<T> {
            const NAME: &'static str = $service_name;
        }

        fn $dispatch_primary<T: $trait_bound>(
            inner: &Arc<T>,
            req: HttpRequest,
        ) -> BoxFuture<HttpResponse, std::convert::Infallible> {
            match req.uri().path() {
                $(
                    $path_a => dispatch_unary($svc_a(Arc::clone(inner)), req),
                )+
                _ => $dispatch_secondary(inner, req),
            }
        }

        fn $dispatch_secondary<T: $trait_bound>(
            inner: &Arc<T>,
            req: HttpRequest,
        ) -> BoxFuture<HttpResponse, std::convert::Infallible> {
            match req.uri().path() {
                $(
                    $path_b => dispatch_unary($svc_b(Arc::clone(inner)), req),
                )+
                _ => Box::pin(async move { Ok(unimplemented_response()) }),
            }
        }

        impl<T> tonic::codegen::Service<HttpRequest> for $server<T>
        where
            T: $trait_bound,
        {
            type Response = HttpResponse;
            type Error = std::convert::Infallible;
            type Future = BoxFuture<Self::Response, Self::Error>;

            fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: HttpRequest) -> Self::Future {
                $dispatch_primary(&self.inner, req)
            }
        }
    };
}

macro_rules! define_grpc_client {
    (
        $(#[$doc:meta])*
        $client:ident {
            $($(#[$m_doc:meta])* $method:ident : $req:ty => $res:ty => $path:expr;)+
        }
    ) => {
        $(#[$doc])*
        #[derive(Debug, Clone)]
        pub struct $client<T> {
            inner: tonic::client::Grpc<T>,
        }

        impl $client<tonic::transport::Channel> {
            /// Creates a new gRPC client from a Tonic transport channel.
            pub fn new(channel: tonic::transport::Channel) -> Self {
                Self {
                    inner: tonic::client::Grpc::new(channel),
                }
            }
        }

        impl<T> $client<T>
        where
            T: tonic::client::GrpcService<tonic::body::BoxBody>,
            T::Error: Into<tonic::codegen::StdError>,
            T::ResponseBody: tonic::codegen::Body<Data = tonic::codegen::Bytes> + Send + 'static,
            <T::ResponseBody as tonic::codegen::Body>::Error:
                Into<tonic::codegen::StdError> + Send,
        {
            async fn unary_call<Req, Res>(
                &mut self,
                request: impl tonic::IntoRequest<Req>,
                path: &'static str,
            ) -> Result<tonic::Response<Res>, tonic::Status>
            where
                Req: prost::Message + 'static,
                Res: prost::Message + Default + 'static,
            {
                self.inner.ready().await.map_err(|err| {
                    tonic::Status::unknown(format!("Service was not ready: {}", err.into()))
                })?;
                let codec = tonic::codec::ProstCodec::default();
                let path = tonic::codegen::http::uri::PathAndQuery::from_static(path);
                self.inner.unary(request.into_request(), path, codec).await
            }

            $(
                $(#[$m_doc])*
                pub async fn $method(
                    &mut self,
                    request: impl tonic::IntoRequest<$req>,
                ) -> Result<tonic::Response<$res>, tonic::Status> {
                    self.unary_call(request, $path).await
                }
            )+
        }
    };
}

define_unary_svc! {
    MoyasarCoreResources {
        RetrievePaymentSvc: RetrieveIdRpcRequest => PaymentJsonRpcResponse => retrieve_payment;
        SmartRefundSvc: SmartRefundRpcRequest => SmartRefundRpcResponse => smart_refund;
        RetrieveIntentSvc: RetrieveIdRpcRequest => IntentJsonRpcResponse => retrieve_intent;
        CancelIntentSvc: RetrieveIdRpcRequest => IntentJsonRpcResponse => cancel_intent;
        RetrieveTokenSvc: RetrieveIdRpcRequest => TokenRpcResponse => retrieve_token;
        DeleteTokenSvc: RetrieveIdRpcRequest => TokenRpcResponse => delete_token;
        CreateInvoiceSvc: CreateInvoiceRpcRequest => InvoiceRpcResponse => create_invoice;
        CancelInvoiceSvc: RetrieveIdRpcRequest => InvoiceRpcResponse => cancel_invoice;
        RetrieveCardAuthSvc: RetrieveIdRpcRequest => CardAuthRpcResponse => retrieve_card_auth;
    }
}

define_grpc_server! {
    /// Tonic gRPC server wrapper for [`MoyasarCoreResources`].
    MoyasarCoreResourcesServer: MoyasarCoreResources => "moyasar.v1.MoyasarCoreResources";
    dispatch_payment_intent_rpc {
        "/moyasar.v1.MoyasarCoreResources/RetrievePayment" => RetrievePaymentSvc;
        "/moyasar.v1.MoyasarCoreResources/SmartRefund" => SmartRefundSvc;
        "/moyasar.v1.MoyasarCoreResources/RetrieveIntent" => RetrieveIntentSvc;
        "/moyasar.v1.MoyasarCoreResources/CancelIntent" => CancelIntentSvc;
    }
    dispatch_token_invoice_3ds_rpc {
        "/moyasar.v1.MoyasarCoreResources/RetrieveToken" => RetrieveTokenSvc;
        "/moyasar.v1.MoyasarCoreResources/DeleteToken" => DeleteTokenSvc;
        "/moyasar.v1.MoyasarCoreResources/CreateInvoice" => CreateInvoiceSvc;
        "/moyasar.v1.MoyasarCoreResources/CancelInvoice" => CancelInvoiceSvc;
        "/moyasar.v1.MoyasarCoreResources/RetrieveCardAuth" => RetrieveCardAuthSvc;
    }
}

define_grpc_client! {
    /// Tonic gRPC client for `moyasar.v1.MoyasarCoreResources`.
    MoyasarCoreResourcesClient {
        /// Calls `RetrievePayment` on the remote core resources service.
        retrieve_payment: RetrieveIdRpcRequest => PaymentJsonRpcResponse =>
            "/moyasar.v1.MoyasarCoreResources/RetrievePayment";
        /// Calls `SmartRefund` on the remote core resources service.
        smart_refund: SmartRefundRpcRequest => SmartRefundRpcResponse =>
            "/moyasar.v1.MoyasarCoreResources/SmartRefund";
        /// Calls `RetrieveIntent` on the remote core resources service.
        retrieve_intent: RetrieveIdRpcRequest => IntentJsonRpcResponse =>
            "/moyasar.v1.MoyasarCoreResources/RetrieveIntent";
        /// Calls `CancelIntent` on the remote core resources service.
        cancel_intent: RetrieveIdRpcRequest => IntentJsonRpcResponse =>
            "/moyasar.v1.MoyasarCoreResources/CancelIntent";
        /// Calls `RetrieveToken` on the remote core resources service.
        retrieve_token: RetrieveIdRpcRequest => TokenRpcResponse =>
            "/moyasar.v1.MoyasarCoreResources/RetrieveToken";
        /// Calls `DeleteToken` on the remote core resources service.
        delete_token: RetrieveIdRpcRequest => TokenRpcResponse =>
            "/moyasar.v1.MoyasarCoreResources/DeleteToken";
        /// Calls `CreateInvoice` on the remote core resources service.
        create_invoice: CreateInvoiceRpcRequest => InvoiceRpcResponse =>
            "/moyasar.v1.MoyasarCoreResources/CreateInvoice";
        /// Calls `CancelInvoice` on the remote core resources service.
        cancel_invoice: RetrieveIdRpcRequest => InvoiceRpcResponse =>
            "/moyasar.v1.MoyasarCoreResources/CancelInvoice";
        /// Calls `RetrieveCardAuth` on the remote core resources service.
        retrieve_card_auth: RetrieveIdRpcRequest => CardAuthRpcResponse =>
            "/moyasar.v1.MoyasarCoreResources/RetrieveCardAuth";
    }
}

define_unary_svc! {
    MoyasarPlatformResources {
        ListPayoutAccountsSvc: ListPageRpcRequest => JsonPayloadRpcResponse => list_payout_accounts;
        CreatePayoutSvc: CreatePayoutRpcRequest => JsonPayloadRpcResponse => create_payout;
        FetchSettlementBalanceSvc: ListPageRpcRequest => JsonPayloadRpcResponse => fetch_settlement_balance;
        RetrieveSettlementSvc: RetrieveIdRpcRequest => JsonPayloadRpcResponse => retrieve_settlement;
        ListTransfersSvc: ListPageRpcRequest => JsonPayloadRpcResponse => list_transfers;
        ListInternalTransactionsSvc: ListPageRpcRequest => JsonPayloadRpcResponse => list_internal_transactions;
        LookupIssuerSvc: BinLookupRpcRequest => JsonPayloadRpcResponse => lookup_issuer;
        InitiateApplePaySessionSvc: ApplePaySessionRpcRequest => JsonPayloadRpcResponse => initiate_apple_pay_session;
        ManageWebhookEndpointSvc: CreateWebhookEndpointRpcRequest => JsonPayloadRpcResponse => manage_webhook_endpoint;
        ChargeCustomerAndSubscriptionSvc: CustomerChargeRpcRequest => JsonPayloadRpcResponse => charge_customer_and_subscription;
    }
}

define_grpc_server! {
    /// Tonic gRPC server wrapper for [`MoyasarPlatformResources`].
    MoyasarPlatformResourcesServer: MoyasarPlatformResources => "moyasar.v1.MoyasarPlatformResources";
    dispatch_ledger_payout_rpc {
        "/moyasar.v1.MoyasarPlatformResources/ListPayoutAccounts" => ListPayoutAccountsSvc;
        "/moyasar.v1.MoyasarPlatformResources/CreatePayout" => CreatePayoutSvc;
        "/moyasar.v1.MoyasarPlatformResources/FetchSettlementBalance" => FetchSettlementBalanceSvc;
        "/moyasar.v1.MoyasarPlatformResources/RetrieveSettlement" => RetrieveSettlementSvc;
        "/moyasar.v1.MoyasarPlatformResources/ListTransfers" => ListTransfersSvc;
    }
    dispatch_sources_stripe_rpc {
        "/moyasar.v1.MoyasarPlatformResources/ListInternalTransactions" => ListInternalTransactionsSvc;
        "/moyasar.v1.MoyasarPlatformResources/LookupIssuer" => LookupIssuerSvc;
        "/moyasar.v1.MoyasarPlatformResources/InitiateApplePaySession" => InitiateApplePaySessionSvc;
        "/moyasar.v1.MoyasarPlatformResources/ManageWebhookEndpoint" => ManageWebhookEndpointSvc;
        "/moyasar.v1.MoyasarPlatformResources/ChargeCustomerAndSubscription" => ChargeCustomerAndSubscriptionSvc;
    }
}

define_grpc_client! {
    /// Tonic gRPC client for `moyasar.v1.MoyasarPlatformResources`.
    MoyasarPlatformResourcesClient {
        /// Calls `ListPayoutAccounts` on the remote platform resources service.
        list_payout_accounts: ListPageRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/ListPayoutAccounts";
        /// Calls `CreatePayout` on the remote platform resources service.
        create_payout: CreatePayoutRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/CreatePayout";
        /// Calls `FetchSettlementBalance` on the remote platform resources service.
        fetch_settlement_balance: ListPageRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/FetchSettlementBalance";
        /// Calls `RetrieveSettlement` on the remote platform resources service.
        retrieve_settlement: RetrieveIdRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/RetrieveSettlement";
        /// Calls `ListTransfers` on the remote platform resources service.
        list_transfers: ListPageRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/ListTransfers";
        /// Calls `ListInternalTransactions` on the remote platform resources service.
        list_internal_transactions: ListPageRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/ListInternalTransactions";
        /// Calls `LookupIssuer` on the remote platform resources service.
        lookup_issuer: BinLookupRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/LookupIssuer";
        /// Calls `InitiateApplePaySession` on the remote platform resources service.
        initiate_apple_pay_session: ApplePaySessionRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/InitiateApplePaySession";
        /// Calls `ManageWebhookEndpoint` on the remote platform resources service.
        manage_webhook_endpoint: CreateWebhookEndpointRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/ManageWebhookEndpoint";
        /// Calls `ChargeCustomerAndSubscription` on the remote platform resources service.
        charge_customer_and_subscription: CustomerChargeRpcRequest => JsonPayloadRpcResponse =>
            "/moyasar.v1.MoyasarPlatformResources/ChargeCustomerAndSubscription";
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use tonic::server::NamedService;

    use super::*;

    #[test]
    fn core_and_platform_dtos_roundtrip_minor_units() {
        let invoice_req = CreateInvoiceRpcRequest {
            amount_minor: 25_000,
            currency: "SAR".into(),
            description: Some("Subscription Invoice".into()),
        };
        let bytes = invoice_req.encode_to_vec();
        let decoded =
            CreateInvoiceRpcRequest::decode(bytes.as_slice()).expect("valid invoice prost");
        assert_eq!(decoded, invoice_req);

        let sub_req = SubscriptionCycleRpcRequest {
            subscription_id: "sub_01".into(),
            cycle_index: 1,
            amount_minor: 9_900,
            token_id: "token_abc".into(),
        };
        let sub_bytes = sub_req.encode_to_vec();
        let sub_decoded =
            SubscriptionCycleRpcRequest::decode(sub_bytes.as_slice()).expect("valid sub prost");
        assert_eq!(sub_decoded, sub_req);
    }

    struct DummyResources;

    macro_rules! impl_dummy_trait {
        ($trait_name:ident { $($method:ident : $req:ty => $res:ty;)+ }) => {
            #[tonic::async_trait]
            impl $trait_name for DummyResources {
                $(
                    async fn $method(
                        &self,
                        _request: tonic::Request<$req>,
                    ) -> Result<tonic::Response<$res>, tonic::Status> {
                        Err(tonic::Status::unimplemented("stub"))
                    }
                )+
            }
        };
    }

    impl_dummy_trait! {
        MoyasarCoreResources {
            retrieve_payment: RetrieveIdRpcRequest => PaymentJsonRpcResponse;
            smart_refund: SmartRefundRpcRequest => SmartRefundRpcResponse;
            retrieve_intent: RetrieveIdRpcRequest => IntentJsonRpcResponse;
            cancel_intent: RetrieveIdRpcRequest => IntentJsonRpcResponse;
            retrieve_token: RetrieveIdRpcRequest => TokenRpcResponse;
            delete_token: RetrieveIdRpcRequest => TokenRpcResponse;
            create_invoice: CreateInvoiceRpcRequest => InvoiceRpcResponse;
            cancel_invoice: RetrieveIdRpcRequest => InvoiceRpcResponse;
            retrieve_card_auth: RetrieveIdRpcRequest => CardAuthRpcResponse;
        }
    }

    impl_dummy_trait! {
        MoyasarPlatformResources {
            list_payout_accounts: ListPageRpcRequest => JsonPayloadRpcResponse;
            create_payout: CreatePayoutRpcRequest => JsonPayloadRpcResponse;
            fetch_settlement_balance: ListPageRpcRequest => JsonPayloadRpcResponse;
            retrieve_settlement: RetrieveIdRpcRequest => JsonPayloadRpcResponse;
            list_transfers: ListPageRpcRequest => JsonPayloadRpcResponse;
            list_internal_transactions: ListPageRpcRequest => JsonPayloadRpcResponse;
            lookup_issuer: BinLookupRpcRequest => JsonPayloadRpcResponse;
            initiate_apple_pay_session: ApplePaySessionRpcRequest => JsonPayloadRpcResponse;
            manage_webhook_endpoint: CreateWebhookEndpointRpcRequest => JsonPayloadRpcResponse;
            charge_customer_and_subscription: CustomerChargeRpcRequest => JsonPayloadRpcResponse;
        }
    }

    #[test]
    fn server_named_services_match_proto_packages() {
        assert_eq!(
            MoyasarCoreResourcesServer::<DummyResources>::NAME,
            "moyasar.v1.MoyasarCoreResources"
        );
        assert_eq!(
            MoyasarPlatformResourcesServer::<DummyResources>::NAME,
            "moyasar.v1.MoyasarPlatformResources"
        );
    }
}
