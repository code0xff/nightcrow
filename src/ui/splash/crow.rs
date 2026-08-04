//! The splash crow: one fixed body silhouette with a wing flapping over it.

/// A perched crow silhouette — head with a beak and eye on the upper-left,
/// body sweeping down to a pointed tail on the lower-right.
const BODY: &[&str] = &[
    "      ▄▄▄",
    "    ▄█▀ ▀█▄",
    "   ▄█  ●  █▄",
    "   █▀▀   ▀▀█▄▄▄",
    "    ██████████████▄",
    "     ███████████████▄",
    "      ████████████████▄",
    "       █████████████████▄",
    "        ██████████████████▄",
    "         ███████████████████",
    "          ████████████████▀",
    "           ██████████████▀",
    "            ████████████▀",
    "             ██████████▀",
    "              ████████▀",
    "               ██████▀",
    "                ████▀",
    "                 ██▀",
];

/// Blank canvas rows above the body, leaving the raised wing somewhere to go.
const BODY_TOP: usize = 1;

/// Canvas row where the wing joins the back — every wing's last row lands here.
const WING_ROOT: usize = 4;

/// Wing positions from outstretched to raised. Rows run top-down; a wing may be
/// at most `WING_ROOT + 1` rows tall so its root stays on the back.
const WINGS: &[&[&str]] = &[
    &["            ▄██████████▀▀"],
    &["                 ▄██████▀", "            ▄███████▀"],
    &[
        "                    ▄████▀",
        "                ▄█████▀",
        "            ▄██████▀",
    ],
    &[
        "                     ▄███▀",
        "                  ▄████▀",
        "               ▄█████▀",
        "            ▄██████▀",
    ],
    &[
        "                    ▄██▀",
        "                  ▄███▀",
        "                ▄████▀",
        "              ▄█████▀",
        "            ▄██████▀",
    ],
];

/// Which wing each animation frame shows: one flap, held at both extremes.
const FLAP: &[usize] = &[0, 1, 2, 3, 4, 4, 3, 2, 1, 0];

/// Every row is padded to this width so the centred logo keeps its column
/// instead of drifting as the wing sweeps out.
pub(super) const WIDTH: usize = 28;

pub(super) const HEIGHT: usize = BODY_TOP + BODY.len();

/// The crow for animation frame `tick`, one padded row per canvas line.
pub(super) fn frame(tick: usize) -> Vec<String> {
    let wing = WINGS[FLAP[tick % FLAP.len()]];
    let wing_top = WING_ROOT + 1 - wing.len();

    (0..HEIGHT)
        .map(|row| {
            let mut cells = vec![' '; WIDTH];
            if let Some(art) = row.checked_sub(BODY_TOP).and_then(|i| BODY.get(i)) {
                paint(&mut cells, art);
            }
            if let Some(art) = row.checked_sub(wing_top).and_then(|i| wing.get(i)) {
                paint(&mut cells, art);
            }
            cells.into_iter().collect()
        })
        .collect()
}

/// Overlay `art` onto `cells`, its spaces left transparent.
fn paint(cells: &mut Vec<char>, art: &str) {
    for (col, ch) in art.chars().enumerate() {
        if ch == ' ' {
            continue;
        }
        if col >= cells.len() {
            cells.resize(col + 1, ' ');
        }
        cells[col] = ch;
    }
}

#[cfg(test)]
mod tests {
    use super::{BODY, BODY_TOP, FLAP, HEIGHT, WIDTH, WING_ROOT, WINGS, frame};

    fn padded_body_row(row: usize) -> String {
        let art = BODY[row - BODY_TOP];
        format!("{art:<WIDTH$}")
    }

    #[test]
    fn every_flap_frame_is_the_same_size() {
        for tick in 0..FLAP.len() {
            let rows = frame(tick);
            assert_eq!(rows.len(), HEIGHT, "frame {tick} height");
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(
                    row.chars().count(),
                    WIDTH,
                    "frame {tick} row {i} is not padded to the canvas width: {row:?}"
                );
            }
        }
    }

    #[test]
    fn the_flap_cycle_uses_every_wing_and_returns_to_its_start() {
        let mut used: Vec<usize> = FLAP.to_vec();
        used.sort_unstable();
        used.dedup();
        assert_eq!(used, (0..WINGS.len()).collect::<Vec<_>>());
        assert_eq!(frame(FLAP.len()), frame(0));
    }

    #[test]
    fn each_wing_is_rooted_on_the_back_without_a_gap() {
        let back = BODY[WING_ROOT - BODY_TOP];
        let back_end = back.trim_end().chars().count() - 1;

        for (stage, wing) in WINGS.iter().enumerate() {
            assert!(
                wing.len() <= WING_ROOT + 1,
                "wing {stage} is too tall to keep its root on the back"
            );
            let root = wing.last().expect("a wing has at least one row");
            let start = root
                .chars()
                .position(|ch| ch != ' ')
                .expect("a wing row is not blank");
            assert!(
                start <= back_end + 1,
                "wing {stage} starts at column {start}, detached from the back \
                 (painted through column {back_end})"
            );
        }
    }

    #[test]
    fn no_wing_paints_over_the_body_below_the_back() {
        for tick in 0..FLAP.len() {
            for (row, painted) in frame(tick).iter().enumerate().skip(WING_ROOT + 1) {
                assert_eq!(
                    *painted,
                    padded_body_row(row),
                    "frame {tick} altered body row {row}"
                );
            }
        }
    }

    #[test]
    fn no_wing_covers_the_eye() {
        for tick in 0..FLAP.len() {
            assert!(
                frame(tick).iter().any(|row| row.contains('●')),
                "frame {tick} lost the crow's eye"
            );
        }
    }

    #[test]
    fn a_wrapped_tick_still_renders_a_frame() {
        assert_eq!(frame(usize::MAX).len(), HEIGHT);
        assert_eq!(frame(usize::MAX), frame(usize::MAX % FLAP.len()));
    }
}
