use super::{human_bytes, logged, percent, status};

#[test]
fn percent_is_clamped_and_never_divides_by_zero() {
    assert_eq!(percent(0, 100), 0);
    assert_eq!(percent(50, 100), 50);
    assert_eq!(percent(150, 100), 100);
    assert_eq!(percent(7, 0), 100);
}

#[test]
fn a_log_line_is_emitted_once_per_step() {
    assert!(logged(0, 10));
    assert!(!logged(10, 11));
    assert!(!logged(11, 19));
    assert!(logged(19, 20));
}

#[test]
fn bytes_are_scaled_to_the_largest_whole_unit() {
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(2048), "2.0 KiB");
    assert_eq!(human_bytes(13_974_528), "13.3 MiB");
}

#[test]
fn a_status_line_reports_both_sides_of_the_transfer() {
    assert_eq!(status(50, 512, 1024), "nightcrow:  50% (512 B / 1.0 KiB)");
}
