use super::{body_of, get, login, post, server};
use crate::test_util::make_repo;

#[test]
fn reordering_the_tabs_changes_the_served_order() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let (dir_c, c) = make_repo();
    let server = server(&[a, b, c]);
    let token = login(server.addr());

    let list = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
    let ids: Vec<String> = value["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["id"].as_str().unwrap().to_string())
        .collect();
    let body = format!(
        "{{\"order\":[\"{}\",\"{}\",\"{}\"]}}",
        ids[2], ids[0], ids[1]
    );

    let response = post(server.addr(), "/api/repos/order", &body, Some(&token));
    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    let echoed: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    let echoed_ids: Vec<&str> = echoed["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        echoed_ids,
        vec![ids[2].as_str(), ids[0].as_str(), ids[1].as_str()]
    );

    let after = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&after)).unwrap();
    let after_ids: Vec<&str> = value["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        after_ids,
        vec![ids[2].as_str(), ids[0].as_str(), ids[1].as_str()]
    );
    drop((dir_a, dir_b, dir_c));
}

#[test]
fn reordering_requires_authentication() {
    let (dir, path) = make_repo();
    let server = server(&[path]);

    let response = post(server.addr(), "/api/repos/order", "{\"order\":[]}", None);

    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    drop(dir);
}
