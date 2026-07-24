    use super::*;

    fn entry(name: &str, is_dir: bool) -> TreeEntry {
        TreeEntry {
            name: name.to_string(),
            is_dir,
        }
    }

    /// A tree with `src/` (dir) and `README.md` (file) at the root, and
    /// `main.rs` inside `src/`. Nothing expanded yet.
    fn sample() -> TreeView {
        let mut tv = TreeView::default();
        tv.cache.insert(
            "".to_string(),
            vec![entry("src", true), entry("README.md", false)],
        );
        tv.cache
            .insert("src".to_string(), vec![entry("main.rs", false)]);
        tv
    }

    #[test]
    fn visible_rows_shows_only_top_level_when_nothing_expanded() {
        let tv = sample();
        let rows = tv.visible_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, "src");
        assert!(rows[0].is_dir);
        assert!(!rows[0].expanded);
        assert_eq!(rows[1].path, "README.md");
    }

    #[test]
    fn visible_rows_includes_children_of_expanded_dir() {
        let mut tv = sample();
        tv.expanded.insert("src".to_string());
        let rows = tv.visible_rows();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].path, "src");
        assert!(rows[0].expanded);
        assert_eq!(rows[1].path, "src/main.rs");
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].path, "README.md");
    }

    #[test]
    fn visible_rows_skips_expanded_dir_without_cached_children() {
        // `expanded` references a dir whose children were never read: it should
        // simply contribute no child rows rather than panic.
        let mut tv = sample();
        tv.expanded.insert("src".to_string());
        tv.cache.remove("src");

        let rows = tv.visible_rows();
        assert_eq!(rows.len(), 2);
        // The directory row itself still renders as expanded.
        assert!(rows[0].expanded);
    }

    #[test]
    fn clamp_selection_pins_cursor_inside_row_count() {
        let mut tv = sample();
        tv.selected = 9;
        tv.clamp_selection(2);
        assert_eq!(tv.selected, 1);
        tv.clamp_selection(0);
        assert_eq!(tv.selected, 0);
    }

    #[test]
    fn selected_path_follows_visible_rows() {
        let mut tv = sample();
        tv.expanded.insert("src".to_string());
        tv.selected = 1;
        assert_eq!(tv.selected_path().as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn parent_path_returns_none_for_top_level() {
        assert_eq!(parent_path("README.md"), None);
        assert_eq!(parent_path("src"), None);
        assert_eq!(parent_path("src/ui/mod.rs"), Some("src/ui"));
        assert_eq!(parent_path("src/main.rs"), Some("src"));
    }

    /// Lowercased-basename index entry from a repo-relative path.
    fn idx(path: &str) -> TreeIndexEntry {
        let name = path.rsplit('/').next().unwrap_or(path);
        TreeIndexEntry {
            path: path.to_string(),
            name_lower: name.to_lowercase(),
        }
    }

    /// A deeper tree: `src/ui/mod.rs`, `src/main.rs`, `README.md`. Cache is
    /// fully populated (as `build_tree_index` would leave it) and an index is
    /// seeded so the filter can be exercised without a filesystem.
    fn indexed_sample() -> TreeView {
        let mut tv = TreeView::default();
        tv.cache.insert(
            "".to_string(),
            vec![entry("src", true), entry("README.md", false)],
        );
        tv.cache.insert(
            "src".to_string(),
            vec![entry("ui", true), entry("main.rs", false)],
        );
        tv.cache
            .insert("src/ui".to_string(), vec![entry("mod.rs", false)]);
        tv.index = vec![
            idx("src"),
            idx("README.md"),
            idx("src/ui"),
            idx("src/main.rs"),
            idx("src/ui/mod.rs"),
        ];
        tv
    }

    #[test]
    fn recompute_filter_collects_matches_and_their_ancestors() {
        let mut tv = indexed_sample();
        tv.search_query.set("main");
        tv.recompute_filter();
        assert_eq!(tv.match_count, 1);
        // The match plus the `src` ancestor; nothing else.
        let mut shown: Vec<&str> = tv.show_set.iter().map(String::as_str).collect();
        shown.sort_unstable();
        assert_eq!(shown, vec!["src", "src/main.rs"]);
    }

    #[test]
    fn filtered_visible_rows_show_match_with_ancestor_chain() {
        let mut tv = indexed_sample();
        tv.search_active = true;
        tv.search_query.set("mod");
        tv.recompute_filter();

        let rows = tv.visible_rows();
        // The whole chain src -> src/ui -> src/ui/mod.rs, each at increasing
        // depth; README.md and src/main.rs are filtered out.
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].path, "src");
        assert_eq!(rows[0].depth, 0);
        assert!(rows[0].expanded);
        assert_eq!(rows[1].path, "src/ui");
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].path, "src/ui/mod.rs");
        assert_eq!(rows[2].depth, 2);
        assert!(!rows[2].is_dir);
    }

    #[test]
    fn filter_is_case_insensitive() {
        let mut tv = indexed_sample();
        tv.search_active = true;
        tv.search_query.set("README");
        tv.recompute_filter();
        let rows = tv.visible_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "README.md");
    }

    #[test]
    fn empty_query_in_search_mode_keeps_normal_view() {
        let mut tv = indexed_sample();
        tv.search_active = true;
        // No query typed yet: the tree must not explode into a full expansion.
        tv.recompute_filter();
        assert!(!tv.search_filtering());
        let rows = tv.visible_rows();
        // Normal view with nothing expanded: only the two top-level entries.
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, "src");
        assert_eq!(rows[1].path, "README.md");
    }

    #[test]
    fn cancel_search_clears_all_transient_state() {
        let mut tv = indexed_sample();
        tv.search_active = true;
        tv.search_query.set("mod");
        tv.recompute_filter();
        tv.cancel_search();
        assert!(!tv.search_active);
        assert!(tv.search_query.is_empty());
        assert!(tv.index.is_empty());
        assert!(tv.show_set.is_empty());
        assert_eq!(tv.match_count, 0);
    }

    #[test]
    fn is_safe_rel_path_accepts_repo_internal_and_rejects_escapes() {
        assert!(is_safe_rel_path("src"));
        assert!(is_safe_rel_path("src/ui/mod.rs"));
        // Escapes / absolute / empty are rejected.
        assert!(!is_safe_rel_path(""));
        assert!(!is_safe_rel_path(".."));
        assert!(!is_safe_rel_path("../etc"));
        assert!(!is_safe_rel_path("src/../../etc"));
        assert!(!is_safe_rel_path("/etc/passwd"));
    }
