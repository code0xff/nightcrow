//! The night scene: a crow perched on a bough under a crescent moon.
//!
//! The bird is fixed art and stays put: between frames only the stars twinkle
//! and, now and then, the eye blinks. That is what keeps the screen alive without
//! turning it into an animation.
//!
//! A sprite is a grid of glyphs plus an aligned ink map, one legend letter per
//! glyph (see [`ink_of`]). The glyphs carry shape and shading — `█ ▓ ▒` are
//! decreasing density, `▄ ▀` are the contour — and the map says which surface a
//! cell belongs to, leaving the colour to the renderer. The two tables must line
//! up exactly; `sprite_ink_maps_line_up_with_their_art` enforces it.

use std::sync::OnceLock;

/// What a cell belongs to, rather than how it looks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ink {
    Sky,
    Star {
        bright: bool,
    },
    Moon,
    /// The crow, unlit.
    Bird,
    /// The crow's edges facing the moon.
    BirdLit,
    /// The folded wing, a plane of its own.
    BirdWing,
    /// The crow's underside, away from the moon.
    BirdShade,
    Eye,
    Bough,
    BoughLit,
    BoughShade,
}

impl Ink {
    /// Part of the fixed drawing rather than the sky.
    pub(super) fn is_fixed(self) -> bool {
        !matches!(self, Ink::Sky | Ink::Star { .. })
    }

    /// Only the invariants that guard the art care which surface a cell is on;
    /// the renderer matches every ink separately to give it a colour.
    #[cfg(test)]
    fn is_bird(self) -> bool {
        matches!(
            self,
            Ink::Bird | Ink::BirdLit | Ink::BirdWing | Ink::BirdShade | Ink::Eye
        )
    }

    #[cfg(test)]
    fn is_bough(self) -> bool {
        matches!(self, Ink::Bough | Ink::BoughLit | Ink::BoughShade)
    }
}

/// The ink map alphabet.
fn ink_of(code: char) -> Option<Ink> {
    Some(match code {
        'b' => Ink::Bird,
        'l' => Ink::BirdLit,
        'w' => Ink::BirdWing,
        'd' => Ink::BirdShade,
        'e' => Ink::Eye,
        'r' => Ink::Bough,
        'R' => Ink::BoughLit,
        'x' => Ink::BoughShade,
        _ => return None,
    })
}

struct Sprite {
    at: (usize, usize),
    art: &'static [&'static str],
    paint: Paint,
}

enum Paint {
    /// Every glyph takes one ink.
    Flat(Ink),
    /// One legend letter per glyph, aligned with the art.
    Map(&'static [&'static str]),
}

/// Crescent moon, horns opening right.
const MOON: Sprite = Sprite {
    at: (0, 37),
    art: &[
        "  ▗▄▄▄▖",
        " ▟███▀▘",
        "▟███▘",
        "▐███",
        "▐███",
        "▝███▖",
        " ▜███▄▖",
        "  ▝▀▀▀▘",
    ],
    paint: Paint::Flat(Ink::Moon),
};

/// The crow in profile, facing the moon: beak out past the eye, folded wing over
/// the flank, tail swept back and down. Its last row are the legs, and they land
/// on the bough's top edge — see `the_crow_stands_on_the_bough`.
const CROW: Sprite = Sprite {
    at: (2, 4),
    art: &[
        "              ▄▄▄▄",
        "            ▄███████▄",
        "           ▐████●████▄▄▄",
        "            ▀███████▀",
        "        ▄▄▄▓▓▓▓▓▓▓████▄",
        "     ▄██▒▒▒▒▒▒▒▒▒████▓",
        "  ▄▄▄██▒▒▒▒▒▒▒▒█████▀",
        "  ▀▀▀▀  ▀▀████████▀",
        "          ██   ██",
    ],
    paint: Paint::Map(&[
        "              llll",
        "            bbbblllll",
        "           bbbbbebblllll",
        "            bbbbbllll",
        "        bbbwwwwwwwbbbll",
        "     bbbwwwwwwwwwbbbll",
        "  dddbbwwwwwwwwbbbbbl",
        "  dddd  dddddddddbd",
        "          bb   bb",
    ]),
};

