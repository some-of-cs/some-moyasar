//! Strongly-typed API key wrappers with compile-time isolation and log redaction.

use core::fmt;
use secrecy::{ExposeSecret, SecretString};

use crate::kernel::error::MoyasarError;

/// Execution environment mode derived from the Moyasar API key prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// Production environment (`sk_live_` / `pk_live_`).
    Live,
    /// Sandbox environment (`sk_test_` / `pk_test_`).
    Test,
}

/// Common interface for Moyasar authentication keys.
pub trait AuthKey: Send + Sync + 'static {
    /// Exposes the underlying secret string for header encoding.
    fn expose_secret(&self) -> &str;

    /// Returns the key environment mode (`Live` or `Test`).
    fn mode(&self) -> AuthMode;
}

/// Marker trait for backend-only secret keys (`sk_live_...` / `sk_test_...`).
pub trait SecretAuthKey: AuthKey {}

/// Marker trait for client-side publishable keys (`pk_live_...` / `pk_test_...`).
pub trait PublishableAuthKey: AuthKey {}

/// Backend-only Moyasar secret key with zeroized storage and redacted `Debug`.
#[derive(Clone)]
pub struct SecretKey {
    inner: SecretString,
    mode: AuthMode,
}

/// Client-originated Moyasar publishable key with zeroized storage and redacted `Debug`.
#[derive(Clone)]
pub struct PublishableKey {
    inner: SecretString,
    mode: AuthMode,
}

impl SecretKey {
    /// Parses and validates a Moyasar secret key (`sk_live_...` or `sk_test_...`).
    pub fn new(raw: &str) -> Result<Self, MoyasarError> {
        let mode = if raw.starts_with("sk_live_") {
            AuthMode::Live
        } else if raw.starts_with("sk_test_") {
            AuthMode::Test
        } else {
            return Err(MoyasarError::InvalidKey);
        };
        Ok(Self {
            inner: SecretString::from(raw.to_owned()),
            mode,
        })
    }
}

impl PublishableKey {
    /// Parses and validates a Moyasar publishable key (`pk_live_...` or `pk_test_...`).
    pub fn new(raw: &str) -> Result<Self, MoyasarError> {
        let mode = if raw.starts_with("pk_live_") {
            AuthMode::Live
        } else if raw.starts_with("pk_test_") {
            AuthMode::Test
        } else {
            return Err(MoyasarError::InvalidKey);
        };
        Ok(Self {
            inner: SecretString::from(raw.to_owned()),
            mode,
        })
    }
}

impl AuthKey for SecretKey {
    fn expose_secret(&self) -> &str {
        self.inner.expose_secret()
    }

    fn mode(&self) -> AuthMode {
        self.mode
    }
}

impl SecretAuthKey for SecretKey {}

impl AuthKey for PublishableKey {
    fn expose_secret(&self) -> &str {
        self.inner.expose_secret()
    }

    fn mode(&self) -> AuthMode {
        self.mode
    }
}

impl PublishableAuthKey for PublishableKey {}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey([REDACTED])")
    }
}

impl fmt::Debug for PublishableKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PublishableKey([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_key_prefixes_and_modes() {
        let live = SecretKey::new("sk_live_abc123").expect("valid live secret");
        assert_eq!(live.mode(), AuthMode::Live);

        let test = SecretKey::new("sk_test_abc123").expect("valid test secret");
        assert_eq!(test.mode(), AuthMode::Test);

        let pk = PublishableKey::new("pk_test_xyz").expect("valid publishable");
        assert_eq!(pk.mode(), AuthMode::Test);

        assert!(matches!(SecretKey::new("pk_test_wrong"), Err(MoyasarError::InvalidKey)));
        assert!(matches!(SecretKey::new("abc123"), Err(MoyasarError::InvalidKey)));
        assert!(matches!(PublishableKey::new("sk_live_wrong"), Err(MoyasarError::InvalidKey)));
    }

    #[test]
    fn redacts_keys_in_debug_output() {
        let sk = SecretKey::new("sk_live_987654321secret").expect("valid key");
        let dbg = format!("{sk:?}");
        assert_eq!(dbg, "SecretKey([REDACTED])");
        assert!(!dbg.contains("sk_live"));
    }
}
