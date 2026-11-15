//! Sources (BIN lookup), Apple Pay session initiation, and Webhooks CRUD API route handlers.
#![forbid(unsafe_code)]

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use some_moyasar::prelude::{
    ApplePayMerchantSession, BinLookupParams, CreateWebhookParams, InitiateApplePayParams,
    IssuerMetadata, MoyasarError, PaginatedList, SourceIssuerCardType, WebhookEndpoint,
    WebhookEventType, WebhookSecret,
};

use super::error::AppError;
use super::state::AppState;

/// Request payload for `POST /sources/issuer`.
#[derive(Debug, Deserialize)]
pub struct LookupIssuerRequest {
    /// Leading 6- to 8-digit card BIN/IIN prefix.
    pub first_digits: String,
}

/// Serializable JSON response DTO for [`IssuerMetadata`].
#[derive(Debug, Serialize)]
pub struct IssuerMetadataResponse {
    /// Issuing bank or financial institution name, if available.
    pub issuer_name: Option<String>,
    /// Two-letter ISO 3166-1 alpha-2 country code of the issuer (e.g., `"SA"`).
    pub issuer_country: Option<String>,
    /// Funding classification of the card (`debit`, `credit`, `charge_card`, etc.).
    pub issuer_card_type: SourceIssuerCardType,
    /// Card program or tier category (e.g., `"classic"`, `"platinum"`), if available.
    pub issuer_card_category: Option<String>,
    /// Leading BIN/IIN digits echoed by the API.
    pub first_digits: String,
    /// Trailing card digits, if returned by the API.
    pub last_digits: Option<String>,
}

impl From<IssuerMetadata> for IssuerMetadataResponse {
    fn from(meta: IssuerMetadata) -> Self {
        Self {
            issuer_name: meta.issuer_name,
            issuer_country: meta.issuer_country,
            issuer_card_type: meta.issuer_card_type,
            issuer_card_category: meta.issuer_card_category,
            first_digits: meta.first_digits,
            last_digits: meta.last_digits,
        }
    }
}

/// Request payload for `POST /apple_pay/session`.
#[derive(Debug, Deserialize)]
pub struct InitiateApplePayRequest {
    /// Apple Pay validation URL provided by `PKPaymentAuthorizationController` (must be `https://`).
    pub validation_url: String,
    /// Merchant display name shown on the Apple Pay payment sheet.
    pub display_name: String,
    /// Verified merchant domain name initiating the session.
    pub domain_name: String,
    /// Optional publishable Moyasar API key (`pk_live_...` or `pk_test_...`).
    pub publishable_api_key: Option<String>,
}

/// Request payload for `POST /webhooks_api`.
#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    /// HTTPS destination URL that receives webhook events.
    pub url: String,
    /// Caller-supplied HMAC shared secret used to sign webhook payloads.
    pub shared_secret: WebhookSecret,
    /// Optional HTTP delivery method override (defaults to `"post"`).
    pub http_method: Option<String>,
    /// Optional list of subscribed event types; `None` subscribes to all events.
    pub events: Option<Vec<WebhookEventType>>,
}

/// Optional query parameters for `GET /webhooks_api`.
#[derive(Debug, Default, Deserialize)]
pub struct ListWebhooksQuery {
    /// Optional 1-based page number.
    pub page: Option<u32>,
}

/// Redacted JSON response DTO for [`WebhookEndpoint`].
#[derive(Debug, Serialize)]
pub struct WebhookEndpointResponse {
    /// Unique identifier of the webhook endpoint.
    pub id: Uuid,
    /// HTTP method used for webhook delivery (e.g., `"post"`).
    pub http_method: String,
    /// Destination URL receiving webhook event payloads.
    pub url: String,
    /// Optional list of subscribed webhook event types.
    pub events: Option<Vec<WebhookEventType>>,
    /// RFC 3339 timestamp when the webhook endpoint was created.
    pub created_at: String,
}

impl From<WebhookEndpoint> for WebhookEndpointResponse {
    fn from(endpoint: WebhookEndpoint) -> Self {
        Self {
            id: endpoint.id,
            http_method: endpoint.http_method,
            url: endpoint.url,
            events: endpoint.events,
            created_at: endpoint.created_at.to_rfc3339(),
        }
    }
}

