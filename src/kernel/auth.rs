//! Strongly-typed API key wrappers with compile-time isolation and log redaction.

use core::fmt;
use secrecy::{ExposeSecret, SecretString};
use serde::Serializer;
#[cfg(feature = "env-keys")]
use zeroize::Zeroize;

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
///
/// # Examples
///
/// A [`PublishableKey`] does not implement `SecretAuthKey` (`TC-1.1.0-03`):
///
/// ```compile_fail
/// use some_moyasar::prelude::{PublishableKey, SecretAuthKey};
///
/// fn require_secret_key<K: SecretAuthKey>(_key: &K) {}
///
/// let pk = PublishableKey::new("pk_test_abc123").unwrap();
/// require_secret_key(&pk);
/// ```
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
    ///
    /// Bare prefixes with an empty suffix (e.g., `"sk_live_"`, `"sk_test_"`) are rejected.
    /// With the `env-keys` feature enabled, values starting with `$` (`$VAR` or `${VAR}`)
    /// are resolved from the process environment or `.env*` files before validation.
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::prelude::{AuthKey, AuthMode, SecretKey};
    ///
    /// let key = SecretKey::new("sk_test_abc123").unwrap();
    /// assert_eq!(key.mode(), AuthMode::Test);
    /// assert!(SecretKey::new("sk_test_").is_err());
    /// ```
    pub fn new(raw: &str) -> Result<Self, MoyasarError> {
        let (inner, mode) = resolve_and_build(raw, parse_secret_mode)?;
        Ok(Self { inner, mode })
    }
}

impl PublishableKey {
    /// Parses and validates a Moyasar publishable key (`pk_live_...` or `pk_test_...`).
    ///
    /// Bare prefixes with an empty suffix (e.g., `"pk_live_"`, `"pk_test_"`) are rejected.
    /// With the `env-keys` feature enabled, values starting with `$` (`$VAR` or `${VAR}`)
    /// are resolved from the process environment or `.env*` files before validation.
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::prelude::{AuthKey, AuthMode, PublishableKey};
    ///
    /// let key = PublishableKey::new("pk_live_xyz789").unwrap();
    /// assert_eq!(key.mode(), AuthMode::Live);
    /// assert!(PublishableKey::new("pk_live_").is_err());
    /// ```
    ///
    /// `PublishableKey` does not implement [`SecretAuthKey`] (`TC-1.1.0-03`):
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{PublishableKey, SecretAuthKey};
    ///
    /// fn assert_secret<K: SecretAuthKey>(_key: &K) {}
    ///
    /// let pk = PublishableKey::new("pk_test_abc123").unwrap();
    /// assert_secret(&pk);
    /// ```
    pub fn new(raw: &str) -> Result<Self, MoyasarError> {
        let (inner, mode) = resolve_and_build(raw, parse_publishable_mode)?;
        Ok(Self { inner, mode })
    }
}

fn has_non_empty_suffix(key: &str, prefix: &str) -> bool {
    key.strip_prefix(prefix).is_some_and(|s| !s.is_empty())
}

fn parse_secret_mode(key: &str) -> Result<AuthMode, MoyasarError> {
    if has_non_empty_suffix(key, "sk_live_") {
        Ok(AuthMode::Live)
    } else if has_non_empty_suffix(key, "sk_test_") {
        Ok(AuthMode::Test)
    } else {
        Err(MoyasarError::InvalidKey)
    }
}

fn parse_publishable_mode(key: &str) -> Result<AuthMode, MoyasarError> {
    if has_non_empty_suffix(key, "pk_live_") {
        Ok(AuthMode::Live)
    } else if has_non_empty_suffix(key, "pk_test_") {
        Ok(AuthMode::Test)
    } else {
        Err(MoyasarError::InvalidKey)
    }
}

#[cfg(feature = "env-keys")]
fn resolve_and_build(
    raw: &str,
    parse_mode: fn(&str) -> Result<AuthMode, MoyasarError>,
) -> Result<(SecretString, AuthMode), MoyasarError> {
    if raw.starts_with('$') {
        let mut resolved = crate::kernel::env::resolve_env_secret(raw)?;
        let mode = parse_mode(&resolved);
        let inner = SecretString::from(resolved.as_str());
        resolved.zeroize();
        return Ok((inner, mode?));
    }
    let mode = parse_mode(raw)?;
    Ok((SecretString::from(raw), mode))
}

