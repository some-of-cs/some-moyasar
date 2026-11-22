//! Raw card PAN default-deny guard and HTTP 201 status / `NextAction` mapper (US-9.1.0 Unit 2).
//!
//! Enforces two `some-payment` orchestrator alignment invariants locally:
//!
//! 1. **Default-deny raw PAN submission (`TM-2`, `AC-2`)**: Rejects raw `creditcard` /
//!    `credit_card` sources locally with zero HTTP calls and zero PAN data in error messages
//!    unless the caller explicitly opts in via [`RawCardPolicy::opt_in_raw_cards`].
//! 2. **Strict HTTP 201 status & `NextAction` inspection (`TM-4`, `AC-3`)**: Never assumes an
//!    HTTP 201 response means [`MappedPaymentStatus::Paid`]; inspects `payment.status` and maps
//!    `initiated` flows into explicit [`MappedNextAction`] variants (`OtpChallenge` for STC Pay,
//!    `OfflineBill` for SADAD, `Redirect` for 3DS/card flows).

use crate::kernel::MoyasarError;

/// Policy controlling whether raw card PAN (`creditcard` / `credit_card`) payment sources are
/// permitted in pre-flight checks.
///
/// The [`Default`] implementation is fail-closed (`allow_raw_cards: false`), matching
/// [`RawCardPolicy::default_deny`].
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::pan_and_status::RawCardPolicy;
///
/// let policy = RawCardPolicy::default();
/// assert!(!policy.allow_raw_cards);
/// assert_eq!(policy, RawCardPolicy::default_deny());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawCardPolicy {
    /// When `false` (default), raw `creditcard` / `credit_card` sources are rejected locally.
    pub allow_raw_cards: bool,
}

impl RawCardPolicy {
    /// Returns the default-deny policy that blocks raw card PAN submission
    /// (`allow_raw_cards = false`).
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::alignment::pan_and_status::RawCardPolicy;
    ///
    /// let policy = RawCardPolicy::default_deny();
    /// assert!(!policy.allow_raw_cards);
    /// ```
    #[must_use]
    pub const fn default_deny() -> Self {
        Self {
            allow_raw_cards: false,
        }
    }

    /// Returns an explicit opt-in policy permitting raw card PAN submission
    /// (`allow_raw_cards = true`).
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::alignment::pan_and_status::RawCardPolicy;
    ///
    /// let policy = RawCardPolicy::opt_in_raw_cards();
    /// assert!(policy.allow_raw_cards);
    /// ```
    #[must_use]
    pub const fn opt_in_raw_cards() -> Self {
        Self {
            allow_raw_cards: true,
        }
    }
}

fn is_raw_card_source(source_type: &str) -> bool {
    let normalized = source_type.trim();
    normalized.eq_ignore_ascii_case("creditcard") || normalized.eq_ignore_ascii_case("credit_card")
}

/// Guards payment creation against accidental raw card PAN submission under a [`RawCardPolicy`].
///
/// Performs a case-insensitive, whitespace-trimmed check on `source_type`. When `source_type` is
/// `"creditcard"` or `"credit_card"` and `policy.allow_raw_cards` is `false`, this function fails
/// closed locally with [`MoyasarError::invalid_request`] without issuing any HTTP request or
/// including any cardholder data in the error message.
///
/// Tokenized and non-PAN sources (such as `"token"`, `"applepay"`, `"samsungpay"`, `"stcpay"`,
/// `"sadad"`, and `"sadadbill"`) are allowed under [`RawCardPolicy::default_deny`].
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with `InvalidRequestError` when `source_type` is a raw card
/// source (`"creditcard"` or `"credit_card"`) and `policy.allow_raw_cards` is `false`.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::pan_and_status::{RawCardPolicy, guard_raw_pan};
///
/// assert!(guard_raw_pan("creditcard", RawCardPolicy::default_deny()).is_err());
/// assert!(guard_raw_pan("token", RawCardPolicy::default_deny()).is_ok());
/// assert!(guard_raw_pan("creditcard", RawCardPolicy::opt_in_raw_cards()).is_ok());
/// ```
pub fn guard_raw_pan(source_type: &str, policy: RawCardPolicy) -> Result<(), MoyasarError> {
    if !policy.allow_raw_cards && is_raw_card_source(source_type) {
        return Err(MoyasarError::invalid_request(
            "raw creditcard PAN submission is denied by default policy; use a tokenized source or opt in explicitly via RawCardPolicy::opt_in_raw_cards()",
        ));
    }
    Ok(())
}

