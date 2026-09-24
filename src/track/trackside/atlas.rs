//! One texture for every piece of fine detail beside the road: advertising,
//! braking board numbers, start post chequers, tyre wall belts and the crowd.
//!
//! Detail drawn as geometry flickers: lettering laid on a board fights the
//! board for the same depth, and strokes thinner than a pixel crawl as the car
//! moves. Painted here instead, each face is one quad, and the mip chain
//! (averaged in linear light, like the surface textures) fades detail evenly
//! with distance. Painted once at startup; each cell has a gutter of its own
//! edge colour so the smaller mips do not bleed one cell into the next.

use super::super::{boards, textures};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

pub(super) const WIDTH: usize = 1024;
pub(super) const HEIGHT: usize = 512;
/// Clean up to the fourth mip; below that a cell is a few texels on screen.
const GUTTER: usize = 8;
/// Samples per texel edge when painting, for smooth edges before mipmapping.
const SUPERSAMPLE: usize = 3;

pub(super) const BRANDS: usize = 6;
pub(super) const BELTS: usize = 2;
pub(super) const CROWDS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::track) enum Cell {
    /// An advertising board: Omarchy, or one of the made-up brands.
    Brand(usize),
    /// A braking board: 50, 100 or 150.
    Label(usize),
    /// A start post panel.
    Chequer,
    /// A tyre wall face, belt red or white.
    Belt(usize),
    /// A row of the grandstand crowd.
    Crowd(usize),
}

impl Cell {
    /// Texel rectangle: x, y, width, height.
    pub(super) fn rect(self) -> (usize, usize, usize, usize) {
        let g = GUTTER;
        match self {
            Cell::Brand(i) => (g + (i % 2) * 512, g + (i / 2) * 80, 496, 64),
            Cell::Label(i) => (g + i * 128, g + 240, 112, 64),
            Cell::Chequer => (g + 3 * 128, g + 240, 112, 64),
            Cell::Belt(i) => (g + (4 + i) * 128, g + 240, 112, 48),
            Cell::Crowd(i) => (g + (i % 2) * 512, g + 320 + (i / 2) * 80, 496, 64),
        }
    }

    /// Texture coordinates of the cell's corners, left to right and top to
    /// bottom as the picture is seen.
    pub(in crate::track) fn uv(self) -> (Vec2, Vec2) {
        let (x, y, w, h) = self.rect();
        let size = Vec2::new(WIDTH as f32, HEIGHT as f32);
        (
            Vec2::new(x as f32, y as f32) / size,
            Vec2::new((x + w) as f32, (y + h) as f32) / size,
        )
    }

    fn every() -> Vec<Cell> {
        let mut cells = vec![Cell::Chequer];
        cells.extend((0..BRANDS).map(Cell::Brand));
        cells.extend((0..3).map(Cell::Label));
        cells.extend((0..BELTS).map(Cell::Belt));
        cells.extend((0..CROWDS).map(Cell::Crowd));
        cells
    }

    /// The colour at `(u, v)` across the cell, `v` up, in sRGB.
    fn paint(self, u: f32, v: f32) -> (f32, f32, f32) {
        match self {
            Cell::Brand(i) => brand(i, u, v),
            Cell::Label(i) => boards::paint_label(i, u, v),
            Cell::Chequer => {
                if ((u * 4.0) as usize + (v * 2.0) as usize).is_multiple_of(2) {
                    (0.93, 0.93, 0.91)
                } else {
                    (0.07, 0.07, 0.08)
                }
            }
            Cell::Belt(i) => belt(i, u, v),
            Cell::Crowd(i) => crowd(i, u, v),
        }
    }
}

