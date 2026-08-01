use super::*;
use tempfile::TempDir;

/// A temp directory with `dirs` created inside it (slash-separated paths are
/// created level by level), plus its canonical path — macOS hands out temp paths
/// under a symlinked `/var`, and the browser reports canonical roots.
///
/// On Windows the verbatim `\\\\?\\` prefix is stripped so the path can be
/// re-canonicalised by `PathTree::open` (which rejects verbatim paths with
/// trailing slashes or `..` components).
fn tree(dirs: &[&str]) -> (TempDir, PathBuf) {
    let root = TempDir::new().expect("a temp dir");
    for d in dirs {
        let mut p = root.path().to_path_buf();
        for part in d.split('/') {
            p.push(part);
            if !p.is_dir() {
                std::fs::create_dir(&p).expect("create dir");
            }
        }
    }
    let canonical = std::fs::canonicalize(root.path()).expect("canonical temp path");
    // Strip `\\\\?\\` so the path can round-trip through `canonicalize` again.
    #[cfg(windows)]
    let canonical = {
        let s = canonical.to_string_lossy();
        PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
    };
    (root, canonical)
}

fn text(path: &Path) -> String {
    let s = path.to_str().expect("a UTF-8 temp path").to_string();
    // Normalise to forward slashes so test assertions are platform-consistent.
    #[cfg(windows)]
    let s = s.replace('\\', "/");
    s
}

fn names(tree: &PathTree) -> Vec<(usize, &str)> {
    tree.rows()
        .iter()
        .map(|r| (r.depth, r.name.as_str()))
        .collect()
}

#[test]
fn opening_on_a_directory_lists_its_sub_directories_sorted() {
    let (_guard, root) = tree(&["zeta", "alpha", ".hidden"]);

    let picker = PathTree::open(&text(&root)).expect("a readable root");

    assert_eq!(
        names(&picker),
        vec![(0, "."), (0, ".."), (0, "alpha"), (0, "zeta")],
        "sorted, directories only, hidden left out"
    );
}

#[test]
fn opening_on_a_half_typed_name_falls_back_to_its_directory() {
    let (_guard, root) = tree(&["alpha"]);

    let picker = PathTree::open(&format!("{}/alp", text(&root))).expect("a readable root");

    assert_eq!(picker.root_label(), format!("{}/", text(&root)));
    assert_eq!(names(&picker), vec![(0, "."), (0, ".."), (0, "alpha")]);
}

#[test]
fn opening_on_an_unreadable_path_yields_nothing_to_draw() {
    let (_guard, root) = tree(&[]);

    assert!(PathTree::open(&format!("{}/missing/deeper", text(&root))).is_none());
}

#[test]
fn expanding_splices_children_below_their_parent() {
    let (_guard, root) = tree(&["alpha/inner", "zeta"]);
    let mut picker = PathTree::open(&text(&root)).expect("a readable root");

    // Skip `.` and `..` to reach "alpha" at index 2.
    picker.move_selection(true);
    picker.move_selection(true);
    picker.expand();

    assert_eq!(
        names(&picker),
        vec![(0, "."), (0, ".."), (0, "alpha"), (1, "inner"), (0, "zeta")]
    );
    assert!(picker.rows()[2].expanded);
}

#[test]
fn collapsing_removes_the_whole_subtree_below_the_row() {
    let (_guard, root) = tree(&["alpha/inner/deepest", "zeta"]);
    let mut picker = PathTree::open(&text(&root)).expect("a readable root");
    // Skip `.` and `..` to reach "alpha" at index 2.
    picker.move_selection(true);
    picker.move_selection(true);
    picker.expand();
    picker.move_selection(true);
    picker.expand();
    assert_eq!(picker.rows().len(), 6, "., .., alpha, inner, deepest, zeta");

    picker.move_selection(false);
    picker.collapse_or_up();

    assert_eq!(
        names(&picker),
        vec![(0, "."), (0, ".."), (0, "alpha"), (0, "zeta")]
    );
    assert!(!picker.rows()[2].expanded);
}