/// Normalized payment lifecycle status derived from a Moyasar payment snapshot.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::pan_and_status::MappedPaymentStatus;
///
/// assert_ne!(MappedPaymentStatus::Failed, MappedPaymentStatus::Paid);
/// assert_eq!(
///     MappedPaymentStatus::SetupCompleted,
///     MappedPaymentStatus::SetupCompleted
/// );
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MappedPaymentStatus {
    /// Payment was created and is awaiting further progression with no immediate interactive
    /// action payload.
    Initiated,
    /// Payment requires a customer action described by [`MappedNextAction`] (3DS redirect,
    /// STC Pay OTP challenge, or offline SADAD bill settlement).
    RequiresAction,
    /// Funds have been authorized and held pending capture.
    Authorized,
    /// Previously authorized funds have been captured.
    Captured,
    /// Payment has completed and settled as paid.
    Paid,
    /// Payment has been refunded.
    Refunded,
    /// Payment authorization has been voided before capture.
    Voided,
    /// Payment failed or was declined (even when returned inside an HTTP 201 creation body).
    Failed,
    /// Card verification / setup flow succeeded (`"verified"` in Moyasar).
    SetupCompleted,
}

/// Structured customer follow-up action required to complete an `"initiated"` Moyasar payment.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::pan_and_status::MappedNextAction;
///
/// let action = MappedNextAction::OtpChallenge {
///     submission_url: "https://api.moyasar.com/v1/stc_pays/otp".to_owned(),
/// };
/// assert!(matches!(action, MappedNextAction::OtpChallenge { .. }));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MappedNextAction {
    /// Browser redirect challenge (for example, 3-D Secure authentication on card/token flows).
    Redirect {
        /// Target URL to redirect the customer's browser to.
        url: String,
    },
    /// Server-side or in-app OTP submission challenge (for example, STC Pay).
    OtpChallenge {
        /// Endpoint URL where the customer's one-time password must be submitted.
        submission_url: String,
    },
    /// Offline bill-payment instructions (for example, SADAD bill settlement).
    OfflineBill {
        /// Bill reference / number the customer uses to settle the payment offline.
        bill_reference: String,
        /// Optional hosted instructions URL provided by Moyasar.
        instructions_url: Option<String>,
    },
}

/// Combined status and optional [`MappedNextAction`] produced by [`map_moyasar_status`].
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::pan_and_status::{MappedPaymentStatus, MoyasarStatusOutcome};
///
/// let outcome = MoyasarStatusOutcome {
///     status: MappedPaymentStatus::Paid,
///     next_action: None,
/// };
/// assert_eq!(outcome.status, MappedPaymentStatus::Paid);
/// assert!(outcome.next_action.is_none());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoyasarStatusOutcome {
    /// Normalized payment status.
    pub status: MappedPaymentStatus,
    /// Required customer follow-up action, if any.
    pub next_action: Option<MappedNextAction>,
}

/// Borrowed snapshot of the Moyasar payment fields required to map HTTP 201 status and
/// next-action state without assuming HTTP 201 implies [`MappedPaymentStatus::Paid`].
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::pan_and_status::{
///     MappedPaymentStatus, MoyasarPaymentSnapshot, map_moyasar_status,
/// };
///
/// let snapshot = MoyasarPaymentSnapshot {
///     status: "failed",
///     source_type: Some("token"),
///     transaction_url: None,
///     bill_reference: None,
/// };
/// let outcome = map_moyasar_status(&snapshot).expect("known status maps cleanly");
/// assert_eq!(outcome.status, MappedPaymentStatus::Failed);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoyasarPaymentSnapshot<'a> {
    /// Raw `payment.status` string returned by Moyasar (for example, `"initiated"`, `"failed"`,
    /// `"paid"`, `"verified"`).
    pub status: &'a str,
    /// Optional `payment.source.type` string (for example, `"stcpay"`, `"sadadbill"`,
    /// `"creditcard"`, `"token"`).
    pub source_type: Option<&'a str>,
    /// Optional `payment.source.transaction_url` string used for redirects, OTP submission, or
    /// offline bill instructions.
    pub transaction_url: Option<&'a str>,
    /// Optional SADAD bill reference / bill number for offline bill payments.
    pub bill_reference: Option<&'a str>,
}

