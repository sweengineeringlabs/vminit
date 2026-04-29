//! Manifest fetching and integrity verification.

use sha2::{Digest, Sha256};
use swe_justpkg_pkg::HttpClient;

/// Fetch a package manifest over HTTPS.
/// Returns the manifest body as a UTF-8 string, or an error message.
/// The caller is responsible for aborting if this returns `Err`.
pub fn fetch_manifest(http: &dyn HttpClient, url: &str) -> Result<String, String> {
    match http.get_bytes(url) {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|e| format!("manifest response is not valid UTF-8: {e}")),
        Err(e) => Err(format!("HTTP error fetching manifest from {url}: {e}")),
    }
}

/// Verify SHA-256 integrity of a manifest body against a `sha256:<hex>` spec.
/// Returns `Ok(())` on match, `Err(message)` on scheme error or hash mismatch.
pub fn verify_manifest_hash(manifest_bytes: &[u8], hash_spec: &str) -> Result<(), String> {
    let expected_hex = hash_spec
        .strip_prefix("sha256:")
        .ok_or_else(|| format!("unsupported manifest hash scheme (expected sha256:<hex>): {hash_spec}"))?;

    let digest = Sha256::digest(manifest_bytes);
    let actual_hex = hex::encode(digest);

    if actual_hex != expected_hex {
        return Err(format!(
            "manifest integrity check failed: expected sha256:{expected_hex}, got sha256:{actual_hex}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use swe_justpkg_pkg::{HttpClient, JustpkgError};

    struct ErrorHttpClient;

    impl HttpClient for ErrorHttpClient {
        fn get_bytes(&self, url: &str) -> Result<Vec<u8>, JustpkgError> {
            Err(JustpkgError::Http { url: url.to_string(), status: 503 })
        }
        fn get_stream(&self, url: &str, _dest: &mut dyn std::io::Write) -> Result<u64, JustpkgError> {
            Err(JustpkgError::Http { url: url.to_string(), status: 503 })
        }
    }

    struct OkHttpClient { body: &'static [u8] }

    impl HttpClient for OkHttpClient {
        fn get_bytes(&self, _url: &str) -> Result<Vec<u8>, JustpkgError> {
            Ok(self.body.to_vec())
        }
        fn get_stream(&self, _url: &str, dest: &mut dyn std::io::Write) -> Result<u64, JustpkgError> {
            let n = std::io::copy(&mut std::io::Cursor::new(self.body), dest)
                .map_err(JustpkgError::Io)?;
            Ok(n)
        }
    }

    #[test]
    fn test_fetch_manifest_success_returns_body_as_string() {
        // Bug caught: get_bytes result not being converted to String, returning Err on valid UTF-8.
        let http = OkHttpClient { body: b"{\"packages\":{}}" };
        let result = fetch_manifest(&http, "https://example.com/manifest.json");
        assert_eq!(result.unwrap(), "{\"packages\":{}}");
    }

    #[test]
    fn test_fetch_manifest_http_error_returns_err() {
        // Bug caught: HTTP error being swallowed and returning Ok("") instead of Err.
        let http = ErrorHttpClient;
        let result = fetch_manifest(&http, "https://example.com/manifest.json");
        assert!(result.is_err(), "HTTP 503 must return Err, not Ok");
    }

    #[test]
    fn test_fetch_manifest_http_error_message_contains_url() {
        // Bug caught: error message omitting the URL, making diagnosis harder.
        let http = ErrorHttpClient;
        let err = fetch_manifest(&http, "https://deploy.internal/manifest.json").unwrap_err();
        assert!(
            err.contains("https://deploy.internal/manifest.json"),
            "error message must contain the URL for diagnosis; got: {err}"
        );
    }

    #[test]
    fn test_fetch_manifest_non_utf8_response_returns_err() {
        // Bug caught: returning Ok(garbled) for non-UTF-8 manifest bytes (e.g. gzip body).
        let http = OkHttpClient { body: b"\xFF\xFE\x00\x01" };
        let result = fetch_manifest(&http, "https://example.com/manifest.json");
        assert!(result.is_err(), "non-UTF-8 body must return Err");
    }

    // --- verify_manifest_hash tests ---

    fn sha256_hex(data: &[u8]) -> String {
        hex::encode(Sha256::digest(data))
    }

    #[test]
    fn test_verify_manifest_hash_matching_hash_returns_ok() {
        // Bug caught: correct hash returning Err due to digest encoding mismatch.
        let body = b"{\"packages\":{\"curl\":\"/nix/store/xxx-curl\"}}";
        let spec = format!("sha256:{}", sha256_hex(body));
        assert!(
            verify_manifest_hash(body, &spec).is_ok(),
            "correct sha256 hash must return Ok"
        );
    }

    #[test]
    fn test_verify_manifest_hash_mismatched_hash_returns_err() {
        // Bug caught: hash mismatch being silently ignored, allowing tampered manifests.
        let body = b"{\"packages\":{}}";
        let wrong_spec = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let result = verify_manifest_hash(body, wrong_spec);
        assert!(result.is_err(), "wrong hash must return Err");
    }

    #[test]
    fn test_verify_manifest_hash_mismatch_error_contains_expected_and_actual() {
        // Bug caught: error message not including both hashes, making forensics impossible.
        let body = b"tampered";
        let wrong_spec = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let err = verify_manifest_hash(body, wrong_spec).unwrap_err();
        assert!(err.contains("0000000000000000000000000000000000000000000000000000000000000000"),
            "error must include expected hash; got: {err}");
        assert!(err.contains(&sha256_hex(body)),
            "error must include actual hash; got: {err}");
    }

    #[test]
    fn test_verify_manifest_hash_unsupported_scheme_returns_err() {
        // Bug caught: non-sha256 hash spec silently accepted and treated as matching.
        let body = b"manifest";
        let result = verify_manifest_hash(body, "md5:abc123");
        assert!(result.is_err(), "unsupported hash scheme must return Err");
    }

    #[test]
    fn test_verify_manifest_hash_empty_body_matches_its_own_sha256() {
        // Bug caught: empty-body edge case panicking or returning wrong hash.
        let spec = format!("sha256:{}", sha256_hex(b""));
        assert!(
            verify_manifest_hash(b"", &spec).is_ok(),
            "empty body must match sha256 of empty bytes"
        );
    }
}
