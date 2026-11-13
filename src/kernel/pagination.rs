//! Cursor pagination types for Moyasar list endpoints.

use serde::Deserialize;

/// Pagination metadata returned by Moyasar list endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Meta {
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

/// Generic paginated response wrapper supporting all Moyasar resource array keys.
#[derive(Debug, Clone, Deserialize)]
pub struct PaginatedList<T> {
    /// Items on the current page.
    #[serde(
        alias = "payments",
        alias = "invoices",
        alias = "payouts",
        alias = "payout_accounts",
        alias = "settlements",
        alias = "transfers",
        alias = "lines",
        alias = "internal_transactions",
        alias = "webhooks",
        alias = "attempts"
    )]
    pub items: Vec<T>,
    /// Pagination cursor metadata.
    pub meta: Meta,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserializes_paginated_payments_payload() {
        let raw = json!({
            "payments": [{"id": "pay_1"}, {"id": "pay_2"}],
            "meta": {
                "current_page": 1,
                "next_page": 2,
                "prev_page": null,
                "total_pages": 3,
                "total_count": 6
            }
        });
        let page: PaginatedList<serde_json::Value> = serde_json::from_value(raw).unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.meta.next_page, Some(2));
        assert_eq!(page.meta.total_pages, 3);
    }
}
