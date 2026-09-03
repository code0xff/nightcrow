use super::test_server::TestServer;
use super::*;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn latest_release_at_the_current_patch_does_not_touch_the_target() {
    let server = TestServer::bind();
    let metadata = serde_json::to_vec(&json!({
        "tag_name": "v0.1.1",
        "draft": false,
        "prerelease": false,
        "assets": []
    }))
    .unwrap();
    let routes = HashMap::from([(
        "/repos/code0xff/nightcrow/releases/latest".to_owned(),
        metadata,
    )]);
    let client = Client::test(&server.base);
    let handle = server.serve(routes);
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nightcrow");
    std::fs::write(&target, b"current binary").unwrap();

    run_with(
        &client,
        &target,
        PatchVersion::requested("0.1.1").unwrap(),
        None,
        "unused-asset",
    )
    .unwrap();

    handle.join().unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"current binary");
}
