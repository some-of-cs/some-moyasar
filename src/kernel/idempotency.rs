//! Body-injected `given_id` idempotency policy.

use serde_json::{Map, Value};
use uuid::Uuid;

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
pub fn inject_given_id(payload: &mut Value, policy: &GivenIdPolicy) -> Option<Uuid> {
    let obj: &mut Map<String, Value> = payload.as_object_mut()?;
    if let Some(existing) = obj.get("given_id").and_then(Value::as_str)
        && let Ok(parsed) = Uuid::parse_str(existing)
    {
        return Some(parsed);
    }
    let id = policy.derive();
    obj.insert("given_id".to_owned(), Value::String(id.to_string()));
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn injects_uuidv4_when_given_id_absent() {
        let mut body = json!({ "amount": 1000 });
        let id = inject_given_id(&mut body, &GivenIdPolicy::Random).expect("injected");
        assert_eq!(body["given_id"], id.to_string());
    }

    #[test]
    fn preserves_explicit_given_id() {
        let explicit = "3fa85f64-5717-4562-b3fc-2c963f66afa6";
        let mut body = json!({ "amount": 1000, "given_id": explicit });
        let id = inject_given_id(&mut body, &GivenIdPolicy::Random).expect("preserved");
        assert_eq!(id.to_string(), explicit);
    }
}
