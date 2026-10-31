use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn notice() -> Notice {
    Notice {
        repo_path: "/work/app".into(),
        pane: 3,
        pane_title: Some("claude".into()),
        message: Some("done".into()),
    }
}

#[test]
fn every_listener_hears_a_published_notice() {
    let notices = Notices::default();
    let heard = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let heard = Arc::clone(&heard);
        notices.subscribe(move |n| {
            assert_eq!(n.pane, 3);
            heard.fetch_add(1, Ordering::SeqCst);
        });
    }
    notices.publish(&notice());
    assert_eq!(heard.load(Ordering::SeqCst), 2);
}

#[test]
fn publishing_with_no_listener_is_a_no_op() {
    Notices::default().publish(&notice());
}

#[test]
fn a_listener_may_subscribe_another_without_deadlocking() {
    let notices = Arc::new(Notices::default());
    let inner = Arc::clone(&notices);
    notices.subscribe(move |_| inner.subscribe(|_| {}));
    notices.publish(&notice());
    notices.publish(&notice());
}

#[test]
fn messages_are_trimmed_emptied_and_cut() {
    assert_eq!(clean_message(None), None);
    assert_eq!(clean_message(Some("   ")), None);
    assert_eq!(clean_message(Some("  hi \n")), Some("hi".into()));
    let long = "가".repeat(NOTICE_MESSAGE_MAX_CHARS + 50);
    assert_eq!(
        clean_message(Some(&long)).unwrap().chars().count(),
        NOTICE_MESSAGE_MAX_CHARS
    );
}
