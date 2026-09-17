use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

use age::secrecy::zeroize::Zeroize;

use crate::plan::AgeIdentity;

fn identities(source: &AgeIdentity) -> io::Result<Vec<age::x25519::Identity>> {
    let mut text = match source {
        AgeIdentity::File(path) => fs::read_to_string(path)?,
        AgeIdentity::OnePassword(reference) => {
            let output = Command::new("op")
                .args(["read", "--no-newline", reference])
                .output()?;
            if !output.status.success() {
                return Err(io::Error::other("age identity: op read failed"));
            }
            String::from_utf8(output.stdout)
                .map_err(|_| io::Error::other("age identity is not UTF-8"))?
        }
    };
    let result = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            line.parse::<age::x25519::Identity>()
                .map_err(|_| io::Error::other("expected a native AGE-SECRET-KEY identity"))
        })
        .collect::<io::Result<Vec<_>>>();
    text.zeroize();
    let keys = result?;
    if keys.is_empty() {
        return Err(io::Error::other("age identity contains no keys"));
    }
    Ok(keys)
}

pub(crate) fn decrypt(path: &Path, identity: &AgeIdentity) -> io::Result<Vec<u8>> {
    let keys = identities(identity)?;
    let input = age::armor::ArmoredReader::new(fs::File::open(path)?);
    let decryptor = age::Decryptor::new(input).map_err(io::Error::other)?;
    let mut reader = decryptor
        .decrypt(keys.iter().map(|key| key as &dyn age::Identity))
        .map_err(io::Error::other)?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use age::secrecy::ExposeSecret;

    pub(crate) fn fixture(dir: &Path, plaintext: &[u8], armored: bool) -> AgeIdentity {
        let key = age::x25519::Identity::generate();
        let ciphertext = if armored {
            age::encrypt_and_armor(&key.to_public(), plaintext)
                .unwrap()
                .into_bytes()
        } else {
            age::encrypt(&key.to_public(), plaintext).unwrap()
        };
        fs::write(dir.join("secret.age"), ciphertext).unwrap();
        fs::write(
            dir.join("key.txt"),
            format!(
                "# generated test key\n\n{}\n",
                key.to_string().expose_secret()
            ),
        )
        .unwrap();
        AgeIdentity::File(dir.join("key.txt"))
    }

    #[test]
    fn decrypts_binary_and_armored_files_with_newlines_intact() {
        for armored in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let plaintext = b"a\n\n\x00\xff\n";
            let identity = fixture(dir.path(), plaintext, armored);
            assert_eq!(
                decrypt(&dir.path().join("secret.age"), &identity).unwrap(),
                plaintext
            );
        }
    }

    #[test]
    fn rejects_wrong_keys_and_corrupted_ciphertext() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let identity = fixture(dir.path(), b"private", false);
        let wrong = fixture(other.path(), b"other", false);
        let path = dir.path().join("secret.age");
        assert!(decrypt(&path, &wrong).is_err());
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(decrypt(&path, &identity).is_err());
    }

    #[test]
    fn invalid_identity_errors_do_not_echo_key_material() {
        let dir = tempfile::tempdir().unwrap();
        let identity = fixture(dir.path(), b"private", false);
        fs::write(dir.path().join("key.txt"), "secret-invalid-key").unwrap();
        let error = decrypt(&dir.path().join("secret.age"), &identity).unwrap_err();
        assert!(!error.to_string().contains("secret-invalid-key"));
        fs::write(dir.path().join("key.txt"), "# empty\n").unwrap();
        assert!(decrypt(&dir.path().join("secret.age"), &identity).is_err());
    }
}
