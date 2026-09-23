use rootbeer_catalog::decode_hex;

/// Approves one validated package record with the publisher's Ed25519 key.
pub fn sign_package_record(
    record: &rootbeer_package::distribution::PackageRecord,
    key_der: &[u8],
    public_key: &str,
) -> Result<Vec<u8>, String> {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use rootbeer_package::distribution::{signing_message, SignedPackageRecord, RECORD_LIMIT};

    record.validate()?;
    let key =
        Ed25519KeyPair::from_pkcs8(key_der).map_err(|_| "invalid Ed25519 PKCS#8 signing key")?;
    if key.public_key().as_ref() != decode_hex::<32>(public_key)? {
        return Err("signing key does not match the package verification key".into());
    }
    let record = serde_json::value::to_raw_value(record).map_err(|error| error.to_string())?;
    let signature = key
        .sign(&signing_message(&record))
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let bytes = serde_json::to_vec(&SignedPackageRecord { record, signature })
        .map_err(|error| error.to_string())?;
    if bytes.len() > RECORD_LIMIT {
        return Err("package record exceeds 1 MiB".into());
    }
    Ok(bytes)
}
