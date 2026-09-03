use super::*;

fn asset(name: &str, size: u64) -> Asset {
    Asset {
        name: name.to_owned(),
        browser_download_url: "https://example.invalid/asset".to_owned(),
        size,
        digest: None,
        state: "uploaded".to_owned(),
    }
}

#[test]
fn versions_accept_only_canonical_patch_series_values() {
    assert_eq!(
        PatchVersion::requested("0.1.0").unwrap().to_string(),
        "0.1.0"
    );
    assert_eq!(PatchVersion::tag("v0.1.42").unwrap().to_string(), "0.1.42");
    for invalid in ["v0.1.1", "0.2.1", "0.1.01", "0.1.-1", "0.1.1.0"] {
        assert!(
            PatchVersion::requested(invalid).is_err(),
            "accepted {invalid}"
        );
    }
    for invalid in ["0.1.1", "v0.2.1", "v0.1.01", "v0.1.x"] {
        assert!(PatchVersion::tag(invalid).is_err(), "accepted {invalid}");
    }
}

#[test]
fn platform_assets_are_allowlisted() {
    assert_eq!(
        platform_asset("windows", "x86_64").unwrap(),
        "nightcrow-x86_64-pc-windows-msvc.exe"
    );
    assert!(platform_asset("linux", "aarch64").is_err());
    assert!(platform_asset("freebsd", "x86_64").is_err());
}

#[test]
fn release_assets_must_be_unique_uploaded_and_bounded() {
    let duplicate = Release {
        tag_name: "v0.1.2".to_owned(),
        draft: false,
        prerelease: false,
        assets: vec![asset("nightcrow", 12), asset("nightcrow", 12)],
    };
    assert!(duplicate.asset("nightcrow", 100).is_err());

    let oversized = Release {
        assets: vec![asset("nightcrow", 101)],
        ..duplicate
    };
    assert!(oversized.asset("nightcrow", 100).is_err());
}

#[test]
fn checksums_require_one_exact_asset_entry() {
    let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sums = format!("{digest}  other\n{digest} *nightcrow\n");
    assert_eq!(
        checksum_for(&sums, "nightcrow").unwrap(),
        parse_digest(digest).unwrap()
    );
    assert!(checksum_for(&format!("{digest}  other\n"), "nightcrow").is_err());
    assert!(
        checksum_for(
            &format!("{digest}  nightcrow\n{digest}  nightcrow\n"),
            "nightcrow"
        )
        .is_err()
    );
}

#[test]
fn api_digests_must_be_sha256() {
    let mut release_asset = asset("nightcrow", 12);
    release_asset.digest = Some(format!("sha256:{}", "ab".repeat(32)));
    assert_eq!(release_asset.api_digest().unwrap().unwrap(), [0xab; 32]);
    release_asset.digest = Some(format!("sha512:{}", "ab".repeat(32)));
    assert!(release_asset.api_digest().is_err());
}

#[test]
fn release_metadata_must_match_the_requested_stable_patch() {
    let mut release = Release {
        tag_name: "v0.1.2".to_owned(),
        draft: false,
        prerelease: false,
        assets: Vec::new(),
    };
    let requested = PatchVersion::requested("0.1.2").unwrap();
    assert_eq!(release.validate(Some(requested)).unwrap(), requested);

    release.tag_name = "v0.1.3".to_owned();
    assert!(release.validate(Some(requested)).is_err());
    release.tag_name = "v0.1.2".to_owned();
    release.prerelease = true;
    assert!(release.validate(Some(requested)).is_err());
}
