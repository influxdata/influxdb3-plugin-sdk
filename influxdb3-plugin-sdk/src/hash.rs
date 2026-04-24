//! SHA-256 hashing of archive bytes.
//!
//! Every published artifact's hash is SHA-256 of the tar.gz bytes, rendered
//! as `sha256:<64 lowercase hex chars>`. Returned as [`ArtifactHash`], the
//! schemas-crate newtype that enforces this canonical string form.

use influxdb3_plugin_schemas::ArtifactHash;
use sha2::{Digest, Sha256};

use crate::SdkError;

/// Returns the SHA-256 hash of `bytes` as an [`ArtifactHash`] in the
/// canonical `sha256:<64 lowercase hex chars>` form.
pub fn sha256_of_bytes(bytes: &[u8]) -> ArtifactHash {
    let digest = Sha256::digest(bytes);
    let hex = encode_lowercase_hex(&digest);
    let raw = format!("sha256:{hex}");
    // Format is constructed correctly above; a failure here is a bug in
    // this module, not a caller error.
    ArtifactHash::try_new(&raw).expect("sha256 digest is always canonical form")
}

/// Streaming variant of [`sha256_of_bytes`]; reads 8 KiB at a time for
/// archives too large to keep fully in memory.
///
/// Returns `Err(SdkError::Hash { source })` on read failure.
pub fn sha256_of_reader<R: std::io::Read>(mut reader: R) -> Result<ArtifactHash, SdkError> {
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|source| SdkError::Hash { source })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let hex = encode_lowercase_hex(&digest);
    let raw = format!("sha256:{hex}");
    Ok(ArtifactHash::try_new(&raw).expect("sha256 digest is always canonical form"))
}

fn encode_lowercase_hex(bytes: &[u8]) -> String {
    // Inline loop avoids pulling in the `hex` crate.
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(nibble_to_hex(byte >> 4));
        out.push(nibble_to_hex(byte & 0xF));
    }
    out
}

fn nibble_to_hex(n: u8) -> char {
    debug_assert!(n < 16);
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'a' + (n - 10)) as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn empty_bytes_hash() {
        let h = sha256_of_bytes(b"");
        assert_eq!(
            h.as_str(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hello_bytes_hash() {
        let h = sha256_of_bytes(b"hello");
        assert_eq!(
            h.as_str(),
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn deterministic_across_calls() {
        let sample = b"InfluxDB 3 plugin SDK canonical test vector";
        let first = sha256_of_bytes(sample);
        let second = sha256_of_bytes(sample);
        assert_eq!(first, second);
    }

    #[test]
    fn reader_matches_bytes_for_identical_input() {
        let sample: Vec<u8> = (0u8..=255).cycle().take(16_384).collect();
        let from_bytes = sha256_of_bytes(&sample);
        let from_reader = sha256_of_reader(sample.as_slice()).unwrap();
        assert_eq!(from_bytes, from_reader);
    }

    #[test]
    fn reader_propagates_io_errors_as_hash_variant() {
        struct AlwaysErr;
        impl std::io::Read for AlwaysErr {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("forced"))
            }
        }
        let err = sha256_of_reader(AlwaysErr).unwrap_err();
        assert!(matches!(err, SdkError::Hash { .. }));
    }
}