/// Static lookup table for non-`initiated` Moyasar payment statuses.
const KNOWN_STATUSES: &[(&str, MappedPaymentStatus)] = &[
    ("failed", MappedPaymentStatus::Failed),
    ("paid", MappedPaymentStatus::Paid),
    ("authorized", MappedPaymentStatus::Authorized),
    ("captured", MappedPaymentStatus::Captured),
    ("refunded", MappedPaymentStatus::Refunded),
    ("voided", MappedPaymentStatus::Voided),
    ("verified", MappedPaymentStatus::SetupCompleted),
];

fn non_empty_trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
}

fn initiated_outcome(next_action: Option<MappedNextAction>) -> MoyasarStatusOutcome {
    let status = if next_action.is_some() {
        MappedPaymentStatus::RequiresAction
    } else {
        MappedPaymentStatus::Initiated
    };
    MoyasarStatusOutcome {
        status,
        next_action,
    }
}

fn otp_action(url: Option<String>) -> Option<MappedNextAction> {
    url.map(|submission_url| MappedNextAction::OtpChallenge { submission_url })
}

fn sadad_action(bill_ref: Option<String>, url: Option<String>) -> Option<MappedNextAction> {
    bill_ref.map(|bill_reference| MappedNextAction::OfflineBill {
        bill_reference,
        instructions_url: url,
    })
}

fn redirect_action(url: Option<String>) -> Option<MappedNextAction> {
    url.map(|url| MappedNextAction::Redirect { url })
}

fn map_initiated_status(snapshot: &MoyasarPaymentSnapshot<'_>) -> MoyasarStatusOutcome {
    let source = snapshot
        .source_type
        .map(|raw| raw.trim().to_ascii_lowercase());
    let tx_url = non_empty_trimmed(snapshot.transaction_url);
    let bill_ref = non_empty_trimmed(snapshot.bill_reference);
    let next_action = match source.as_deref() {
        Some("stcpay") => otp_action(tx_url),
        Some("sadad" | "sadadbill") => sadad_action(bill_ref, tx_url),
        _ => redirect_action(tx_url),
    };
    initiated_outcome(next_action)
}

