use super::*;

#[test]
fn every_clipboard_image_format_is_recognised_by_its_magic_bytes() {
    assert_eq!(image_extension(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
    assert_eq!(
        image_extension(&[0xff, 0xd8, 0xff, 0xe0, 0x00]),
        Some("jpg")
    );
    assert_eq!(image_extension(b"GIF89a....."), Some("gif"));
    assert_eq!(image_extension(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
}

#[test]
fn anything_that_is_not_an_image_is_refused() {
    assert_eq!(image_extension(b""), None);
    assert_eq!(image_extension(b"<html>"), None);
    // Long enough to be an image, and starts like one, but is not.
    assert_eq!(image_extension(b"RIFF\0\0\0\0WAVEfmt "), None);
    // A truncated WebP header must not be read past its end.
    assert_eq!(image_extension(b"RIFF\0\0\0\0WEB"), None);
}

#[test]
fn a_non_image_body_is_refused_before_anything_is_written() {
    let response = String::from_utf8_lossy(&handle_paste_image(b"not an image")).into_owned();
    assert!(response.starts_with("HTTP/1.1 415 "), "{response}");
}

#[test]
fn a_written_paste_keeps_its_bytes_and_never_lands_on_an_existing_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let png = b"\x89PNG\r\n\x1a\nbody".as_slice();

    let first = write_paste(dir.path(), png, "png").expect("write");
    let second = write_paste(dir.path(), png, "png").expect("write");

    assert_eq!(std::fs::read(&first).expect("read"), png);
    assert_eq!(first.extension().and_then(|e| e.to_str()), Some("png"));
    // Identical bytes must not overwrite an earlier paste: a pane may still be
    // about to read it.
    assert_ne!(first, second);
    assert!(first.exists() && second.exists());
}

#[test]
fn sweeping_drops_files_past_the_ttl_and_keeps_the_rest() {
    let dir = tempfile::tempdir().expect("temp dir");
    let old = dir.path().join("paste-old.png");
    let fresh = dir.path().join("paste-fresh.png");
    std::fs::write(&old, b"old").expect("write old");
    std::fs::write(&fresh, b"fresh").expect("write fresh");

    // Sweeping from a point far in the future ages both files past the TTL;
    // touching mtimes portably is what this avoids.
    sweep(dir.path(), SystemTime::now() + PASTE_TTL * 2);
    assert!(!old.exists());
    assert!(!fresh.exists());

    std::fs::write(&fresh, b"fresh").expect("rewrite fresh");
    sweep(dir.path(), SystemTime::now());
    assert!(fresh.exists());
}

#[test]
fn sweeping_drops_the_oldest_once_the_directory_is_over_the_cap() {
    let dir = tempfile::tempdir().expect("temp dir");
    for i in 0..MAX_KEPT + 5 {
        std::fs::write(dir.path().join(format!("paste-{i}.png")), b"x").expect("write");
    }
    sweep(dir.path(), SystemTime::now());
    let left = std::fs::read_dir(dir.path()).expect("read dir").count();
    assert_eq!(left, MAX_KEPT);
}
