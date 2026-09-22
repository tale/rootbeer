use rootbeer_package::official::{decode_hex, Manifest};
use rootbeer_package::{ArtifactIndex, OfficialIndexSource, PackageIndexPin};
use rootbeer_store::hash_bytes;

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

/// Signs a complete index after checking its trust key and previous signed sequence.
pub fn sign_index(
    bytes: &[u8],
    url: &str,
    sequence: u64,
    key_der: &[u8],
    public_key: &str,
    previous: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let index: ArtifactIndex = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    index.validate_complete()?;
    rootbeer_package::index::validate_https(url)?;
    if sequence == 0 {
        return Err("publication sequence must be positive".into());
    }
    let key =
        Ed25519KeyPair::from_pkcs8(key_der).map_err(|_| "invalid Ed25519 PKCS#8 signing key")?;
    if key.public_key().as_ref() != decode_hex::<32>(public_key)? {
        return Err("signing key does not match the release verification key".into());
    }
    if let Some(bytes) = previous {
        let manifest: Manifest = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        OfficialIndexSource {
            url: url.into(),
            public_key: public_key.into(),
        }
        .verify(&manifest)?;
        if sequence <= manifest.sequence {
            return Err("publication sequence must increase".into());
        }
    }
    let pin = PackageIndexPin {
        url: url.into(),
        sha256: hash_bytes(bytes),
    };
    let message = serde_json::to_vec(&("rootbeer-index-v1", sequence, &pin.url, &pin.sha256))
        .map_err(|e| e.to_string())?;
    let signature: String = key
        .sign(&message)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    serde_json::to_vec(&Manifest {
        schema: 1,
        sequence,
        index: pin,
        signature,
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    fn fixture(root: &std::path::Path) -> ((), (), Vec<u8>) {
        let (catalog, receipt) = crate::bundle::tests::fixture(root);
        let output = root.join("bundle");
        crate::bundle_artifacts(&catalog, &[receipt], "https://example.org", &output).unwrap();
        ((), (), std::fs::read(output.join("index.json")).unwrap())
    }
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
    #[test]
    fn signer_requires_complete_coverage_matching_key_and_increasing_sequence() {
        let root = tempfile::tempdir().unwrap();
        let (_, _, incomplete) = fixture(root.path());
        let der = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(der.as_ref()).unwrap();
        let public_key = hex(key.public_key().as_ref());
        let url = "https://example.org/snapshot.json";
        assert!(
            sign_index(&incomplete, url, 1, der.as_ref(), &public_key, None)
                .unwrap_err()
                .contains("incomplete")
        );
        let mut index: ArtifactIndex = serde_json::from_slice(&incomplete).unwrap();
        index.catalog.packages.retain(|name, _| name == "xz");
        let package = index.catalog.packages.get_mut("xz").unwrap();
        package
            .default_versions
            .retain(|system, _| system == "aarch64-linux");
        for recipe in package.versions.values_mut() {
            recipe
                .platforms
                .retain(|system, _| system == "aarch64-linux");
        }
        index.catalog_sha256 = index.catalog.sha256();
        let bytes = serde_json::to_vec(&index).unwrap();
        let signed = sign_index(&bytes, url, 10, der.as_ref(), &public_key, None).unwrap();
        let source = OfficialIndexSource {
            url: url.into(),
            public_key: public_key.clone(),
        };
        source
            .verify(&serde_json::from_slice(&signed).unwrap())
            .unwrap();
        assert!(
            sign_index(&bytes, url, 10, der.as_ref(), &public_key, Some(&signed))
                .unwrap_err()
                .contains("increase")
        );
        assert!(sign_index(&bytes, url, 11, der.as_ref(), &public_key, Some(&signed)).is_ok());
        assert!(
            sign_index(&bytes, url, 11, der.as_ref(), &"0".repeat(64), None)
                .unwrap_err()
                .contains("does not match")
        );
    }
}
