fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
  let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
  if len > 1e-10 {
    [v[0] / len, v[1] / len, v[2] / len]
  } else {
    [0.0, 0.0, 1.0]
  }
}

fn write_tri(buf: &mut Vec<u8>, v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) {
  let n = normalize(cross(sub(v1, v0), sub(v2, v0)));
  for f in n.iter().chain(v0.iter()).chain(v1.iter()).chain(v2.iter()) {
    buf.extend_from_slice(&f.to_le_bytes());
  }
  buf.extend_from_slice(&(0u16).to_le_bytes());
}

/// Build a watertight solid clipped to a per-vertex footprint: heightfield
/// top over footprint cells, vertical walls along the footprint boundary,
/// and a mirrored flat bottom at z=0. A grid cell is meshed when all four of
/// its corner vertices are inside the footprint, so a landmass export
/// contains that shape alone — not its bounding rectangle. Relief is
/// measured above the footprint's minimum elevation at true scale (the same
/// mm per metre as the horizontal) times `vertical_scale`, and sits on a
/// `base_mm`-thick slab. Model axes are +x east, +y north, +z up.
pub(crate) fn build_stl(
  data: &[f32],
  footprint: &[bool],
  width: usize,
  height: usize,
  vertical_scale: f32,
  base_mm: f32,
  south: f64,
  north: f64,
  west: f64,
  east: f64
) -> Result<Vec<u8>, String> {
  let lat_center = (south + north) / 2.0;
  let lat_m = (north - south) * 110_540.0;
  let lon_m = (east - west) * 111_320.0 * ((lat_center * std::f64::consts::PI) / 180.0).cos();
  let max_m = lat_m.max(lon_m);
  let mm_per_m = 200.0 / max_m;

  let cell_x = ((lon_m * mm_per_m) / ((width - 1) as f64)) as f32;
  let cell_y = ((lat_m * mm_per_m) / ((height - 1) as f64)) as f32;

  let mut min_elev = f32::INFINITY;
  for (i, &inside) in footprint.iter().enumerate() {
    if inside {
      min_elev = min_elev.min(data[i]);
    }
  }
  if !min_elev.is_finite() {
    return Err("Selection contains no terrain".into());
  }
  // Same mm per metre as the horizontal, then exaggerated: what the 3D
  // preview draws. (This used to divide by the selection's elevation range
  // and multiply by 200, which normalized every region's relief to a 200 m
  // hill: right for Montreal, 16× too flat for Etna.)
  let elev_scale = (mm_per_m as f32) * vertical_scale;
  let base = base_mm.max(0.4); // degenerate walls below ~one layer height

  // Raster rows run north → south, but +y is north in the model, so rows
  // count down from the top. Counting them up instead (y = row) reflects the
  // terrain: east stays on the right and north lands at the front, which no
  // rotation in a slicer can undo.
  let y_of = |row: usize| ((height - 1 - row) as f32) * cell_y;
  let v = |col: usize, row: usize| -> [f32; 3] {
    [
      (col as f32) * cell_x,
      y_of(row),
      (data[row * width + col] - min_elev).max(0.0) * elev_scale + base,
    ]
  };
  let vb = |col: usize, row: usize| -> [f32; 3] { [(col as f32) * cell_x, y_of(row), 0.0] };
  // cell (col,row) spans vertices (col..col+1, row..row+1)
  let (cw, ch) = (width - 1, height - 1);
  let mut cells = vec![false; cw * ch];
  for r in 0..ch {
    for c in 0..cw {
      cells[r * cw + c] =
        footprint[r * width + c] &&
        footprint[r * width + c + 1] &&
        footprint[(r + 1) * width + c] &&
        footprint[(r + 1) * width + c + 1];
    }
  }
  // Cells that touch only at a corner would make four wall quads share one
  // vertical edge — non-manifold. Fill one of the two empty diagonal
  // neighbours until no corner-only contacts remain (additions can create
  // new contacts, hence the fixed-point loop).
  loop {
    let mut changed = false;
    for r in 0..ch - 1 {
      for c in 0..cw - 1 {
        let (a, b) = (cells[r * cw + c], cells[r * cw + c + 1]);
        let (d, e) = (cells[(r + 1) * cw + c], cells[(r + 1) * cw + c + 1]);
        if a && e && !b && !d {
          cells[r * cw + c + 1] = true;
          changed = true;
        } else if b && d && !a && !e {
          cells[r * cw + c] = true;
          changed = true;
        }
      }
    }
    if !changed {
      break;
    }
  }
  let cell = |col: isize, row: isize| -> bool {
    if col < 0 || row < 0 || (col as usize) >= cw || (row as usize) >= ch {
      return false;
    }
    cells[(row as usize) * cw + (col as usize)]
  };

  let mut buf = vec![0u8; 84]; // header + count placeholder, patched below

  for row in 0..height - 1 {
    for col in 0..width - 1 {
      if !cell(col as isize, row as isize) {
        continue;
      }
      // cell corners by compass point (row = north edge, row + 1 = south)
      let (nw, ne) = (v(col, row), v(col + 1, row));
      let (sw, se) = (v(col, row + 1), v(col + 1, row + 1));
      let (bnw, bne) = (vb(col, row), vb(col + 1, row));
      let (bsw, bse) = (vb(col, row + 1), vb(col + 1, row + 1));

      // top (+z) and mirrored bottom (-z)
      write_tri(&mut buf, sw, se, ne);
      write_tri(&mut buf, sw, ne, nw);
      write_tri(&mut buf, bsw, bne, bse);
      write_tri(&mut buf, bsw, bnw, bne);

      // walls wherever the neighbouring cell is outside the footprint
      let (ci, ri) = (col as isize, row as isize);
      if !cell(ci, ri - 1) {
        // north side (+y)
        write_tri(&mut buf, bne, bnw, ne);
        write_tri(&mut buf, bnw, nw, ne);
      }
      if !cell(ci, ri + 1) {
        // south side (-y)
        write_tri(&mut buf, bsw, bse, sw);
        write_tri(&mut buf, bse, se, sw);
      }
      if !cell(ci - 1, ri) {
        // west side (-x)
        write_tri(&mut buf, bnw, bsw, nw);
        write_tri(&mut buf, bsw, sw, nw);
      }
      if !cell(ci + 1, ri) {
        // east side (+x)
        write_tri(&mut buf, bse, bne, se);
        write_tri(&mut buf, bne, ne, se);
      }
    }
  }

  let n_tris = (buf.len() - 84) / 50;
  if n_tris == 0 {
    return Err("Selection too small to mesh at this resolution".into());
  }
  buf[80..84].copy_from_slice(&(n_tris as u32).to_le_bytes());
  Ok(buf)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::landmass::label_landmasses;
  use crate::test_support::{ assert_watertight, montreal_fixture };
  use crate::water::mask_from_overpass;

  #[test]
  fn full_rect_stl_is_watertight() {
    let data = vec![10.0, 12.0, 11.0, 10.5, 13.0, 12.5, 10.0, 11.0, 14.0];
    let (w, h) = (3usize, 3usize);
    let footprint = vec![true; w * h];
    let stl = build_stl(&data, &footprint, w, h, 1.5, 2.0, 45.0, 45.1, -73.1, -73.0).unwrap();
    let n_tris = 4 * (w - 1) * (h - 1) + 4 * (w - 1) + 4 * (h - 1);
    assert_eq!(u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize, n_tris);
    assert_watertight(&stl);
  }

  #[test]
  fn footprint_stl_is_clipped_and_watertight() {
    // plus-shaped footprint on a 7×7 grid
    let (w, h) = (7usize, 7usize);
    let mut footprint = vec![false; w * h];
    for r in 0..h {
      for c in 0..w {
        if (2..=4).contains(&r) || (2..=4).contains(&c) {
          footprint[r * w + c] = true;
        }
      }
    }
    let data: Vec<f32> = (0..w * h).map(|i| 100.0 + ((i % 7) as f32)).collect();
    let stl = build_stl(&data, &footprint, w, h, 1.0, 1.0, 45.0, 45.1, -73.1, -73.0).unwrap();
    assert_watertight(&stl);
    // clipped: fewer top cells than the full 6×6 grid would produce
    let n = u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize;
    assert!(n < 4 * (w - 1) * (h - 1) + 4 * (w - 1) + 4 * (h - 1));
  }

  /// Every triangle corner, in file order.
  fn vertices(stl: &[u8]) -> Vec<[f32; 3]> {
    let n = u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize;
    let f = |o: usize| f32::from_le_bytes(stl[o..o + 4].try_into().unwrap());
    (0..n * 3)
      .map(|k| {
        let off = 84 + (k / 3) * 50 + 12 + (k % 3) * 12;
        [f(off), f(off + 4), f(off + 8)]
      })
      .collect()
  }

  #[test]
  fn exports_are_not_from_the_mirror_universe() {
    // A lone peak in the raster's northeast corner (row 0 = north, last
    // column = east) has to print at the model's +x/+y corner. It used to
    // print at +x/−y: the terrain's mirror image, Laval on the South Shore.
    let (w, h) = (5usize, 5usize);
    let mut data = vec![0.0f32; w * h];
    data[w - 1] = 100.0;
    let stl = build_stl(&data, &vec![true; w * h], w, h, 1.0, 2.0, 45.0, 45.1, -73.1, -73.0).unwrap();
    assert_watertight(&stl);
    let vs = vertices(&stl);
    let peak = vs
      .iter()
      .copied()
      .max_by(|a, b| a[2].total_cmp(&b[2]))
      .unwrap();
    let max_x = vs.iter().map(|v| v[0]).fold(f32::MIN, f32::max);
    let max_y = vs.iter().map(|v| v[1]).fold(f32::MIN, f32::max);
    assert_eq!((peak[0], peak[1]), (max_x, max_y), "the NE peak should sit at +x/+y");
  }

  #[test]
  fn relief_is_true_scale_times_exaggeration() {
    // A 20 m bump and a 1000 m peak must print 50× apart, at the footprint's
    // own mm per metre. The old formula printed both as tall as a 200 m hill.
    let (w, h) = (5usize, 5usize);
    let (s, n, wst, e) = (45.0, 45.1, -73.1, -73.0);
    let lat_m = (n - s) * 110_540.0;
    let lon_m = (e - wst) * 111_320.0 * ((s + n) / 2.0f64).to_radians().cos();
    let mm_per_m = 200.0 / lat_m.max(lon_m);
    for (relief_m, scale) in [(20.0f32, 1.0f32), (1000.0, 1.0), (1000.0, 2.5)] {
      let mut data = vec![0.0f32; w * h];
      data[2 * w + 2] = relief_m;
      let stl = build_stl(&data, &vec![true; w * h], w, h, scale, 2.0, s, n, wst, e).unwrap();
      let top = vertices(&stl)
        .iter()
        .map(|v| v[2])
        .fold(f32::MIN, f32::max);
      let want = (relief_m as f64) * mm_per_m * (scale as f64) + 2.0;
      assert!(((top as f64) - want).abs() < 1e-3, "{relief_m} m ×{scale}: got {top} mm, want {want:.3}");
    }
  }

  #[test]
  fn empty_footprint_is_an_error() {
    let data = vec![1.0f32; 9];
    assert!(build_stl(&data, &[false; 9], 3, 3, 1.0, 1.0, 45.0, 45.1, -73.1, -73.0).is_err());
  }

  #[test]
  fn montreal_island_stl_is_watertight() {
    let body = montreal_fixture();
    let mask = mask_from_overpass(&body, 45.30, 45.75, -74.25, -73.3, 760, 500).unwrap();
    let (labels, _) = label_landmasses(&mask, 760, 500);
    let island = labels[((0.25 / 0.45) * 499.0) as usize * 760 + ((0.63 / 0.95) * 759.0) as usize];
    assert_ne!(island, 0);
    let footprint: Vec<bool> = labels
      .iter()
      .map(|&l| l == island)
      .collect();
    let data: Vec<f32> = (0..760 * 500).map(|i| 10.0 + (((i % 760) as f32) * 0.01)).collect();
    let stl = build_stl(&data, &footprint, 760, 500, 1.0, 2.0, 45.3, 45.75, -74.25, -73.3).unwrap();
    assert_watertight(&stl);
  }
}
