use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use super::test_server::TestServer;

const ASSET_NAME: &str = "nightcrow-x86_64-unknown-linux-gnu";

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn metadata(base: &str, tag: &str, binary: &[u8], digest: Option<String>) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "tag_name": tag,
        "draft": false,
        "prerelease": false,
        "assets": [{
            "name": ASSET_NAME,
            "browser_download_url": format!("{base}/binary"),
            "size": binary.len(),
            "digest": digest,
            "state": "uploaded"
        }]
    }))
    .unwrap()
}

fn latest_path() -> String {
    "/repos/code0xff/nightcrow/releases/latest".to_owned()
}

#[test]
fn a_verified_latest_release_replaces_the_target() {
    let server = TestServer::bind();
    let binary = b"verified release binary";
    let routes = HashMap::from([
        (
            latest_path(),
            metadata(
                &server.base,
                "v0.1.2",
                binary,
                Some(format!("sha256:{}", sha256(binary))),
            ),
        ),
        ("/binary".to_owned(), binary.to_vec()),
    ]);
    let client = Client::test(&server.base);
    let handle = server.serve(routes);
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nightcrow");
    std::fs::write(&target, b"old binary").unwrap();

    run_with(
        &client,
        &target,
        PatchVersion::requested("0.1.1").unwrap(),
        None,
        ASSET_NAME,
    )
    .unwrap();

    handle.join().unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), binary);
}

#[test]
fn a_hash_mismatch_leaves_the_target_unchanged() {
    let server = TestServer::bind();
    let binary = b"tampered binary";
    let routes = HashMap::from([
        (
            latest_path(),
            metadata(
                &server.base,
                "v0.1.2",
                binary,
                Some(format!("sha256:{}", "00".repeat(32))),
            ),
        ),
        ("/binary".to_owned(), binary.to_vec()),
    ]);
    let client = Client::test(&server.base);
    let handle = server.serve(routes);
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nightcrow");
    std::fs::write(&target, b"old binary").unwrap();

    let error = run_with(
        &client,
        &target,
        PatchVersion::requested("0.1.1").unwrap(),
        None,
        ASSET_NAME,
    )
    .unwrap_err();

    handle.join().unwrap();
    assert!(error.to_string().contains("SHA-256 verification failed"));
    assert_eq!(std::fs::read(&target).unwrap(), b"old binary");
}

#[test]
fn checksum_manifest_is_used_when_the_asset_digest_is_absent() {
    let server = TestServer::bind();
    let binary = b"manifest verified binary";
    let sums = format!("{}  {ASSET_NAME}\n", sha256(binary)).into_bytes();
    let mut value: serde_json::Value =
        serde_json::from_slice(&metadata(&server.base, "v0.1.2", binary, None)).unwrap();
    value["assets"].as_array_mut().unwrap().push(json!({
        "name": CHECKSUM_ASSET,
        "browser_download_url": format!("{}/sums", server.base),
        "size": sums.len(),
        "digest": format!("sha256:{}", sha256(&sums)),
        "state": "uploaded"
    }));
    let routes = HashMap::from([
        (latest_path(), serde_json::to_vec(&value).unwrap()),
        ("/sums".to_owned(), sums),
        ("/binary".to_owned(), binary.to_vec()),
    ]);
    let client = Client::test(&server.base);
    let handle = server.serve(routes);
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nightcrow");
    std::fs::write(&target, b"old binary").unwrap();

    run_with(
        &client,
        &target,
        PatchVersion::requested("0.1.1").unwrap(),
        None,
        ASSET_NAME,
    )
    .unwrap();

    handle.join().unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), binary);
}

#[test]
fn an_explicit_older_patch_is_installed_from_its_tag_endpoint() {
    let server = TestServer::bind();
    let binary = b"older verified binary";
    let requested = PatchVersion::requested("0.1.0").unwrap();
    let routes = HashMap::from([
        (
            "/repos/code0xff/nightcrow/releases/tags/v0.1.0".to_owned(),
            metadata(
                &server.base,
                "v0.1.0",
                binary,
                Some(format!("sha256:{}", sha256(binary))),
            ),
        ),
        ("/binary".to_owned(), binary.to_vec()),
    ]);
    let client = Client::test(&server.base);
    let handle = server.serve(routes);
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nightcrow");
    std::fs::write(&target, b"newer binary").unwrap();

    run_with(
        &client,
        &target,
        PatchVersion::requested("0.1.1").unwrap(),
        Some(requested),
        ASSET_NAME,
    )
    .unwrap();

    handle.join().unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), binary);
}
