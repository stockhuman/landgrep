//! Sea-level rise. Water comes in from the map edge (the sea beyond the bbox
//! is assumed to connect) and through every water body already on the map,
//! and fills the terrain up to the new level wherever it can reach: a
//! bathtub with plumbing, so a basin behind a ridge stays dry until the
//! water tops the ridge.

use std::cmp::{ Ordering, Reverse };
use std::collections::BinaryHeap;

/// f32 with a total order, for the heap.
#[derive(PartialEq)]
struct Level(f32);

impl Eq for Level {}

impl PartialOrd for Level {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Level {
  fn cmp(&self, other: &Self) -> Ordering {
    self.0.total_cmp(&other.0)
  }
}

/// Priority-Flood (Barnes, Lehman & Mulla 2014): for every pixel, the lowest
/// sea level that reaches it, i.e. the minimax elevation over all paths in
/// from the map edge or from existing water. Existing water reports −∞
/// (always wet). Computed once; every sea level after that is a threshold,
/// see `flooded_mask`. 4-connected, like `label_landmasses`.
pub(crate) fn flood_levels(elev: &[f32], water: &[bool], width: usize, height: usize) -> Vec<f32> {
  let n = width * height;
  let mut level = vec![f32::INFINITY; n];
  let mut heap: BinaryHeap<Reverse<(Level, usize)>> = BinaryHeap::new();

  let neighbours = |i: usize| {
    let (row, col) = (i / width, i % width);
    [
      (col > 0).then(|| i - 1),
      (col + 1 < width).then(|| i + 1),
      (row > 0).then(|| i - width),
      (row + 1 < height).then(|| i + width),
    ]
      .into_iter()
      .flatten()
  };

  // Existing water is wet at any level. Only its shore pixels can spread
  // anywhere, so only those need to go on the heap.
  for i in 0..n {
    if water[i] {
      level[i] = f32::NEG_INFINITY;
      if neighbours(i).any(|j| !water[j]) {
        heap.push(Reverse((Level(f32::NEG_INFINITY), i)));
      }
    }
  }
  // The edge of the map is open sea: a border pixel floods as soon as the
  // water is above it.
  for i in 0..n {
    let (row, col) = (i / width, i % width);
    let border = row == 0 || col == 0 || row + 1 == height || col + 1 == width;
    if border && !water[i] {
      level[i] = elev[i];
      heap.push(Reverse((Level(elev[i]), i)));
    }
  }

  while let Some(Reverse((Level(l), i))) = heap.pop() {
    if l > level[i] {
      continue; // stale: already reached lower
    }
    for j in neighbours(i) {
      let lj = l.max(elev[j]);
      if lj < level[j] {
        level[j] = lj;
        heap.push(Reverse((Level(lj), j)));
      }
    }
  }
  level
}

/// The water mask at sea level `sea_level`: what was already water, plus
/// everything the rise reaches.
pub(crate) fn flooded_mask(water: &[bool], levels: &[f32], sea_level: f32) -> Vec<bool> {
  water
    .iter()
    .zip(levels)
    .map(|(&w, &l)| w || l <= sea_level)
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Wet set at `sea_level` the slow way: flood-fill from every seed
  /// through pixels at or below the level.
  fn brute_force(elev: &[f32], water: &[bool], w: usize, h: usize, sea_level: f32) -> Vec<bool> {
    let mut wet = water.to_vec();
    let mut stack: Vec<usize> = (0..w * h)
      .filter(|&i| {
        let (r, c) = (i / w, i % w);
        water[i] || ((r == 0 || c == 0 || r + 1 == h || c + 1 == w) && elev[i] <= sea_level)
      })
      .collect();
    for &i in &stack {
      wet[i] = true;
    }
    while let Some(i) = stack.pop() {
      let (r, c) = (i / w, i % w);
      let mut visit = |j: usize| {
        if !wet[j] && elev[j] <= sea_level {
          wet[j] = true;
          stack.push(j);
        }
      };
      if c > 0 {
        visit(i - 1);
      }
      if c + 1 < w {
        visit(i + 1);
      }
      if r > 0 {
        visit(i - w);
      }
      if r + 1 < h {
        visit(i + w);
      }
    }
    wet
  }

  #[test]
  fn basin_behind_a_ridge_stays_dry_until_the_water_tops_it() {
    // West to east: coast at 0 m, a 50 m ridge, a basin at 10 m, high
    // ground at 80 m. The basin is lower than the ridge but can only be
    // reached over it.
    let row = [0.0f32, 50.0, 10.0, 10.0, 80.0];
    let (w, h) = (row.len(), 3usize);
    let elev: Vec<f32> = (0..w * h).map(|i| if i / w == 1 { row[i % w] } else { 100.0 }).collect();
    let water = vec![false; w * h];
    let levels = flood_levels(&elev, &water, w, h);
    let mid = |c: usize| levels[w + c];
    assert_eq!(mid(0), 0.0, "the coast floods first");
    assert_eq!(mid(2), 50.0, "the basin floods when the ridge does");
    assert_eq!(mid(3), 50.0);
    assert!(!flooded_mask(&water, &levels, 30.0)[w + 2], "dry at +30 m");
    assert!(flooded_mask(&water, &levels, 50.0)[w + 2], "wet at +50 m");
  }

  #[test]
  fn rivers_carry_the_flood_inland() {
    // 100 m country with a river channel cut in from the west edge, and
    // 20 m banks along it: the banks flood at 20 m via the river, long
    // before the sea could come overland.
    let (w, h) = (7usize, 5usize);
    let mut elev = vec![100.0f32; w * h];
    let mut water = vec![false; w * h];
    for c in 0..5 {
      water[2 * w + c] = true;
      elev[w + c] = 20.0;
      elev[3 * w + c] = 20.0;
    }
    let levels = flood_levels(&elev, &water, w, h);
    assert_eq!(levels[2 * w + 3], f32::NEG_INFINITY, "the river is always wet");
    assert_eq!(levels[w + 4], 20.0);
    assert_eq!(levels[3 * w + 4], 20.0);
    assert_eq!(levels[w + 6], 100.0, "the far bank needs the whole country to drown");
  }

  #[test]
  fn flood_levels_match_brute_force_flood_fills() {
    // Random terrain with a few lakes: thresholding the levels must agree
    // with a direct flood-fill at every elevation in the map.
    let mut seed = 0x9e37_79b9u32;
    let mut rand = || {
      seed ^= seed << 13;
      seed ^= seed >> 17;
      seed ^= seed << 5;
      seed
    };
    for _ in 0..20 {
      let (w, h) = (9usize, 7usize);
      let elev: Vec<f32> = (0..w * h).map(|_| (rand() % 40) as f32).collect();
      let water: Vec<bool> = (0..w * h).map(|_| rand() % 9 == 0).collect();
      let levels = flood_levels(&elev, &water, w, h);
      let mut thresholds = elev.clone();
      thresholds.push(-1.0);
      for &t in &thresholds {
        assert_eq!(flooded_mask(&water, &levels, t), brute_force(&elev, &water, w, h, t), "sea level {t}");
      }
    }
  }
}