/// The bough, with a twig rising away from the bird. Its `▄` edges catch the
/// moon and its `▀` underside stays in shadow.
const BOUGH: Sprite = Sprite {
    at: (9, 2),
    art: &[
        "                                  ▄▄▄▀▀",
        "                              ▄▄▀▀",
        "▄▄▄▄▄██████████████████████████████▄▄▄▄▄▄▄▄▄",
        "   ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀",
    ],
    paint: Paint::Map(&[
        "                                  RRRxx",
        "                              RRxx",
        "RRRRRrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrRRRRRRRRR",
        "   xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    ]),
};

const SPRITES: &[&Sprite] = &[&MOON, &BOUGH, &CROW];

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

/// The eye shuts for a single frame this often — rare enough to read as a blink
/// rather than a tic. `BLINK_AT` is where in the cycle it falls, and it is not 0
/// so that frame 0, the first thing anyone sees, has the bird looking back.
const BLINK_CYCLE: usize = 15;
const BLINK_AT: usize = 7;
const EYE_SHUT: char = '─';

/// Frames before the whole scene repeats: the twinkle and the blink together.
#[cfg(test)]
const PERIOD: usize = CYCLE * BLINK_CYCLE;

pub(super) const WIDTH: usize = 48;
pub(super) const HEIGHT: usize = 13;

/// The bird and the bough it stands on, i.e. what a cropped scene must keep.
/// Rows above this are sky and are dropped first when the area is short.
pub(super) const SUBJECT_HEIGHT: usize = HEIGHT - CROW.at.0;

pub(super) type Cell = (char, Ink);

/// The scene at animation frame `tick`, `HEIGHT` rows of `WIDTH` cells.
pub(super) fn frame(tick: usize) -> Vec<Vec<Cell>> {
    let mut grid = fixed().clone();

    for &(row, col, phase) in STARS {
        // The sky is behind everything: a star shows only where no sprite claimed
        // the cell.
        if !grid[row][col].1.is_fixed()
            && let Some(cell) = star(tick, phase)
        {
            grid[row][col] = cell;
        }
    }

    if tick % BLINK_CYCLE == BLINK_AT
        && let Some((row, col)) = eye()
    {
        grid[row][col].0 = EYE_SHUT;
    }

    grid
}

/// Where the eye sits, found once from the painted scene. `None` only if the art
/// lost its eye, which `the_crow_has_an_eye_to_blink` rules out.
fn eye() -> Option<(usize, usize)> {
    static EYE: OnceLock<Option<(usize, usize)>> = OnceLock::new();
    *EYE.get_or_init(|| {
        fixed().iter().enumerate().find_map(|(row, cells)| {
            cells
                .iter()
                .position(|&(_, ink)| ink == Ink::Eye)
                .map(|col| (row, col))
        })
    })
}

/// Everything that never changes, drawn once.
fn fixed() -> &'static Vec<Vec<Cell>> {
    static FIXED: OnceLock<Vec<Vec<Cell>>> = OnceLock::new();
    FIXED.get_or_init(|| {
        let mut grid = vec![vec![(' ', Ink::Sky); WIDTH]; HEIGHT];
        for sprite in SPRITES {
            paint(&mut grid, sprite);
        }
        grid
    })
}

/// A star's look this frame, or `None` while it is out.
fn star(tick: usize, phase: usize) -> Option<Cell> {
    match (tick.wrapping_add(phase)) % CYCLE {
        0 | 1 => Some(('*', Ink::Star { bright: true })),
        2..=5 => Some(('·', Ink::Star { bright: false })),
        _ => None,
    }
}

/// Stamp a sprite onto the grid, its spaces left transparent. Art that would
/// fall off the canvas is clipped rather than wrapped; `art_fits_the_canvas`
/// guards that the shipped sprites never need it.
fn paint(grid: &mut [Vec<Cell>], sprite: &Sprite) {
    let (row0, col0) = sprite.at;
    for (i, line) in sprite.art.iter().enumerate() {
        for (j, ch) in line.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let Some(ink) = sprite.ink_at(i, j) else {
                continue;
            };
            if let Some(cell) = grid.get_mut(row0 + i).and_then(|row| row.get_mut(col0 + j)) {
                *cell = (ch, ink);
            }
        }
    }
}

