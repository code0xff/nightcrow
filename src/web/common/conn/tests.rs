use super::*;

fn head_with(headers: &[(&str, &str)]) -> RequestHead {
    let mut text = String::from("GET / HTTP/1.1\r\n");
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str("\r\n");
    http::parse_request_head(&text).unwrap()
}

#[test]
fn completed_head_over_the_limit_is_rejected() {
    let mut bytes = vec![b'a'; MAX_HEAD_BYTES];
    bytes.extend_from_slice(b"\r\n\r\n");

    let error = request_head_end(&bytes).unwrap_err();

    assert!(error.to_string().contains("request head exceeds"));
}

#[test]
fn host_allowed_accepts_loopback_spellings() {
    for host in [
        "localhost:8091",
        "LOCALHOST",
        "127.0.0.1:8091",
        "127.0.0.1",
        "[::1]:8091",
        "127.0.0.2",
    ] {
        assert!(
            host_allowed(&head_with(&[("Host", host)]), true),
            "{host} must be accepted"
        );
    }
}

#[test]
fn host_allowed_refuses_a_rebound_name_on_a_loopback_bind() {
    let head = head_with(&[("Host", "evil.example"), ("Origin", "http://evil.example")]);

    assert!(origin_allowed(&head), "precondition: origin matches host");
    assert!(!host_allowed(&head, true), "a rebound name must be refused");
}

#[test]
fn host_allowed_defers_when_bound_off_loopback() {
    let head = head_with(&[("Host", "nightcrow.internal")]);

    assert!(!host_allowed(&head, true));
    assert!(host_allowed(&head, false));
}

#[test]
fn connection_slot_refuses_over_the_cap() {
    let counter = Arc::new(AtomicUsize::new(0));

    let held: Vec<_> = (0..2)
        .map(|_| ConnectionSlot::acquire(&counter, 2).expect("under the cap"))
        .collect();

    assert!(ConnectionSlot::acquire(&counter, 2).is_none());
    assert_eq!(counter.load(Ordering::Acquire), 2);
    drop(held);
}

#[test]
fn connection_slot_releases_on_drop() {
    let counter = Arc::new(AtomicUsize::new(0));

    drop(ConnectionSlot::acquire(&counter, 1).expect("under the cap"));

    assert_eq!(counter.load(Ordering::Acquire), 0);
    assert!(ConnectionSlot::acquire(&counter, 1).is_some());
}