/// The atlas, with its mip chain and a sampler for grazing angles.
pub(super) fn image() -> Image {
    let linear = |c: f32| c.powf(2.2);
    let mut texels = vec![(0.3f32, 0.3f32, 0.3f32); WIDTH * HEIGHT];
    for cell in Cell::every() {
        let (x0, y0, w, h) = cell.rect();
        for y in y0 - GUTTER..y0 + h + GUTTER {
            for x in x0 - GUTTER..x0 + w + GUTTER {
                let mut sum = (0.0, 0.0, 0.0);
                for sy in 0..SUPERSAMPLE {
                    for sx in 0..SUPERSAMPLE {
                        let fx = x as f32 + (sx as f32 + 0.5) / SUPERSAMPLE as f32;
                        let fy = y as f32 + (sy as f32 + 0.5) / SUPERSAMPLE as f32;
                        // The gutter repeats the cell's own edge.
                        let u = ((fx - x0 as f32) / w as f32).clamp(0.0, 1.0);
                        let v = 1.0 - ((fy - y0 as f32) / h as f32).clamp(0.0, 1.0);
                        let c = cell.paint(u, v);
                        sum.0 += linear(c.0);
                        sum.1 += linear(c.1);
                        sum.2 += linear(c.2);
                    }
                }
                let n = (SUPERSAMPLE * SUPERSAMPLE) as f32;
                texels[y * WIDTH + x] = (sum.0 / n, sum.1 / n, sum.2 / n);
            }
        }
    }
    let srgb = |c: f32| (c.powf(1.0 / 2.2) * 255.0).round().clamp(0.0, 255.0) as u8;
    let data = texels
        .iter()
        .flat_map(|&(r, g, b)| [srgb(r), srgb(g), srgb(b), 255])
        .collect();
    let mut image = Image::new(
        Extent3d {
            width: WIDTH as u32,
            height: HEIGHT as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    textures::mipmaps(&mut image);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    });
    image
}

/// A board's brand: a made-up one, drawn as blocks of lettering, or Omarchy.
struct Brand {
    ground: (f32, f32, f32),
    letters: (f32, f32, f32),
    /// An accent block before a made-up word; for Omarchy, the logo mark's
    /// colour.
    accent: Option<(f32, f32, f32)>,
    /// Letter widths of the "word", as fractions of the board; empty for
    /// Omarchy, whose wordmark is drawn instead.
    word: &'static [f32],
}

/// Omarchy's green and the Tokyo Night background and foreground it sits on.
const OMARCHY_GREEN: (f32, f32, f32) = (0.620, 0.808, 0.416);
const TOKYO_NIGHT: (f32, f32, f32) = (0.102, 0.106, 0.149);
const TOKYO_LIGHT: (f32, f32, f32) = (0.753, 0.792, 0.961);

const BRAND: [Brand; BRANDS] = [
    // Omarchy as it is drawn: green mark and wordmark on Tokyo Night.
    Brand {
        ground: TOKYO_NIGHT,
        letters: OMARCHY_GREEN,
        accent: Some(OMARCHY_GREEN),
        word: &[],
    },
    Brand {
        ground: (0.97, 0.8, 0.1),
        letters: (0.08, 0.08, 0.09),
        accent: None,
        word: &[0.09, 0.06, 0.1, 0.09, 0.07],
    },
    Brand {
        ground: (0.78, 0.1, 0.1),
        letters: (0.96, 0.96, 0.95),
        accent: Some((0.96, 0.96, 0.95)),
        word: &[0.06, 0.06, 0.05, 0.08, 0.06, 0.06, 0.05],
    },
    // Omarchy the other way round: Tokyo Night on green.
    Brand {
        ground: OMARCHY_GREEN,
        letters: TOKYO_NIGHT,
        accent: Some(TOKYO_NIGHT),
        word: &[],
    },
    // Omarchy with a pale wordmark beside the green mark.
    Brand {
        ground: TOKYO_NIGHT,
        letters: TOKYO_LIGHT,
        accent: Some(OMARCHY_GREEN),
        word: &[],
    },
    Brand {
        ground: (0.05, 0.5, 0.52),
        letters: (0.96, 0.96, 0.95),
        accent: None,
        word: &[0.07, 0.07, 0.05, 0.09, 0.06, 0.07],
    },
];