impl Sprite {
    fn ink_at(&self, row: usize, col: usize) -> Option<Ink> {
        match self.paint {
            Paint::Flat(ink) => Some(ink),
            Paint::Map(map) => map.get(row)?.chars().nth(col).and_then(ink_of),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BOUGH, CROW, CYCLE, HEIGHT, Ink, PERIOD, Paint, SPRITES, SUBJECT_HEIGHT, WIDTH, frame,
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
        for sprite in SPRITES {
            let (row0, col0) = sprite.at;
            assert!(
                row0 + sprite.art.len() <= HEIGHT,
                "a sprite at row {row0} runs past the last row"
            );
            for (i, line) in sprite.art.iter().enumerate() {
                let end = col0 + line.chars().count();
                assert!(end <= WIDTH, "sprite row {i} runs past the canvas: {end}");
            }
        }
    }

    #[test]
    fn sprite_ink_maps_line_up_with_their_art() {
        for sprite in SPRITES {
            let Paint::Map(map) = sprite.paint else {
                continue;
            };
            assert_eq!(map.len(), sprite.art.len(), "row count");
            for (i, (art, inks)) in sprite.art.iter().zip(map.iter()).enumerate() {
                let art: Vec<char> = art.chars().collect();
                let inks: Vec<char> = inks.chars().collect();
                assert_eq!(art.len(), inks.len(), "row {i} length:\n{art:?}\n{inks:?}");
                for (j, (glyph, code)) in art.iter().zip(inks.iter()).enumerate() {
                    assert_eq!(
                        *glyph == ' ',
                        *code == ' ',
                        "row {i} column {j}: glyph {glyph:?} against ink {code:?}"
                    );
                    if *code != ' ' {
                        assert!(
                            super::ink_of(*code).is_some(),
                            "row {i} column {j}: {code:?} is not in the ink legend"
                        );
                    }
                }
            }
        }
    }

    /// The wing must read as its own surface, or the bird is a blob.
    #[test]
    fn the_crow_is_shaded_in_every_surface_it_has() {
        let inks = inks(0);
        for wanted in [
            Ink::Bird,
            Ink::BirdLit,
            Ink::BirdWing,
            Ink::BirdShade,
            Ink::Eye,
        ] {
            let count = inks.iter().flatten().filter(|&&ink| ink == wanted).count();
            assert!(count > 0, "{wanted:?} never reaches the canvas");
        }
        for wanted in [Ink::Bough, Ink::BoughLit, Ink::BoughShade] {
            assert!(
                inks.iter().flatten().any(|&ink| ink == wanted),
                "{wanted:?} never reaches the canvas"
            );
        }
    }

    /// The bird holds still: the sky twinkles and the eye blinks, and nothing else
    /// may differ from one frame to the next.
    #[test]
    fn only_the_sky_and_the_blink_change_between_frames() {
        let first = frame(0);
        for tick in 1..PERIOD {
            let other = frame(tick);
            for (row, (a, b)) in first.iter().zip(other.iter()).enumerate() {
                for (col, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                    if !x.1.is_fixed() && !y.1.is_fixed() {
                        continue;
                    }
                    if x.1 == Ink::Eye && y.1 == Ink::Eye {
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
    fn the_crow_has_an_eye_to_blink() {
        let (row, col) = super::eye().expect("the crow has an eye");
        assert_eq!(frame(0)[row][col], ('●', Ink::Eye));
    }

    #[test]
    fn the_eye_shuts_for_one_frame_and_opens_again() {
        let (row, col) = super::eye().unwrap();
        let glyph = |tick: usize| frame(tick)[row][col].0;

        let shut: Vec<usize> = (0..PERIOD)
            .filter(|&tick| glyph(tick) == super::EYE_SHUT)
            .collect();
        assert_eq!(
            shut.len(),
            PERIOD / super::BLINK_CYCLE,
            "expected one blink per blink cycle, got {shut:?}"
        );
        for tick in shut {
            assert_eq!(glyph(tick + 1), '●', "the eye stayed shut after {tick}");
            assert_eq!(glyph(tick - 1), '●', "the eye was shut before {tick}");
        }
    }

    #[test]
    fn the_sky_does_change_between_frames() {
        assert_ne!(frame(0), frame(3), "nothing twinkled");
    }

    #[test]
    fn the_crow_stands_on_the_bough() {
        let inks = inks(0);
        let legs = CROW.at.0 + CROW.art.len() - 1;
        let bough_top = BOUGH.at.0 + 2;
        assert_eq!(
            legs + 1,
            bough_top,
            "the legs must sit on the row above the bough's top edge"
        );

        let feet: Vec<usize> = (0..WIDTH)
            .filter(|&col| inks[legs][col].is_bird())
            .collect();
        assert!(!feet.is_empty(), "no legs found on row {legs}");
        for col in feet {
            assert!(
                inks[bough_top][col].is_bough(),
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
                line.iter().all(|ink| !ink.is_bird() && !ink.is_bough()),
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
        assert_eq!(frame(PERIOD), frame(0));
    }
}
