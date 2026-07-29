use crate::config::{Accent, ThemeConfig};

#[test]
fn theme_default_matches_documented_preset() {
    let cfg = ThemeConfig::default();

    assert_eq!(cfg.name, Accent::Yellow);
    assert_eq!(cfg.preset_index(), 0);
}

#[test]
fn accent_index_from_index_roundtrip_for_every_variant() {
    // Pin the ALL slice against the enum: a missing entry would make
    // `index()` return 0 silently, miscolouring a real variant as the
    // default. Iterate every variant via a match so a future variant
    // addition forces this test to be updated.
    let all = [
        Accent::Yellow,
        Accent::Cyan,
        Accent::Green,
        Accent::Magenta,
        Accent::Blue,
    ];
    for a in all {
        let idx = a.index();
        assert!(idx < Accent::ALL.len(), "{a:?} index {idx} out of range");
        assert_eq!(Accent::from_index(idx), a, "roundtrip failed for {a:?}");
    }
    // And confirm the canonical slice length stays in sync.
    assert_eq!(Accent::ALL.len(), all.len());
}

#[test]
fn accent_from_index_wraps_out_of_range() {
    // Defensive: a hand-edited viewer.json with a huge accent must not
    // panic — `from_index` wraps via `%`. The compile-time guard above
    // keeps `ALL` non-empty so `% len` is sound.
    assert_eq!(
        Accent::from_index(usize::MAX),
        Accent::from_index(usize::MAX % Accent::ALL.len())
    );
    assert_eq!(Accent::from_index(Accent::ALL.len()), Accent::from_index(0));
}
