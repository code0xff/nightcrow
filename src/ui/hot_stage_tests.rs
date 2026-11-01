use super::*;
use std::time::SystemTime;

#[test]
fn classify_hot_buckets_fresh_warm_cool() {
    let window = Duration::from_secs(10);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);

    let fresh = SystemTime::UNIX_EPOCH + Duration::from_secs(99);
    assert_eq!(classify_hot(fresh, now, window), HotStage::Fresh);

    let warm = SystemTime::UNIX_EPOCH + Duration::from_secs(95);
    assert_eq!(classify_hot(warm, now, window), HotStage::Warm);

    let cool = SystemTime::UNIX_EPOCH + Duration::from_secs(80);
    assert_eq!(classify_hot(cool, now, window), HotStage::Cool);
}

#[test]
fn classify_hot_clamps_future_mtime_to_fresh() {
    let window = Duration::from_secs(10);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let future = SystemTime::UNIX_EPOCH + Duration::from_secs(110);
    assert_eq!(classify_hot(future, now, window), HotStage::Fresh);
}

#[test]
fn classify_hot_window_boundary_is_cool() {
    let window = Duration::from_secs(10);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let on_boundary = SystemTime::UNIX_EPOCH + Duration::from_secs(90);
    assert_eq!(classify_hot(on_boundary, now, window), HotStage::Cool);
}

#[test]
fn next_hot_deadline_marks_the_fresh_to_warm_boundary() {
    let window = Duration::from_secs(15);
    let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(102);

    assert_eq!(
        next_hot_deadline(mtime, now, window),
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(105))
    );
    assert_eq!(
        classify_hot(
            mtime,
            SystemTime::UNIX_EPOCH + Duration::from_secs(105),
            window
        ),
        HotStage::Warm
    );
}

#[test]
fn next_hot_deadline_marks_the_warm_to_cool_boundary() {
    let window = Duration::from_secs(15);
    let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(106);

    assert_eq!(
        next_hot_deadline(mtime, now, window),
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(115))
    );
    assert_eq!(
        classify_hot(
            mtime,
            SystemTime::UNIX_EPOCH + Duration::from_secs(115),
            window
        ),
        HotStage::Cool
    );
}

#[test]
fn next_hot_deadline_skips_warm_when_window_ends_before_five_seconds() {
    let window = Duration::from_secs(3);
    let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(101);

    assert_eq!(
        next_hot_deadline(mtime, now, window),
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(103))
    );
    assert_eq!(
        classify_hot(
            mtime,
            SystemTime::UNIX_EPOCH + Duration::from_secs(103),
            window
        ),
        HotStage::Cool
    );
}
