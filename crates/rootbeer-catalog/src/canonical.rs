use serde::Serialize;
use sha2::{Digest, Sha256};

/// JSON with every object's keys sorted and arrays left in order.
///
/// A digest over this is a function of the value alone, not of struct field order or where
/// a decoder kept fields it did not recognise, so a client one engine behind recomputes
/// exactly the hash its publisher recorded.
pub fn canonical_json(value: &impl Serialize) -> Result<Vec<u8>, serde_json::Error> {
    let mut value = serde_json::to_value(value)?;
    value.sort_all_objects();
    serde_json::to_vec(&value)
}

/// Lowercase hex SHA-256 of a value's canonical JSON.
pub fn canonical_sha256(value: &impl Serialize) -> Result<String, serde_json::Error> {
    let digest = Sha256::digest(canonical_json(value)?);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, serde::Deserialize)]
    struct Known {
        name: String,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    }

    #[test]
    fn a_decoder_that_keeps_unknown_fields_reproduces_the_publisher_hash() {
        let published = serde_json::json!({
            "added_later": { "z": 1, "a": [3, 1, 2] },
            "name": "fd",
            "also_new": true,
        });
        let decoded: Known = serde_json::from_value(published.clone()).unwrap();

        assert_ne!(
            serde_json::to_vec(&decoded).unwrap(),
            serde_json::to_vec(&published).unwrap(),
            "field order differs once decoded"
        );
        assert_eq!(
            canonical_sha256(&decoded).unwrap(),
            canonical_sha256(&published).unwrap()
        );
    }

    #[test]
    fn array_order_is_part_of_the_value() {
        let forward = serde_json::json!({ "checks": [["fd", "--version"], ["fd", "--help"]] });
        let reversed = serde_json::json!({ "checks": [["fd", "--help"], ["fd", "--version"]] });
        assert_ne!(
            canonical_sha256(&forward).unwrap(),
            canonical_sha256(&reversed).unwrap()
        );
    }
}