#[cfg(not(feature = "env-keys"))]
fn resolve_and_build(
    raw: &str,
    parse_mode: fn(&str) -> Result<AuthMode, MoyasarError>,
) -> Result<(SecretString, AuthMode), MoyasarError> {
    let mode = parse_mode(raw)?;
    Ok((SecretString::from(raw), mode))
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

/// Serializes a [`SecretString`] by exposing its inner UTF-8 string slice.
pub fn serialize_secret<S: Serializer>(
    secret: &SecretString,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(secret.expose_secret())
}

/// Serializes an optional [`SecretString`] by exposing its inner UTF-8 string slice when `Some`.
pub fn serialize_opt_secret<S: Serializer>(
    secret: &Option<SecretString>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match secret {
        Some(value) => serializer.serialize_some(value.expose_secret()),
        None => serializer.serialize_none(),
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

        let pk_live = PublishableKey::new("pk_live_xyz").expect("valid live publishable");
        assert_eq!(pk_live.mode(), AuthMode::Live);

        let pk_test = PublishableKey::new("pk_test_xyz").expect("valid test publishable");
        assert_eq!(pk_test.mode(), AuthMode::Test);

        for invalid_sk in ["pk_test_wrong", "abc123", "sk_live_", "sk_test_", ""] {
            assert!(
                matches!(SecretKey::new(invalid_sk), Err(MoyasarError::InvalidKey)),
                "expected InvalidKey for {invalid_sk:?}"
            );
        }

        for invalid_pk in ["sk_live_wrong", "abc123", "pk_live_", "pk_test_", ""] {
            assert!(
                matches!(
                    PublishableKey::new(invalid_pk),
                    Err(MoyasarError::InvalidKey)
                ),
                "expected InvalidKey for {invalid_pk:?}"
            );
        }
    }

    #[test]
    fn redacts_keys_in_debug_output() {
        let sk = SecretKey::new("sk_live_987654321secret").expect("valid key");
        let sk_dbg = format!("{sk:?}");
        assert_eq!(sk_dbg, "SecretKey([REDACTED])");
        assert!(!sk_dbg.contains("sk_live"));

        let pk = PublishableKey::new("pk_test_987654321pub").expect("valid key");
        let pk_dbg = format!("{pk:?}");
        assert_eq!(pk_dbg, "PublishableKey([REDACTED])");
        assert!(!pk_dbg.contains("pk_test"));
    }

    #[cfg(feature = "env-keys")]
    mod env_resolution_tests {
        use super::*;
        use std::fs;
        use std::path::PathBuf;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicU64, Ordering};

        static CWD_LOCK: Mutex<()> = Mutex::new(());
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        struct ScopedDir {
            original: PathBuf,
            temp: PathBuf,
        }

        impl ScopedDir {
            fn enter(prefix: &str, env_content: &str) -> Self {
                let id = COUNTER.fetch_add(1, Ordering::Relaxed);
                let temp = std::env::temp_dir().join(format!(
                    "some_moyasar_auth_test_{prefix}_{}_{id}",
                    std::process::id()
                ));
                fs::create_dir_all(&temp).expect("create temp dir");
                fs::write(temp.join(".env"), env_content).expect("write .env");
                let original = std::env::current_dir().expect("read current_dir");
                std::env::set_current_dir(&temp).expect("set current_dir");
                Self { original, temp }
            }
        }

        impl Drop for ScopedDir {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.original);
                let _ = fs::remove_dir_all(&self.temp);
            }
        }

        #[test]
        fn resolves_secret_and_publishable_keys_from_dotenv() {
            let _guard = CWD_LOCK.lock().expect("lock cwd");
            let _dir = ScopedDir::enter(
                "valid",
                "MOYASAR_SK=sk_test_from_dotenv\nMOYASAR_PK=pk_live_from_dotenv\n",
            );

            let sk = SecretKey::new("$MOYASAR_SK").expect("resolve secret key");
            assert_eq!(sk.mode(), AuthMode::Test);
            assert_eq!(sk.expose_secret(), "sk_test_from_dotenv");

            let pk = PublishableKey::new("${MOYASAR_PK}").expect("resolve publishable key");
            assert_eq!(pk.mode(), AuthMode::Live);
            assert_eq!(pk.expose_secret(), "pk_live_from_dotenv");
        }

        #[test]
        fn rejects_invalid_or_missing_env_keys() {
            let _guard = CWD_LOCK.lock().expect("lock cwd");
            let _dir =
                ScopedDir::enter("invalid", "BAD_SK=pk_test_wrong_prefix\nBARE_SK=sk_test_\n");

            assert!(matches!(
                SecretKey::new("$BAD_SK"),
                Err(MoyasarError::InvalidKey)
            ));
            assert!(matches!(
                SecretKey::new("$BARE_SK"),
                Err(MoyasarError::InvalidKey)
            ));
            assert!(matches!(
                SecretKey::new("$MISSING_MOYASAR_KEY_XYZ"),
                Err(MoyasarError::InvalidKey)
            ));
            assert!(matches!(
                SecretKey::new("$PATH"),
                Err(MoyasarError::InvalidKey)
            ));
        }
    }
}