#[test]
fn left_on_a_collapsed_child_steps_to_its_parent_row() {
    let (_guard, root) = tree(&["alpha/inner"]);
    let mut picker = PathTree::open(&text(&root)).expect("a readable root");
    // Skip `.` and `..` to reach "alpha" at index 2.
    picker.move_selection(true);
    picker.move_selection(true);
    picker.expand();
    picker.move_selection(true);
    assert_eq!(picker.selected(), 3);

    picker.collapse_or_up();

    assert_eq!(picker.selected(), 2, "the nearest shallower row above it");
}

#[test]
fn left_at_the_root_re_roots_to_the_parent_and_selects_where_it_came_from() {
    let (_guard, root) = tree(&["alpha"]);
    let inner = root.join("alpha");
    let mut picker = PathTree::open(&text(&inner)).expect("a readable root");

    picker.collapse_or_up();

    assert_eq!(picker.root_label(), text(&root));
    assert_eq!(
        picker.rows()[picker.selected()].name,
        "alpha",
        "the directory just stepped out of stays selected"
    );
}

#[test]
fn re_rooting_keeps_the_users_own_notation() {
    let (_guard, root) = tree(&["alpha/inner"]);
    // A trailing separator and a `..` hop: both have to survive the step up as
    // text rather than being canonicalized into an absolute path.
    let typed = format!("{}/alpha/../alpha/inner/", text(&root));
    let mut picker = PathTree::open(&typed).expect("a readable root");

    picker.collapse_or_up();

    assert_eq!(
        picker.root_label(),
        format!("{}/alpha/../alpha", text(&root))
    );
}

#[test]
fn picking_a_row_returns_the_typed_root_plus_a_trailing_separator() {
    let (_guard, root) = tree(&["alpha/inner"]);
    let mut picker = PathTree::open(&format!("{}/", text(&root))).expect("a readable root");
    // Skip `.` and `..` to reach "alpha" at index 2, then expand and reach "inner" at index 3.
    picker.move_selection(true);
    picker.move_selection(true);
    picker.expand();
    picker.move_selection(true);

    assert_eq!(
        picker.selected_path(),
        format!("{}/alpha/inner/", text(&root)),
        "the separator lets Tab carry on descending in the field"
    );
}

#[test]
fn picking_in_an_empty_directory_yields_the_root_itself() {
    let (_guard, root) = tree(&[]);
    let picker = PathTree::open(&text(&root)).expect("a readable root");

    assert_eq!(names(&picker), vec![(0, "."), (0, "..")]);
    assert_eq!(picker.selected_path(), format!("{}/", text(&root)));
}

#[test]
fn moving_the_selection_clamps_at_both_ends() {
    let (_guard, root) = tree(&["alpha", "zeta"]);
    let mut picker = PathTree::open(&text(&root)).expect("a readable root");

    picker.move_selection(false);
    assert_eq!(picker.selected(), 0);
    picker.move_selection(true);
    picker.move_selection(true);
    picker.move_selection(true);
    picker.move_selection(true);
    picker.move_selection(true);
    assert_eq!(
        picker.selected(),
        3,
        "clamped, not wrapped to the first row"
    );
}

#[test]
fn a_home_relative_root_falls_back_to_an_absolute_parent() {
    // `~` has no expressible parent, so the one rewrite the dialog allows.
    let Some(home) = std::fs::canonicalize(expand_tilde("~")).ok() else {
        return;
    };
    let Some(parent) = home.parent().map(Path::to_path_buf) else {
        return;
    };
    let mut picker = PathTree::open("~").expect("a readable home");
    assert_eq!(picker.root_label(), "~");

    picker.collapse_or_up();

    assert_eq!(picker.root_label(), parent.to_string_lossy());
}

#[test]
fn parent_text_walks_up_without_reading_the_filesystem() {
    assert_eq!(parent_text("", '/').as_deref(), Some(".."));
    assert_eq!(parent_text(".", '/').as_deref(), Some(".."));
    assert_eq!(parent_text("..", '/').as_deref(), Some("../.."));
    assert_eq!(parent_text("nightcrow", '/').as_deref(), Some("."));
    assert_eq!(parent_text("~/coding/", '/').as_deref(), Some("~"));
    assert_eq!(parent_text("/Users", '/').as_deref(), Some("/"));
    assert_eq!(parent_text("/", '/'), None, "the root has no parent");
}