/// Maps a [`MoyasarPaymentSnapshot`] into a normalized [`MoyasarStatusOutcome`].
///
/// Always inspects `snapshot.status` (including on HTTP 201 responses) and never assumes creation
/// implies [`MappedPaymentStatus::Paid`].
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with `InvalidRequestError` when `snapshot.status` is not a
/// recognized Moyasar payment status.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::pan_and_status::{
///     MappedNextAction, MappedPaymentStatus, MoyasarPaymentSnapshot, map_moyasar_status,
/// };
///
/// let stc = MoyasarPaymentSnapshot {
///     status: "initiated",
///     source_type: Some("stcpay"),
///     transaction_url: Some("https://api.moyasar.com/v1/stc_pays/otp"),
///     bill_reference: None,
/// };
/// let outcome = map_moyasar_status(&stc).expect("stcpay initiated maps to OTP challenge");
/// assert_eq!(outcome.status, MappedPaymentStatus::RequiresAction);
/// assert_eq!(
///     outcome.next_action,
///     Some(MappedNextAction::OtpChallenge {
///         submission_url: "https://api.moyasar.com/v1/stc_pays/otp".to_owned(),
///     })
/// );
/// ```
pub fn map_moyasar_status(
    snapshot: &MoyasarPaymentSnapshot<'_>,
) -> Result<MoyasarStatusOutcome, MoyasarError> {
    let normalized = snapshot.status.trim();
    if normalized.eq_ignore_ascii_case("initiated") {
        return Ok(map_initiated_status(snapshot));
    }
    if let Some((_, mapped)) = KNOWN_STATUSES
        .iter()
        .find(|(raw, _)| raw.eq_ignore_ascii_case(normalized))
    {
        return Ok(MoyasarStatusOutcome {
            status: *mapped,
            next_action: None,
        });
    }
    Err(MoyasarError::invalid_request(format!(
        "unrecognized Moyasar payment status: {normalized:?}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::ApiErrorType;

    #[test]
    fn default_deny_rejects_raw_creditcard_pan_locally() {
        let policy = RawCardPolicy::default_deny();
        assert_eq!(policy, RawCardPolicy::default());
        assert!(!policy.allow_raw_cards);
        let raw_sources = [
            "creditcard",
            "CREDITCARD",
            " CreditCard ",
            "credit_card",
            " CREDIT_CARD ",
        ];
        for raw_source in raw_sources {
            let err = guard_raw_pan(raw_source, policy)
                .expect_err("raw PAN source must be rejected under default-deny");
            match err {
                MoyasarError::Api {
                    error_type,
                    message,
                    ..
                } => {
                    assert_eq!(error_type, ApiErrorType::InvalidRequestError);
                    assert!(!message.is_empty());
                    assert!(
                        !message.chars().any(|ch| ch.is_ascii_digit()),
                        "error message must not contain PAN digits: {message}"
                    );
                }
                other => panic!("expected InvalidRequestError, got {other:?}"),
            }
        }
    }

    #[test]
    fn opt_in_policy_allows_raw_creditcard_pan() {
        let policy = RawCardPolicy::opt_in_raw_cards();
        assert!(policy.allow_raw_cards);
        assert!(guard_raw_pan("creditcard", policy).is_ok());
        assert!(guard_raw_pan("credit_card", policy).is_ok());
        assert!(guard_raw_pan(" CreditCard ", policy).is_ok());
    }

    #[test]
    fn default_deny_allows_tokenized_and_alternative_sources() {
        let policy = RawCardPolicy::default_deny();
        for source in [
            "token",
            "applepay",
            "samsungpay",
            "stcpay",
            "sadad",
            "sadadbill",
        ] {
            assert!(
                guard_raw_pan(source, policy).is_ok(),
                "tokenized/non-PAN source {source:?} should be allowed"
            );
        }
    }

    #[test]
    fn http_201_failed_status_maps_to_failed_never_paid() {
        let snapshot = MoyasarPaymentSnapshot {
            status: "failed",
            source_type: Some("token"),
            transaction_url: None,
            bill_reference: None,
        };
        let outcome = map_moyasar_status(&snapshot).expect("failed status should map");
        assert_eq!(outcome.status, MappedPaymentStatus::Failed);
        assert_ne!(outcome.status, MappedPaymentStatus::Paid);
        assert_eq!(outcome.next_action, None);
    }

    #[test]
    fn initiated_stcpay_maps_to_otp_challenge_never_redirect() {
        let otp_url = "https://api.moyasar.com/v1/stc_pays/otp-submit";
        let snapshot = MoyasarPaymentSnapshot {
            status: "initiated",
            source_type: Some("stcpay"),
            transaction_url: Some(otp_url),
            bill_reference: None,
        };
        let outcome = map_moyasar_status(&snapshot).expect("stcpay initiated should map");
        assert_eq!(outcome.status, MappedPaymentStatus::RequiresAction);
        assert_eq!(
            outcome.next_action,
            Some(MappedNextAction::OtpChallenge {
                submission_url: otp_url.to_owned(),
            })
        );
    }

    #[test]
    fn initiated_creditcard_and_token_map_to_redirect() {
        let challenge_url = "https://api.moyasar.com/v1/3ds/challenge";
        for source in ["creditcard", "token", "applepay"] {
            let snapshot = MoyasarPaymentSnapshot {
                status: "initiated",
                source_type: Some(source),
                transaction_url: Some(challenge_url),
                bill_reference: None,
            };
            let outcome = map_moyasar_status(&snapshot).expect("3DS redirect should map");
            assert_eq!(outcome.status, MappedPaymentStatus::RequiresAction);
            assert_eq!(
                outcome.next_action,
                Some(MappedNextAction::Redirect {
                    url: challenge_url.to_owned(),
                })
            );
        }
    }

    #[test]
    fn initiated_sadadbill_and_sadad_map_to_offline_bill() {
        let url = "https://moyasar.com/sadad/instructions/900123";
        for source in ["sadadbill", "sadad", " SADADBILL "] {
            let snapshot = MoyasarPaymentSnapshot {
                status: "initiated",
                source_type: Some(source),
                transaction_url: Some(url),
                bill_reference: Some("SADAD-900123"),
            };
            let outcome = map_moyasar_status(&snapshot).expect("sadad initiated should map");
            assert_eq!(outcome.status, MappedPaymentStatus::RequiresAction);
            assert_eq!(
                outcome.next_action,
                Some(MappedNextAction::OfflineBill {
                    bill_reference: "SADAD-900123".to_owned(),
                    instructions_url: Some(url.to_owned()),
                })
            );
        }
        let no_instructions = MoyasarPaymentSnapshot {
            status: "initiated",
            source_type: Some("sadadbill"),
            transaction_url: None,
            bill_reference: Some("SADAD-900124"),
        };
        let outcome = map_moyasar_status(&no_instructions).expect("sadad without URL should map");
        assert_eq!(outcome.status, MappedPaymentStatus::RequiresAction);
        assert_eq!(
            outcome.next_action,
            Some(MappedNextAction::OfflineBill {
                bill_reference: "SADAD-900124".to_owned(),
                instructions_url: None,
            })
        );
    }

    #[test]
    fn initiated_without_action_payload_remains_initiated() {
        for source in [Some("stcpay"), Some("sadadbill"), Some("creditcard"), None] {
            let snapshot = MoyasarPaymentSnapshot {
                status: "initiated",
                source_type: source,
                transaction_url: Some("   "),
                bill_reference: None,
            };
            let outcome = map_moyasar_status(&snapshot).expect("initiated fallback should map");
            assert_eq!(outcome.status, MappedPaymentStatus::Initiated);
            assert_eq!(outcome.next_action, None);
        }
    }

    #[test]
    fn verified_maps_to_setup_completed() {
        let snapshot = MoyasarPaymentSnapshot {
            status: "verified",
            source_type: Some("token"),
            transaction_url: None,
            bill_reference: None,
        };
        let outcome = map_moyasar_status(&snapshot).expect("verified should map");
        assert_eq!(outcome.status, MappedPaymentStatus::SetupCompleted);
        assert_eq!(outcome.next_action, None);
    }

    #[test]
    fn known_lifecycle_statuses_map_accurately() {
        let cases = [
            ("paid", MappedPaymentStatus::Paid),
            (" PAID ", MappedPaymentStatus::Paid),
            ("authorized", MappedPaymentStatus::Authorized),
            ("captured", MappedPaymentStatus::Captured),
            ("refunded", MappedPaymentStatus::Refunded),
            ("voided", MappedPaymentStatus::Voided),
            ("failed", MappedPaymentStatus::Failed),
            ("verified", MappedPaymentStatus::SetupCompleted),
        ];
        for (raw_status, expected) in cases {
            let snapshot = MoyasarPaymentSnapshot {
                status: raw_status,
                source_type: Some("token"),
                transaction_url: None,
                bill_reference: None,
            };
            let outcome = map_moyasar_status(&snapshot).expect("known status should map");
            assert_eq!(outcome.status, expected);
            assert_eq!(outcome.next_action, None);
        }
    }

    #[test]
    fn unknown_status_returns_invalid_request_error() {
        for unknown in ["pending", "", "   ", "settled_unexpected"] {
            let snapshot = MoyasarPaymentSnapshot {
                status: unknown,
                source_type: Some("token"),
                transaction_url: None,
                bill_reference: None,
            };
            let err = map_moyasar_status(&snapshot)
                .expect_err("unknown payment status must return an error");
            assert!(matches!(
                err,
                MoyasarError::Api {
                    error_type: ApiErrorType::InvalidRequestError,
                    ..
                }
            ));
        }
    }
}
