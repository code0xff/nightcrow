//! The night scene: a crow perched on a bough under a crescent moon.
//!
//! Everything but the sky is fixed art — the bird never moves. Only the stars
//! change from frame to frame, which is what keeps the screen alive without
//! turning it into an animation.

/// Crescent moon, horns opening right.
const MOON: &[&str] = &[
    "  ▗▄▄▄▖",
    " ▟███▀▘",
    "▟███▘",
    "▐███",
    "▐███",
    "▝███▖",
    " ▜███▄▖",
    "  ▝▀▀▀▘",
];
const MOON_AT: (usize, usize) = (0, 37);

/// The crow in profile, facing the moon: beak out past the eye, tail swept back
/// and down, two legs under the body. Its last row are the legs, and they land
/// on [`BOUGH`]'s top edge — see `crow_stands_on_the_bough`.
const CROW: &[&str] = &[
    "              ▄▄▄▄",
    "            ▄███████▄",
    "           ▐████●████▄▄▄",
    "            ▀███████▀",
    "        ▄▄▄███████████▄",
    "     ▄█████████████████",
    "  ▄▄▄███████████████▀",
    "  ▀▀▀▀  ▀▀████████▀",
    "          ██   ██",
];
const CROW_AT: (usize, usize) = (2, 4);

/// The bough, with a twig rising away from the bird.
const BOUGH: &[&str] = &[
    "                                  ▄▄▄▀▀",
    "                              ▄▄▀▀",
    "▄▄▄▄▄██████████████████████████████▄▄▄▄▄▄▄▄▄",
    "   ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀",
];
const BOUGH_AT: (usize, usize) = (9, 2);

/// `(row, column, phase)`. The phase spreads the twinkle out so the sky does not
/// blink in unison.
const STARS: &[(usize, usize, usize)] = &[
    (0, 6, 0),
    (1, 20, 3),
    (2, 11, 6),
    (0, 30, 4),
    (3, 2, 8),
    (5, 28, 1),
    (1, 46, 5),
    (7, 46, 2),
    (8, 2, 7),
    (0, 34, 9),
    (6, 33, 6),
    (9, 45, 3),
    (3, 31, 2),
    (8, 31, 8),
    (2, 44, 4),
];

/// Frames in one twinkle cycle.
const CYCLE: usize = 10;

pub(super) const WIDTH: usize = 48;
pub(super) const HEIGHT: usize = 13;

/// The bird and the bough it stands on, i.e. what a cropped scene must keep.
/// Rows above this are sky and are dropped first when the area is short.
pub(super) const SUBJECT_HEIGHT: usize = HEIGHT - CROW_AT.0;

/// What a cell is, rather than how it looks — the palette lives in the renderer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ink {
    Sky,
    Bird,
    Bough,
    Moon,
    Star { bright: bool },
}

pub(super) type Cell = (char, Ink);

/// The scene at animation frame `tick`, `HEIGHT` rows of `WIDTH` cells.
pub(super) fn frame(tick: usize) -> Vec<Vec<Cell>> {
    let mut grid = vec![vec![(' ', Ink::Sky); WIDTH]; HEIGHT];

    for &(row, col, phase) in STARS {
        if let Some(cell) = star(tick, phase) {
            grid[row][col] = cell;
        }
    }
    // Painted after the stars: the sky is behind everything else.
    paint(&mut grid, MOON, MOON_AT, Ink::Moon);
    paint(&mut grid, BOUGH, BOUGH_AT, Ink::Bough);
    paint(&mut grid, CROW, CROW_AT, Ink::Bird);

    grid
}

/// A star's look this frame, or `None` while it is out.
fn star(tick: usize, phase: usize) -> Option<Cell> {
    match (tick.wrapping_add(phase)) % CYCLE {
        0 | 1 => Some(('*', Ink::Star { bright: true })),
        2..=5 => Some(('·', Ink::Star { bright: false })),
        _ => None,
    }
}

