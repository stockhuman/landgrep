use crate::CachedElevation;
use crate::grid::rasterize_rings;
use crate::landmass::label_landmasses;
use crate::water::{ RingGroup, rasterize_ring_groups };

/// Extract one or more landmasses from the cache: build a footprint of the
/// selected components and crop to it, for footprint-clipped STL meshing.
#[allow(clippy::type_complexity)]
pub(crate) fn crop_to_landmasses(
  c: &CachedElevation,
  labels: &[u32],
  ids: &[u32]
) -> Result<(Vec<f32>, Vec<bool>, usize, usize, f64, f64, f64, f64), String> {
  let keep: Vec<bool> = labels
    .iter()
    .map(|l| ids.contains(l))
    .collect();
  crop_to_footprint(c, &keep)
}

/// Crop the cache to the pixel bounding box of `keep` (plus margin) and
/// return the elevation crop plus the per-vertex footprint restricted to the
/// crop window.
#[allow(clippy::type_complexity)]
pub(crate) fn crop_to_footprint(
  c: &CachedElevation,
  keep: &[bool]
) -> Result<(Vec<f32>, Vec<bool>, usize, usize, f64, f64, f64, f64), String> {
  let (mut r0, mut r1, mut c0, mut c1) = (usize::MAX, 0usize, usize::MAX, 0usize);
  for row in 0..c.height {
    for col in 0..c.width {
      if keep[row * c.width + col] {
        r0 = r0.min(row);
        r1 = r1.max(row);
        c0 = c0.min(col);
        c1 = c1.max(col);
      }
    }
  }
  if r0 == usize::MAX {
    return Err("Selection is empty".into());
  }
  const MARGIN: usize = 1;
  r0 = r0.saturating_sub(MARGIN);
  c0 = c0.saturating_sub(MARGIN);
  r1 = (r1 + MARGIN).min(c.height - 1);
  c1 = (c1 + MARGIN).min(c.width - 1);

  let (w, h) = (c1 - c0 + 1, r1 - r0 + 1);
  if w < 2 || h < 2 {
    return Err("Selection too small to export".into());
  }

  let mut data = Vec::with_capacity(w * h);
  let mut footprint = Vec::with_capacity(w * h);
  for row in r0..=r1 {
    for col in c0..=c1 {
      let i = row * c.width + col;
      data.push(c.data[i]);
      footprint.push(keep[i]);
    }
  }

  // pixel bbox → geographic bbox (row 0 = north)
  let lat_at = |row: usize| c.north - ((row as f64) / ((c.height as f64) - 1.0)) * (c.north - c.south);
  let lon_at = |col: usize| c.west + ((col as f64) / ((c.width as f64) - 1.0)) * (c.east - c.west);
  Ok((data, footprint, w, h, lat_at(r1), lat_at(r0), lon_at(c0), lon_at(c1)))
}

/// Ground area of one raster cell in m².
pub(crate) fn cell_area_m2(c: &CachedElevation) -> f64 {
  let lat_center = (c.south + c.north) / 2.0;
  (((c.north - c.south) * 110_540.0) / (c.height as f64)) *
    (((c.east - c.west) * 111_320.0 * ((lat_center * std::f64::consts::PI) / 180.0).cos()) /
      (c.width as f64))
}

/// Rasterize a drawn lasso ring onto the cached grid and clip it to land
/// when a water mask exists. The outline only needs to be rough: water
/// inside it is dropped, so a coastline never has to be traced precisely —
/// this is also the manual escape hatch for landmasses that label as one
/// component through causeways or bridges.
pub(crate) fn lasso_footprint(
  c: &CachedElevation,
  points: &[[f64; 2]]
) -> Result<Vec<bool>, String> {
  if points.len() < 3 {
    return Err("Lasso needs at least 3 points".into());
  }
  let mut mask = vec![false; c.width * c.height];
  rasterize_rings(
    &[points.to_vec()],
    c.south,
    c.north,
    c.west,
    c.east,
    c.width,
    c.height,
    &mut mask
  );
  if let Some(water) = &c.water_mask {
    for (m, &is_water) in mask.iter_mut().zip(water) {
      *m = *m && !is_water;
    }
  }
  if !mask.iter().any(|&m| m) {
    return Err("Lasso contains no land".into());
  }
  Ok(mask)
}

