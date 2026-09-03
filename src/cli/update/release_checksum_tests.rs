use super::test_server::TestServer;
use super::*;
use crate::cli::update::contract::Asset;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const ASSET_NAME: &str = "nightcrow-x86_64-unknown-linux-gnu";

fn asset(name: &str, url: String, bytes: &[u8], digest: Option<String>) -> Asset {
    Asset {
        name: name.to_owned(),
        browser_download_url: url,
        size: bytes.len() as u64,
        digest,
        state: "uploaded".to_owned(),
    }
}

fn release(base: &str, sums: &[u8], sums_digest: Option<String>) -> Release {
    Release {
        tag_name: "v0.1.2".to_owned(),
        draft: false,
        prerelease: false,
        assets: vec![
            asset(ASSET_NAME, format!("{base}/binary"), b"binary", None),
            asset(CHECKSUM_ASSET, format!("{base}/sums"), sums, sums_digest),
        ],
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn checksum_manifest_without_an_api_digest_is_rejected_before_download() {
    let client = Client::test("http://127.0.0.1:1");
    let release = release("http://127.0.0.1:1", b"checksum manifest", None);

    let error = expected_digest(&client, &release, ASSET_NAME).unwrap_err();

    assert!(error.to_string().contains("no trusted SHA-256 digest"));
}

#[test]
fn checksum_manifest_with_an_invalid_api_digest_is_rejected_before_download() {
    let client = Client::test("http://127.0.0.1:1");
    let release = release(
        "http://127.0.0.1:1",
        b"checksum manifest",
        Some(format!("sha512:{}", "ab".repeat(32))),
    );

    let error = expected_digest(&client, &release, ASSET_NAME).unwrap_err();

    assert!(error.to_string().contains("non-SHA-256 digest"));
}

#[test]
fn checksum_manifest_whose_api_digest_does_not_match_is_rejected() {
    let server = TestServer::bind();
    let sums = format!("{}  {ASSET_NAME}\n", "ab".repeat(32)).into_bytes();
    let release = release(
        &server.base,
        &sums,
        Some(format!("sha256:{}", sha256(b"different contents"))),
    );
    let client = Client::test(&server.base);
    let handle = server.serve(HashMap::from([("/sums".to_owned(), sums)]));

    let error = expected_digest(&client, &release, ASSET_NAME).unwrap_err();

    handle.join().unwrap();
    assert!(error.to_string().contains("SHA-256 verification failed"));
}