/// Overlay `art` at `at`, its spaces left transparent. Art that would fall off
/// the canvas is clipped rather than wrapped; `art_fits_the_canvas` guards that
/// the shipped art never needs it.
fn paint(grid: &mut [Vec<Cell>], art: &[&str], at: (usize, usize), ink: Ink) {
    let (row0, col0) = at;
    for (i, line) in art.iter().enumerate() {
        for (j, ch) in line.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            if let Some(cell) = grid.get_mut(row0 + i).and_then(|r| r.get_mut(col0 + j)) {
                *cell = (ch, ink);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BOUGH, BOUGH_AT, CROW, CROW_AT, CYCLE, HEIGHT, Ink, MOON, MOON_AT, SUBJECT_HEIGHT, WIDTH,
        frame,
    };

    fn inks(tick: usize) -> Vec<Vec<Ink>> {
        frame(tick)
            .into_iter()
            .map(|row| row.into_iter().map(|(_, ink)| ink).collect())
            .collect()
    }

    #[test]
    fn every_frame_fills_the_same_canvas() {
        for tick in 0..CYCLE {
            let rows = frame(tick);
            assert_eq!(rows.len(), HEIGHT, "frame {tick} height");
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(row.len(), WIDTH, "frame {tick} row {i} width");
            }
        }
    }

    #[test]
    fn art_fits_the_canvas() {
        for (name, art, at) in [
            ("moon", MOON, MOON_AT),
            ("crow", CROW, CROW_AT),
            ("bough", BOUGH, BOUGH_AT),
        ] {
            assert!(at.0 + art.len() <= HEIGHT, "{name} runs past the last row");
            for (i, line) in art.iter().enumerate() {
                let end = at.1 + line.chars().count();
                assert!(end <= WIDTH, "{name} row {i} runs past the canvas: {end}");
            }
        }
    }

    #[test]
    fn only_the_sky_changes_between_frames() {
        let first = frame(0);
        for tick in 1..CYCLE {
            let other = frame(tick);
            for (row, (a, b)) in first.iter().zip(other.iter()).enumerate() {
                for (col, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                    let sky = |ink| matches!(ink, Ink::Sky | Ink::Star { .. });
                    if sky(x.1) && sky(y.1) {
                        continue;
                    }
                    assert_eq!(
                        x, y,
                        "frame {tick} moved a fixed cell at row {row}, column {col}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_sky_does_change_between_frames() {
        assert_ne!(frame(0), frame(3), "nothing twinkled");
    }

    #[test]
    fn the_crow_stands_on_the_bough() {
        let inks = inks(0);
        let legs = CROW_AT.0 + CROW.len() - 1;
        let bough_top = BOUGH_AT.0 + 2;
        assert_eq!(
            legs + 1,
            bough_top,
            "the legs must sit on the row above the bough's top edge"
        );

        let feet: Vec<usize> = (0..WIDTH)
            .filter(|&col| inks[legs][col] == Ink::Bird)
            .collect();
        assert!(!feet.is_empty(), "no legs found on row {legs}");
        for col in feet {
            assert_eq!(
                inks[bough_top][col],
                Ink::Bough,
                "the leg at column {col} does not land on the bough"
            );
        }
    }

    #[test]
    fn a_cropped_scene_keeps_the_bird_and_the_bough() {
        let inks = inks(0);
        let kept = HEIGHT - SUBJECT_HEIGHT;
        for (row, line) in inks.iter().enumerate().take(kept) {
            assert!(
                line.iter()
                    .all(|ink| !matches!(ink, Ink::Bird | Ink::Bough)),
                "row {row} would be cropped but holds the subject"
            );
        }
    }

    /// A star behind the crow or the moon is a table entry that never shows.
    #[test]
    fn every_star_is_visible_at_its_brightest() {
        for &(row, col, phase) in super::STARS {
            let tick = (CYCLE - phase % CYCLE) % CYCLE;
            let ink = inks(tick)[row][col];
            assert!(
                matches!(ink, Ink::Star { bright: true }),
                "the star at row {row}, column {col} is hidden by {ink:?}"
            );
        }
    }

    #[test]
    fn a_wrapped_tick_still_renders() {
        assert_eq!(frame(usize::MAX).len(), HEIGHT);
        assert_eq!(frame(CYCLE), frame(0));
    }
}