fn brand(i: usize, u: f32, v: f32) -> (f32, f32, f32) {
    let brand = &BRAND[i];
    if brand.word.is_empty() {
        return omarchy(brand, u, v);
    }
    let inside =
        |u0: f32, u1: f32, v0: f32, v1: f32| (u0..u1).contains(&u) && (v0..v1).contains(&v);
    let gap = 0.022;
    let width: f32 = brand.word.iter().sum::<f32>() + gap * (brand.word.len() - 1) as f32;
    let mut x = 0.5 - width / 2.0 + if brand.accent.is_some() { 0.06 } else { 0.0 };
    if let Some(accent) = brand.accent
        && inside(x - 0.12, x - 0.05, 0.22, 0.78)
    {
        return accent;
    }
    for (k, &w) in brand.word.iter().enumerate() {
        // Alternate letter heights read as lower and upper case at speed.
        let top = if k == 0 || k % 3 == 2 { 0.76 } else { 0.64 };
        if inside(x, x + w, 0.26, top) {
            return brand.letters;
        }
        x += w + gap;
    }
    brand.ground
}

/// The Omarchy mark and wordmark side by side, centred and the same height.
fn omarchy(brand: &Brand, u: f32, v: f32) -> (f32, f32, f32) {
    // The cell is 496 by 64 texels: a unit of height is this much of the width.
    const ASPECT: f32 = 64.0 / 496.0;
    const HEIGHT: f32 = 0.70;
    const GAP: f32 = 0.03;
    let word_aspect = (OMARCHY_WORD[0].len() as f32 * 51.0) / (OMARCHY_WORD.len() as f32 * 50.0);
    let mark_w = HEIGHT * ASPECT;
    let word_w = HEIGHT * ASPECT * word_aspect;
    let left = 0.5 - (mark_w + GAP + word_w) / 2.0;
    let bottom = (1.0 - HEIGHT) / 2.0;
    // The row counts from the top, as the bitmaps are written.
    let lit = |rows: &[&str], x0: f32, width: f32| {
        let fx = (u - x0) / width;
        let fy = (bottom + HEIGHT - v) / HEIGHT;
        if !(0.0..1.0).contains(&fx) || !(0.0..1.0).contains(&fy) {
            return false;
        }
        let row = rows[(fy * rows.len() as f32) as usize].as_bytes();
        row[(fx * row.len() as f32) as usize] == b'#'
    };
    if lit(&OMARCHY_MARK, left, mark_w) {
        brand.accent.unwrap_or(brand.letters)
    } else if lit(&OMARCHY_WORD, left + mark_w + GAP, word_w) {
        brand.letters
    } else {
        brand.ground
    }
}

/// The Omarchy logo mark, cell for cell from the official `omarchy-logo.svg`
/// (omarchy.org/brand): a 15 by 15 grid, top row first.
const OMARCHY_MARK: [&str; 15] = [
    "###############",
    "#......#......#",
    "#.######...##.#",
    "#.#.........#.#",
    "#.#.........#.#",
    "#.#.........#.#",
    "#.#.........#.#",
    "###.........#.#",
    "#.#.........#.#",
    "#.#.........#.#",
    "#.#.........#.#",
    "#.#.........#.#",
    "#.###########.#",
    "#......#......#",
    "########.######",
];

/// The Omarchy wordmark, cell for cell from the official
/// `omarchy-wordmark.svg`: the Delta Corps Priest 1 lettering on an 82 by 19
/// grid of cells 51 wide and 50 tall, top row first.
const OMARCHY_WORD: [&str; 19] = [
    ".................###.............................................................",
    "..#####......###########......#######....#######....#######....#...#......#...#..",
    ".#######....#############....########...########...########...##...##....##...##.",
    "###...###..###...###...###..###...###..###...###..###...###..###...###..###...###",
    "###...###..###...###...###..###...###..###...###..###...###..###...###..###...###",
    "###...###..###...###...###..###...###..###...###..###...##...###...###..###...###",
    "###...###..###...###...###..###...###..###...###..###...#....###...###..###...###",
    "###...###..###...###...###..###...###..###...###..###........###...###..###...###",
    "###...###..###...###...###.##########.#########...###.......###########.#########",
    "###...###..###...###...###.##########.########....###......###########..#########",
    "###...###..###...###...###..###...###..###........###........###...###........###",
    "###...###..###...###...###..###...###.##########..###...#....###...###...##...###",
    "###...###..###...###...###..###...###.##########..###...##...###...###..###...###",
    "###...###..###...###...###..###...###..###...###..###...###..###...###..###...###",
    "###...###..###...###...###..###...###..###...###..###...###..###...###..###...###",
    ".#######....##...###...##...###...##...###...###..########...###...##....#######.",
    "..#####......#...###...#....###...#....###...###..#######....###...#......#####..",
    ".......................................###...##..................................",
    ".......................................###...#...................................",
];