/// Pagination metadata DTO for [`WebhookListResponse`].
#[derive(Debug, Serialize)]
pub struct WebhookListMetaResponse {
    /// Current 1-based page number.
    pub current_page: u32,
    /// Next page number, if more results exist.
    pub next_page: Option<u32>,
    /// Previous page number, if any.
    pub prev_page: Option<u32>,
    /// Total number of pages available.
    pub total_pages: u32,
    /// Total number of matching records.
    pub total_count: u64,
}

/// Paginated JSON response DTO for `GET /webhooks_api`.
#[derive(Debug, Serialize)]
pub struct WebhookListResponse {
    /// Webhook endpoints on the current page.
    pub webhooks: Vec<WebhookEndpointResponse>,
    /// Pagination cursor metadata.
    pub meta: WebhookListMetaResponse,
}

impl From<PaginatedList<WebhookEndpoint>> for WebhookListResponse {
    fn from(list: PaginatedList<WebhookEndpoint>) -> Self {
        Self {
            webhooks: list
                .items
                .into_iter()
                .map(WebhookEndpointResponse::from)
                .collect(),
            meta: WebhookListMetaResponse {
                current_page: list.meta.current_page,
                next_page: list.meta.next_page,
                prev_page: list.meta.prev_page,
                total_pages: list.meta.total_pages,
                total_count: list.meta.total_count,
            },
        }
    }
}

/// Builds the router for sources, Apple Pay, and webhooks management API.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sources/issuer", post(get_issuer))
        .route("/apple_pay/session", post(initiate_apple_pay_session))
        .route("/webhooks_api", post(create_webhook).get(list_webhooks))
        .route("/webhooks_api/events", get(list_available_events))
        .route("/webhooks_api/{id}", delete(delete_webhook))
}

/// Handles `POST /sources/issuer` by looking up card BIN/IIN issuer metadata.
async fn get_issuer(
    State(state): State<AppState>,
    Json(payload): Json<LookupIssuerRequest>,
) -> Result<Json<IssuerMetadataResponse>, AppError> {
    let params = BinLookupParams::new(payload.first_digits)?;
    let issuer = state.client.sources().get_issuer(&params).await?;
    Ok(Json(IssuerMetadataResponse::from(issuer)))
}

/// Handles `POST /apple_pay/session` using the publishable-key client.
async fn initiate_apple_pay_session(
    State(state): State<AppState>,
    Json(payload): Json<InitiateApplePayRequest>,
) -> Result<Json<ApplePayMerchantSession>, AppError> {
    let pk = payload
        .publishable_api_key
        .unwrap_or_else(|| "pk_test_axum_key".to_owned());
    let params = InitiateApplePayParams::new(
        payload.validation_url,
        payload.display_name,
        payload.domain_name,
        pk,
    )?;
    let session = state
        .publishable_client
        .apple_pay()
        .initiate_session(&params)
        .await?;
    Ok(Json(session))
}

/// Constructs validated [`CreateWebhookParams`] from the incoming request DTO.
fn build_create_webhook_params(
    payload: CreateWebhookRequest,
) -> Result<CreateWebhookParams, MoyasarError> {
    let mut params = CreateWebhookParams::new(payload.url, payload.shared_secret)?;
    if let Some(http_method) = payload.http_method {
        params = params.http_method(http_method);
    }
    if let Some(events) = payload.events {
        params = params.events(events);
    }
    Ok(params)
}

/// Handles `POST /webhooks_api` by registering a new webhook endpoint.
async fn create_webhook(
    State(state): State<AppState>,
    Json(payload): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<WebhookEndpointResponse>), AppError> {
    let params = build_create_webhook_params(payload)?;
    let endpoint = state.client.webhooks().create(&params).await?;
    Ok((
        StatusCode::CREATED,
        Json(WebhookEndpointResponse::from(endpoint)),
    ))
}

/// Handles `GET /webhooks_api` by listing registered webhook endpoints.
async fn list_webhooks(
    State(state): State<AppState>,
    Query(query): Query<ListWebhooksQuery>,
) -> Result<Json<WebhookListResponse>, AppError> {
    let page = state.client.webhooks().list(query.page).await?;
    Ok(Json(WebhookListResponse::from(page)))
}

/// Handles `GET /webhooks_api/events` by returning supported webhook event types.
async fn list_available_events(
    State(state): State<AppState>,
) -> Result<Json<Vec<WebhookEventType>>, AppError> {
    let events = state.client.webhooks().available_events().await?;
    Ok(Json(events))
}

/// Handles `DELETE /webhooks_api/{id}` by deleting a webhook endpoint by UUID.
async fn delete_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state.client.webhooks().delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
