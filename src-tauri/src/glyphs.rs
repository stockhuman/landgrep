//! Geoglyphs: text carved into the terrain, Nazca-style, letters as tall as
//! you like. A 5×7 bitmap font (with the accents Québec needs) is stamped
//! onto the DEM grid. Raised letters that land on water become land, so a
//! name written across a lake comes out as a chain of islands.

/// 5×7 capitals, digits and some punctuation; `#` is ink.
const FONT: &[(char, [&str; 7])] = &[
  ('A', [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
  ('B', ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."]),
  ('C', [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."]),
  ('D', ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."]),
  ('E', ["#####", "#....", "#....", "####.", "#....", "#....", "#####"]),
  ('F', ["#####", "#....", "#....", "####.", "#....", "#....", "#...."]),
  ('G', [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".###."]),
  ('H', ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
  ('I', [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."]),
  ('J', ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."]),
  ('K', ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"]),
  ('L', ["#....", "#....", "#....", "#....", "#....", "#....", "#####"]),
  ('M', ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"]),
  ('N', ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#"]),
  ('O', [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
  ('P', ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."]),
  ('Q', [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"]),
  ('R', ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"]),
  ('S', [".####", "#....", "#....", ".###.", "....#", "....#", "####."]),
  ('T', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."]),
  ('U', ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
  ('V', ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."]),
  ('W', ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#."]),
  ('X', ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"]),
  ('Y', ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."]),
  ('Z', ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"]),
  ('0', [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."]),
  ('1', ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."]),
  ('2', [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"]),
  ('3', ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###."]),
  ('4', ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."]),
  ('5', ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."]),
  ('6', ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."]),
  ('7', ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."]),
  ('8', [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."]),
  ('9', [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."]),
  ('.', [".....", ".....", ".....", ".....", ".....", ".##..", ".##.."]),
  (',', [".....", ".....", ".....", ".....", ".##..", "..#..", ".#..."]),
  ('!', ["..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#.."]),
  ('?', [".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#.."]),
  ('\'', ["..#..", "..#..", ".#...", ".....", ".....", ".....", "....."]),
  ('-', [".....", ".....", ".....", "#####", ".....", ".....", "....."]),
  ('+', [".....", "..#..", "..#..", "#####", "..#..", "..#..", "....."]),
  (':', [".....", ".##..", ".##..", ".....", ".##..", ".##..", "....."]),
  ('/', [".....", "....#", "...#.", "..#..", ".#...", "#....", "....."]),
  ('&', [".##..", "#..#.", "#.#..", ".#...", "#.#.#", "#..#.", ".##.#"]),
  ('#', [".#.#.", ".#.#.", "#####", ".#.#.", "#####", ".#.#.", ".#.#."]),
  ('♥', [".....", ".#.#.", "#####", "#####", ".###.", "..#..", "....."]),
];

/// A diacritic: two rows above the capital (a blank row between), or for
/// the cedilla, the two below it.
enum Mark {
  Above([&'static str; 2]),
  Below([&'static str; 2]),
}

const ACUTE: Mark = Mark::Above(["...#.", "..#.."]);
const GRAVE: Mark = Mark::Above([".#...", "..#.."]);
const CIRCUMFLEX: Mark = Mark::Above(["..#..", ".#.#."]);
const DIAERESIS: Mark = Mark::Above([".....", ".#.#."]);
const CEDILLA: Mark = Mark::Below(["..#..", ".##.."]);

/// Line box in font pixels: accents, a gap, the 7-row capital, cedilla.
const CAP_TOP: usize = 3;
const LINE_ROWS: usize = CAP_TOP + 7 + 2;

fn decompose(c: char) -> (char, Option<Mark>) {
  match c {
    'À' => ('A', Some(GRAVE)),
    'Â' => ('A', Some(CIRCUMFLEX)),
    'Ä' => ('A', Some(DIAERESIS)),
    'Á' => ('A', Some(ACUTE)),
    'Ç' => ('C', Some(CEDILLA)),
    'É' => ('E', Some(ACUTE)),
    'È' => ('E', Some(GRAVE)),
    'Ê' => ('E', Some(CIRCUMFLEX)),
    'Ë' => ('E', Some(DIAERESIS)),
    'Í' => ('I', Some(ACUTE)),
    'Î' => ('I', Some(CIRCUMFLEX)),
    'Ï' => ('I', Some(DIAERESIS)),
    'Ó' => ('O', Some(ACUTE)),
    'Ô' => ('O', Some(CIRCUMFLEX)),
    'Ö' => ('O', Some(DIAERESIS)),
    'Ú' => ('U', Some(ACUTE)),
    'Ù' => ('U', Some(GRAVE)),
    'Û' => ('U', Some(CIRCUMFLEX)),
    'Ü' => ('U', Some(DIAERESIS)),
    'Ÿ' => ('Y', Some(DIAERESIS)),
    c => (c, None),
  }
}

/// `text` as a bitmap in font pixels, row-major, LINE_ROWS tall. Lowercase
/// is set in capitals; characters the font doesn't have come out as '?'.
pub(crate) fn text_bitmap(text: &str) -> (Vec<bool>, usize, usize) {
  let glyph = |c: char| FONT.iter().find(|(g, _)| *g == c).map(|(_, rows)| rows);
  // (column offset, glyph rows, mark) per drawn character
  let mut placed: Vec<(usize, &[&str; 7], Option<Mark>)> = Vec::new();
  let mut x = 0usize;
  for c in text.to_uppercase().chars() {
    if c.is_whitespace() {
      x += 4;
      continue;
    }
    let (base, mark) = decompose(c);
    let rows = glyph(base).or_else(|| glyph('?')).unwrap();
    placed.push((x, rows, mark));
    x += 6;
  }
  let width = x.saturating_sub(1).max(1);
  let mut bits = vec![false; width * LINE_ROWS];
  let mut ink = |col: usize, row: usize, s: &str| {
    for (i, ch) in s.chars().enumerate() {
      if ch == '#' && col + i < width {
        bits[row * width + col + i] = true;
      }
    }
  };
  for (col, rows, mark) in placed {
    for (r, line) in rows.iter().enumerate() {
      ink(col, CAP_TOP + r, line);
    }
    let (top, mark) = match mark {
      Some(Mark::Above(rows)) => (0, rows),
      Some(Mark::Below(rows)) => (CAP_TOP + 7, rows),
      None => {
        continue;
      }
    };
    for (r, line) in mark.iter().enumerate() {
      ink(col, top + r, line);
    }
  }
  (bits, width, LINE_ROWS)
}

/// Text to carve: centred on (lat, lng), capitals `letter_height_m` tall,
/// raised by `relief_m` (negative engraves).
#[derive(Clone, Debug)]
pub(crate) struct Carving {
  pub(crate) text: String,
  pub(crate) lat: f64,
  pub(crate) lng: f64,
  pub(crate) letter_height_m: f64,
  pub(crate) relief_m: f32,
}

impl Carving {
  /// Stamp the text onto a grid over (south, north, west, east). Land under
  /// the letters moves by `relief_m`; water under raised letters becomes
  /// land at the water's own elevation plus `relief_m`, so letters at sea
  /// come out as islands. Engraving leaves water alone. Every stamped pixel
  /// is marked in `carved`. Returns the highest elevation it left, or None
  /// when the text missed the grid entirely.
  pub(crate) fn apply(
    &self,
    terrain: &mut [f32],
    water: Option<&mut Vec<bool>>,
    carved: &mut [bool],
    width: usize,
    height: usize,
    (south, north, west, east): (f64, f64, f64, f64)
  ) -> Option<f32> {
    let (bits, bw, bh) = text_bitmap(&self.text);
    // metres per grid pixel, and per font pixel
    let lat_center = (south + north) / 2.0;
    let px_h = ((north - south) * 110_540.0) / ((height - 1) as f64);
    let px_w = ((east - west) * 111_320.0 * lat_center.to_radians().cos()) / ((width - 1) as f64);
    let font_px = self.letter_height_m / 7.0;
    // grid position of the anchor: the middle of the text, half-way down
    // the capitals
    let col0 = ((self.lng - west) / (east - west)) * ((width - 1) as f64);
    let row0 = ((north - self.lat) / (north - south)) * ((height - 1) as f64);
    let (ax, ay) = ((bw as f64) / 2.0, (CAP_TOP as f64) + 3.5);
    let half_w = (ax * font_px) / px_w + 1.0;
    let (up, down) = ((ay * font_px) / px_h + 1.0, (((bh as f64) - ay) * font_px) / px_h + 1.0);
    let rows = ((row0 - up).floor().max(0.0) as usize)..((row0 + down).ceil().min(height as f64) as usize);
    let cols = ((col0 - half_w).floor().max(0.0) as usize)..((col0 + half_w).ceil().min(width as f64) as usize);

    let mut water = water;
    let mut top: Option<f32> = None;
    for row in rows {
      let fy = ((((row as f64) - row0) * px_h) / font_px + ay).floor();
      if fy < 0.0 || fy >= (bh as f64) {
        continue;
      }
      for col in cols.clone() {
        let fx = ((((col as f64) - col0) * px_w) / font_px + ax).floor();
        if fx < 0.0 || fx >= (bw as f64) || !bits[(fy as usize) * bw + (fx as usize)] {
          continue;
        }
        let i = row * width + col;
        let wet = water.as_ref().is_some_and(|w| w[i]);
        if wet && self.relief_m <= 0.0 {
          continue; // nothing to engrave on a lake
        }
        if wet {
          water.as_mut().unwrap()[i] = false;
        }
        terrain[i] += self.relief_m;
        carved[i] = true;
        top = Some(top.map_or(terrain[i], |t| t.max(terrain[i])));
      }
    }
    top
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn render(text: &str) -> String {
    let (bits, w, h) = text_bitmap(text);
    (0..h)
      .map(|r| (0..w).map(|c| if bits[r * w + c] { '#' } else { '.' }).collect::<String>())
      .collect::<Vec<_>>()
      .join("\n")
  }

  #[test]
  fn every_glyph_is_five_by_seven() {
    for (c, rows) in FONT {
      for row in rows {
        assert_eq!(row.chars().count(), 5, "{c:?}");
        assert!(row.chars().all(|ch| ch == '#' || ch == '.'), "{c:?}");
      }
    }
  }

  #[test]
  fn montreal_gets_its_accent() {
    let art = render("Montréal");
    let rows: Vec<&str> = art.lines().collect();
    // the É is the 6th letter: an acute over it, nothing over the rest
    let e_col = 5 * 6;
    assert_eq!(&rows[0][e_col..e_col + 5], "...#.");
    assert!(rows[0][..e_col].chars().all(|c| c == '.'));
    assert_eq!(&rows[CAP_TOP][e_col..e_col + 5], "#####");
    // lowercase came out in capitals: M's top row
    assert_eq!(&rows[CAP_TOP][..5], "#...#");
  }

  #[test]
  fn unknown_characters_come_out_as_question_marks() {
    assert_eq!(render("🦫"), render("?"));
  }

  /// Raw grid over a 0.1°-square bbox at 45°N: about 11 km tall, 101 px.
  const BBOX: (f64, f64, f64, f64) = (45.0, 45.1, -73.1, -73.0);

  #[test]
  fn raised_letters_on_water_become_islands() {
    let (w, h) = (101usize, 101usize);
    let mut terrain = vec![5.0f32; w * h];
    let mut water = Some(vec![true; w * h]); // all lake
    let mut carved = vec![false; w * h];
    let carving = Carving {
      text: "HI".into(),
      lat: 45.05,
      lng: -73.05,
      letter_height_m: 2000.0,
      relief_m: 30.0,
    };
    let top = carving.apply(&mut terrain, water.as_mut(), &mut carved, w, h, BBOX).unwrap();
    assert_eq!(top, 35.0);
    let land: Vec<bool> = water
      .unwrap()
      .iter()
      .map(|&wet| !wet)
      .collect();
    assert_eq!(land, carved, "exactly the letters became land");
    // two letters, H and I: two islands
    let water_now: Vec<bool> = land
      .iter()
      .map(|&l| !l)
      .collect();
    let (_, n) = crate::landmass::label_landmasses(&water_now, w, h);
    assert_eq!(n, 2);
  }

  #[test]
  fn engraving_cuts_land_and_leaves_water_alone() {
    let (w, h) = (101usize, 101usize);
    let mut terrain = vec![100.0f32; w * h];
    let mut water: Vec<bool> = (0..w * h).map(|i| i % w < 50).collect(); // west half is lake
    let before = water.clone();
    let mut carved = vec![false; w * h];
    let carving = Carving {
      text: "WAS HERE".into(),
      lat: 45.05,
      lng: -73.05,
      letter_height_m: 1500.0,
      relief_m: -20.0,
    };
    carving.apply(&mut terrain, Some(&mut water), &mut carved, w, h, BBOX);
    assert_eq!(water, before);
    for i in 0..w * h {
      let want = if carved[i] { 80.0 } else { 100.0 };
      assert_eq!(terrain[i], want);
      assert!(!(carved[i] && before[i]), "engraved the lake");
    }
    assert!(carved.iter().any(|&c| c), "some land got engraved");
  }

  #[test]
  fn letters_read_left_to_right_and_upright() {
    // An "L": its foot runs east along the bottom (south) edge.
    let (w, h) = (101usize, 101usize);
    let mut terrain = vec![0.0f32; w * h];
    let mut carved = vec![false; w * h];
    let carving = Carving {
      text: "L".into(),
      lat: 45.05,
      lng: -73.05,
      letter_height_m: 7000.0,
      relief_m: 1.0,
    };
    carving.apply(&mut terrain, None, &mut carved, w, h, BBOX);
    let rows: Vec<usize> = (0..h).filter(|&r| (0..w).any(|c| carved[r * w + c])).collect();
    let (top_row, bottom_row) = (rows[0], *rows.last().unwrap());
    let span = |r: usize| (0..w).filter(|&c| carved[r * w + c]).count();
    assert!(span(bottom_row) > 3 * span(top_row), "the foot is at the bottom (south)");
    let first_col = |r: usize| (0..w).position(|c| carved[r * w + c]).unwrap();
    assert_eq!(first_col(top_row), first_col(bottom_row), "the stem is on the left (west)");
  }
}