/// A tyre stack's face: black tyres, with the belt across the middle.
fn belt(i: usize, u: f32, v: f32) -> (f32, f32, f32) {
    const COLOURS: [(f32, f32, f32); BELTS] = [(0.8, 0.12, 0.1), (0.92, 0.92, 0.9)];
    if (0.5..0.72).contains(&v) {
        return COLOURS[i];
    }
    // The seams between tyres, a shade darker.
    let seam = (u * 4.0).fract() < 0.06 || (v * 3.0).fract() < 0.05;
    if seam {
        (0.05, 0.05, 0.06)
    } else {
        (0.11, 0.11, 0.12)
    }
}

/// A row of seated spectators, twenty-two to a grandstand module, and the
/// odd empty seat.
fn crowd(i: usize, u: f32, v: f32) -> (f32, f32, f32) {
    const PEOPLE: f32 = 22.0;
    const SEAT: (f32, f32, f32) = (0.2, 0.33, 0.58);
    const SHIRTS: [(f32, f32, f32); 10] = [
        (0.72, 0.2, 0.16),
        (0.88, 0.86, 0.82),
        (0.2, 0.27, 0.46),
        (0.82, 0.68, 0.26),
        (0.22, 0.22, 0.24),
        (0.3, 0.48, 0.32),
        (0.64, 0.5, 0.4),
        (0.42, 0.38, 0.36),
        (0.58, 0.6, 0.66),
        (0.84, 0.5, 0.26),
    ];
    const SKIN: [(f32, f32, f32); 4] = [
        (0.87, 0.7, 0.58),
        (0.66, 0.48, 0.36),
        (0.42, 0.3, 0.22),
        (0.3, 0.22, 0.17),
    ];
    let person = (u * PEOPLE).floor() as u64;
    let x = (u * PEOPLE).fract();
    let hash = |k: u64| {
        let mut h = (i as u64 * 131 + person)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(k);
        h ^= h >> 29;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 32;
        (h % 1000) as f32 / 1000.0
    };
    if hash(1) < 0.12 {
        return SEAT;
    }
    let shirt = SHIRTS[(hash(2) * SHIRTS.len() as f32) as usize];
    let skin = SKIN[(hash(3) * SKIN.len() as f32) as usize];
    // Rounded shoulders across most of the seat, and a round head above.
    // The cell is stretched about four to one across a module, so the head's
    // circle is drawn squashed by as much to come out round.
    let shoulders = v < 0.5 && (x - 0.5).abs() < 0.42 - (v - 0.35).max(0.0) * 1.2;
    let (hx, hy) = ((x - 0.5) / 0.2, (v - 0.72) / 0.2);
    if shoulders {
        shirt
    } else if hx * hx + hy * hy < 1.0 {
        skin
    } else {
        SEAT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No cell overlaps another's cell or gutter, and all lie in the atlas.
    #[test]
    fn cells_and_gutters_stay_apart() {
        let cells = Cell::every();
        let grown = |c: Cell| {
            let (x, y, w, h) = c.rect();
            (x - GUTTER, y - GUTTER, x + w + GUTTER, y + h + GUTTER)
        };
        for (k, &a) in cells.iter().enumerate() {
            let (ax0, ay0, ax1, ay1) = grown(a);
            assert!(ax1 <= WIDTH && ay1 <= HEIGHT, "{a:?} leaves the atlas");
            for &b in &cells[k + 1..] {
                let (bx0, by0, bx1, by1) = grown(b);
                assert!(
                    ax1 <= bx0 || bx1 <= ax0 || ay1 <= by0 || by1 <= ay0,
                    "{a:?} overlaps {b:?}"
                );
            }
        }
    }

    /// A full mip chain, filtered for grazing angles.
    #[test]
    fn the_atlas_is_mipmapped() {
        let image = image();
        assert_eq!(image.texture_descriptor.mip_level_count, WIDTH.ilog2() + 1);
        let ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("sampler set")
        };
        assert_eq!(sampler.anisotropy_clamp, 16);
    }
}
