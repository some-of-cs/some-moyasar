//! Body-injected `given_id` idempotency policy.

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::kernel::error::MoyasarError;

/// Policy controlling how `given_id` UUIDs are generated for mutating requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GivenIdPolicy {
    /// Generates a fresh random UUIDv4 per operation.
    Random,
    /// Derives a deterministic UUIDv5 from a namespace and scope string.
    Deterministic {
        /// Fixed UUIDv5 namespace.
        namespace: Uuid,
        /// Stable scope identifier (e.g. `"{sub_id}:{cycle_index}"`).
        scope: String,
    },
}

impl GivenIdPolicy {
    /// Derives a UUID according to the configured policy.
    pub fn derive(&self) -> Uuid {
        match self {
            Self::Random => Uuid::new_v4(),
            Self::Deterministic { namespace, scope } => {
                Uuid::new_v5(namespace, scope.as_bytes())
            }
        }
    }
}

/// Injects `given_id` into a JSON object payload if not already present.
///
/// Returns `Ok(None)` if `payload` is not a JSON object, `Ok(Some(uuid))` when
/// a valid `given_id` is preserved or newly derived, or `Err(MoyasarError::InvalidKey)`
/// if an explicit `given_id` field is malformed or not a valid UUID string.
pub fn inject_given_id(
    payload: &mut Value,
    policy: &GivenIdPolicy,
) -> Result<Option<Uuid>, MoyasarError> {
    let Some(obj): Option<&mut Map<String, Value>> = payload.as_object_mut() else {
        return Ok(None);
    };
    if let Some(existing) = obj.get("given_id") {
        let raw = existing.as_str().ok_or(MoyasarError::InvalidKey)?;
        let parsed = Uuid::parse_str(raw).map_err(|_| MoyasarError::InvalidKey)?;
        return Ok(Some(parsed));
    }
    let id = policy.derive();
    obj.insert("given_id".to_owned(), Value::String(id.to_string()));
    Ok(Some(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn injects_uuidv4_when_given_id_absent() {
        let mut body = json!({ "amount": 1000 });
        let id = inject_given_id(&mut body, &GivenIdPolicy::Random)
            .expect("valid")
            .expect("injected");
        assert_eq!(body["given_id"], id.to_string());
    }

    #[test]
    fn preserves_explicit_given_id() {
        let explicit = "3fa85f64-5717-4562-b3fc-2c963f66afa6";
        let mut body = json!({ "amount": 1000, "given_id": explicit });
        let id = inject_given_id(&mut body, &GivenIdPolicy::Random)
            .expect("valid")
            .expect("preserved");
        assert_eq!(id.to_string(), explicit);
    }

    #[test]
    fn rejects_malformed_explicit_given_id() {
        let mut bad_str = json!({ "amount": 1000, "given_id": "not-a-uuid" });
        assert!(matches!(
            inject_given_id(&mut bad_str, &GivenIdPolicy::Random),
            Err(MoyasarError::InvalidKey)
        ));

        let mut bad_type = json!({ "amount": 1000, "given_id": 12345 });
        assert!(matches!(
            inject_given_id(&mut bad_type, &GivenIdPolicy::Random),
            Err(MoyasarError::InvalidKey)
        ));
    }
}