/// What a smoothed export is selecting, in vector-recoverable form.
pub(crate) enum SmoothSelection<'a> {
  /// The drawn lasso ring.
  Lasso(&'a [[f64; 2]]),
  /// Landmass selection, identified by the mesh-resolution footprint it
  /// produced (grid dims follow).
  Landmasses(&'a [bool], usize, usize),
}

/// Vector-fidelity footprint for smoothed exports: re-rasterize the original
/// water rings (and lasso ring) at the supersampled grid, so borders follow
/// the true OSM shorelines at export resolution instead of a blur-rounded
/// DEM-pixel staircase.
pub(crate) fn vector_footprint(
  rings: Option<&[RingGroup]>,
  sel: SmoothSelection,
  south: f64,
  north: f64,
  west: f64,
  east: f64,
  uw: usize,
  uh: usize
) -> Result<Vec<bool>, String> {
  let fp = match sel {
    SmoothSelection::Lasso(ring) => {
      let mut region = vec![false; uw * uh];
      rasterize_rings(&[ring.to_vec()], south, north, west, east, uw, uh, &mut region);
      if let Some(groups) = rings {
        let water = rasterize_ring_groups(groups, south, north, west, east, uw, uh);
        for (m, &is_water) in region.iter_mut().zip(&water) {
          *m = *m && !is_water;
        }
      }
      region
    }
    SmoothSelection::Landmasses(mesh_fp, mw, mh) => {
      let groups = rings.ok_or("No water rings cached — fetch again to smooth landmasses")?;
      let water = rasterize_ring_groups(groups, south, north, west, east, uw, uh);
      let (labels_hi, n_hi) = label_landmasses(&water, uw, uh);
      components_overlapping(&labels_hi, n_hi, uw, uh, mesh_fp, mw, mh)
    }
  };
  if !fp.iter().any(|&b| b) {
    return Err("Selection too small to smooth — reduce edge smoothing".into());
  }
  Ok(fp)
}

/// Keep the hi-res components that overlap the coarse selection footprint:
/// each hi-res land pixel maps back to its coarse-grid pixel, and a component
/// survives when any of its pixels lands on a selected one. Components can
/// only get *more* separated at higher resolution, so overlap is the right
/// join — a split component keeps all its parts.
fn components_overlapping(
  labels_hi: &[u32],
  n_hi: u32,
  uw: usize,
  uh: usize,
  coarse_fp: &[bool],
  cw: usize,
  ch: usize
) -> Vec<bool> {
  let mut selected = vec![false; (n_hi as usize) + 1];
  for r in 0..uh {
    let cr = ((((r as f64) / ((uh as f64) - 1.0)) * ((ch as f64) - 1.0)).round() as usize).min(
      ch - 1
    );
    for c in 0..uw {
      let id = labels_hi[r * uw + c];
      if id == 0 {
        continue;
      }
      let cc = ((((c as f64) / ((uw as f64) - 1.0)) * ((cw as f64) - 1.0)).round() as usize).min(
        cw - 1
      );
      if coarse_fp[cr * cw + cc] {
        selected[id as usize] = true;
      }
    }
  }
  labels_hi
    .iter()
    .map(|&l| l != 0 && selected[l as usize])
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::landmass::label_landmasses;
  use crate::mesh::build_stl;
  use crate::test_support::{ assert_watertight, two_block_cache };

  #[test]
  fn crop_single_landmass_footprint() {
    let (cache, labels) = two_block_cache();
    let id_a = labels[1 * 8 + 1];
    let (crop, footprint, cw, ch, s, n_, wst, e) = crop_to_landmasses(
      &cache,
      &labels,
      &[id_a]
    ).unwrap();
    // bbox rows/cols 1–2 with margin 1 → 0..=3 → 4×4
    assert_eq!((cw, ch), (4, 4));
    assert_eq!(
      footprint
        .iter()
        .filter(|&&f| f)
        .count(),
      4
    );
    // footprint pixels carry landmass A elevations, untouched
    for i in 0..crop.len() {
      if footprint[i] {
        assert_eq!(crop[i], 100.0);
      }
    }
    assert!(s < n_ && wst < e && s >= 45.0 && n_ <= 46.0 && wst >= -74.0 && e <= -73.0);
    // and the footprint meshes into a watertight solid
    let stl = build_stl(&crop, &footprint, cw, ch, 1.0, 1.0, s, n_, wst, e).unwrap();
    assert_watertight(&stl);
  }

  #[test]
  fn crop_multiple_landmasses_meshes_both() {
    let (cache, labels) = two_block_cache();
    let (id_a, id_b) = (labels[1 * 8 + 1], labels[5 * 8 + 5]);
    let (crop, footprint, cw, ch, s, n_, wst, e) = crop_to_landmasses(
      &cache,
      &labels,
      &[id_a, id_b]
    ).unwrap();
    assert_eq!((cw, ch), (8, 8)); // union bbox spans both blocks
    assert_eq!(
      footprint
        .iter()
        .filter(|&&f| f)
        .count(),
      8
    );
    let stl = build_stl(&crop, &footprint, cw, ch, 1.0, 1.0, s, n_, wst, e).unwrap();
    // two disjoint solids, each closed
    assert_watertight(&stl);
  }

  #[test]
  fn lasso_clips_to_land_and_meshes() {
    let (cache, _) = two_block_cache();
    // Top-left quadrant of the 45–46 / -74–-73 bbox: covers land block A
    // (rows/cols 1–2) plus surrounding water, which must be dropped.
    let ring = [[46.0, -74.0], [46.0, -73.5], [45.5, -73.5], [45.5, -74.0]];
    let keep = lasso_footprint(&cache, &ring).unwrap();
    assert_eq!(
      keep
        .iter()
        .filter(|&&k| k)
        .count(),
      4
    );
    let (crop, footprint, cw, ch, s, n_, wst, e) = crop_to_footprint(&cache, &keep).unwrap();
    let stl = build_stl(&crop, &footprint, cw, ch, 1.0, 1.0, s, n_, wst, e).unwrap();
    assert_watertight(&stl);
  }

  #[test]
  fn lasso_splits_a_single_landmass() {
    // One connected 2×6 land strip; the lasso takes only its left half —
    // the manual-split case connected-component labeling can't express.
    let (w, h) = (8usize, 8usize);
    let mut water = vec![true; w * h];
    let mut data = vec![0.0f32; w * h];
    for r in 1..=2 {
      for c in 1..=6 {
        water[r * w + c] = false;
        data[r * w + c] = 100.0;
      }
    }
    let (labels, n) = label_landmasses(&water, w, h);
    assert_eq!(n, 1);
    let cache = CachedElevation {
      data,
      width: w,
      height: h,
      min_elev: 0.0,
      max_elev: 100.0,
      south: 45.0,
      north: 46.0,
      west: -74.0,
      east: -73.0,
      water_mask: Some(water),
      water_rings: None,
      labels: Some(labels),
      n_landmasses: n,
      lasso: None,
      lasso_ring: None,
      edits: Default::default(),
    };
    // rows 0–2, cols 0–3 of the raster → strip pixels rows 1–2 × cols 1–3
    let ring = [[46.0, -74.0], [46.0, -73.5], [45.6, -73.5], [45.6, -74.0]];
    let keep = lasso_footprint(&cache, &ring).unwrap();
    assert_eq!(
      keep
        .iter()
        .filter(|&&k| k)
        .count(),
      6
    );
    for (i, &k) in keep.iter().enumerate() {
      if k {
        let (row, col) = (i / w, i % w);
        assert!((1..=2).contains(&row) && (1..=3).contains(&col));
      }
    }
  }

  #[test]
  fn lasso_without_water_mask_keeps_whole_polygon() {
    let (w, h) = (8usize, 8usize);
    let cache = CachedElevation {
      data: vec![50.0; w * h],
      width: w,
      height: h,
      min_elev: 50.0,
      max_elev: 50.0,
      south: 45.0,
      north: 46.0,
      west: -74.0,
      east: -73.0,
      water_mask: None,
      water_rings: None,
      labels: None,
      n_landmasses: 0,
      lasso: None,
      lasso_ring: None,
      edits: Default::default(),
    };
    let ring = [[46.0, -74.0], [46.0, -73.5], [45.6, -73.5], [45.6, -74.0]];
    let keep = lasso_footprint(&cache, &ring).unwrap();
    // rows 0–2 × cols 0–3, nothing clipped away
    assert_eq!(
      keep
        .iter()
        .filter(|&&k| k)
        .count(),
      12
    );
  }

  #[test]
  fn degenerate_lassos_are_errors() {
    let (cache, _) = two_block_cache();
    assert!(lasso_footprint(&cache, &[[45.5, -73.5], [45.6, -73.6]]).is_err());
    // water-only region
    let ring = [[46.0, -74.0], [46.0, -73.9], [45.95, -73.9], [45.95, -74.0]];
    assert!(lasso_footprint(&cache, &ring).is_err());
  }

  /// One vertical water strip (lon 0.4–0.6, full height with margin so the
  /// scanline fill covers every row) splitting the unit bbox into a left and
  /// a right landmass.
  fn strip_rings() -> Vec<RingGroup> {
    vec![vec![vec![[1.5, 0.4], [1.5, 0.6], [-0.5, 0.6], [-0.5, 0.4]]]]
  }

  #[test]
  fn vector_footprint_landmasses_picks_overlapping_component() {
    let rings = strip_rings();
    // coarse 5×5 selection footprint: right side (cols 3–4) selected
    let (cw, ch) = (5usize, 5usize);
    let coarse: Vec<bool> = (0..cw * ch).map(|i| i % cw >= 3).collect();
    // hi-res 9×9 (factor 2): water fills cols 3–4, land splits at col 5
    let fp = vector_footprint(
      Some(&rings),
      SmoothSelection::Landmasses(&coarse, cw, ch),
      0.0,
      1.0,
      0.0,
      1.0,
      9,
      9
    ).unwrap();
    for r in 0..9 {
      for c in 0..9 {
        let expect = c >= 5; // right landmass only; left one not selected
        assert_eq!(fp[r * 9 + c], expect, "pixel ({r},{c})");
      }
    }
  }

  #[test]
  fn vector_footprint_lasso_carves_water_at_hires() {
    let rings = strip_rings();
    // lasso over the right half — overlaps the water strip, which must be cut
    let ring = [[1.5, 0.55], [1.5, 1.5], [-0.5, 1.5], [-0.5, 0.55]];
    let fp = vector_footprint(
      Some(&rings),
      SmoothSelection::Lasso(&ring),
      0.0,
      1.0,
      0.0,
      1.0,
      9,
      9
    ).unwrap();
    for r in 0..9 {
      for c in 0..9 {
        let expect = c >= 5; // lasso reaches col 4, but cols 3–4 are water
        assert_eq!(fp[r * 9 + c], expect, "pixel ({r},{c})");
      }
    }
  }

  #[test]
  fn vector_footprint_all_water_is_an_error() {
    let rings = strip_rings();
    // lasso entirely inside the water strip
    let ring = [[1.5, 0.45], [1.5, 0.55], [-0.5, 0.55], [-0.5, 0.45]];
    assert!(
      vector_footprint(
        Some(&rings),
        SmoothSelection::Lasso(&ring),
        0.0,
        1.0,
        0.0,
        1.0,
        9,
        9
      ).is_err()
    );
  }
}
