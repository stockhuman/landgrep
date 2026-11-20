//! What landgrep has done to the planet since the fetch: the sea level, and
//! the text carved into it. `data`, `water_mask` and `labels` are always
//! rebuilt from the fetched terrain plus these, so putting the sea back or
//! undoing a carving restores the fetch exactly.

use crate::CachedElevation;
use crate::flood::{ flood_levels, flooded_mask };
use crate::glyphs::Carving;
use crate::landmass::label_landmasses;
use crate::water::{ RingGroup, flatten_water };

#[derive(Default)]
pub(crate) struct Edits {
  /// Metres above today's sea level; None = the coastline as fetched.
  pub(crate) sea_level: Option<f32>,
  /// Text carved into the terrain, oldest first.
  pub(crate) carvings: Vec<Carving>,
  /// Terrain and water as fetched. Nothing is copied until the first edit.
  fetched: Option<(Vec<f32>, Option<Vec<bool>>)>,
  /// `flood_levels` for the fetched terrain with the carvings in, computed
  /// on first use and dropped when the carvings change.
  levels: Option<Vec<f32>>,
  /// For the map overlays: land the sea has drowned, pixels the carvings
  /// touched.
  drowned: Option<Vec<bool>>,
  carved: Option<Vec<bool>>,
  /// Highest point the latest carving left.
  pub(crate) carved_top: Option<f32>,
  /// The carvings turned water into land.
  water_carved: bool,
}

impl CachedElevation {
  /// Move the sea to `level` metres above today's (None, or ≤ 0, = as
  /// fetched). Returns the level the water was flattened to.
  pub(crate) fn set_sea_level(&mut self, level: Option<f32>) -> Option<f32> {
    self.edits.sea_level = level.filter(|l| *l > 0.0);
    self.rebuild()
  }

  pub(crate) fn carve(&mut self, carving: Carving) -> Option<f32> {
    self.edits.carvings.push(carving);
    self.edits.levels = None;
    self.rebuild()
  }

  /// Take back the latest carving.
  pub(crate) fn uncarve(&mut self) -> Option<f32> {
    self.edits.carvings.pop();
    self.edits.levels = None;
    self.rebuild()
  }

  /// Rebuild everything that hangs off the terrain and the coastline: the
  /// water mask, the flattened terrain and the landmass labels. Selections
  /// drawn against the old coastline are dropped. Returns the level the
  /// water was flattened to.
  fn rebuild(&mut self) -> Option<f32> {
    let (w, h) = (self.width, self.height);
    let bbox = (self.south, self.north, self.west, self.east);
    let e = &mut self.edits;
    let (fetched_terrain, fetched_water) = &*e.fetched.get_or_insert_with(|| (
      self.data.clone(),
      self.water_mask.clone(),
    ));

    let mut terrain = fetched_terrain.clone();
    let mut water = fetched_water.clone();
    let mut carved = vec![false; w * h];
    e.carved_top = None;
    for carving in &e.carvings {
      e.carved_top = carving.apply(&mut terrain, water.as_mut(), &mut carved, w, h, bbox);
    }
    e.water_carved = water != *fetched_water;
    e.carved = (!e.carvings.is_empty()).then_some(carved);

    let mask = match e.sea_level {
      None => water.clone(),
      Some(level) => {
        let dry;
        let before = match &water {
          Some(m) => m.as_slice(),
          None => {
            dry = vec![false; w * h];
            &dry
          }
        };
        let levels = e.levels.get_or_insert_with(|| flood_levels(&terrain, before, w, h));
        Some(flooded_mask(before, levels, level))
      }
    };
    e.drowned = e.sea_level.and(mask.as_ref()).map(|now| match &water {
      Some(before) => now
        .iter()
        .zip(before)
        .map(|(&n, &b)| n && !b)
        .collect(),
      None => now.clone(),
    });

    let mut data = terrain;
    let water_level = mask.as_ref().and_then(|m| flatten_water(&mut data, m, w));
    let (labels, n) = match &mask {
      Some(m) => {
        let (l, n) = label_landmasses(m, w, h);
        (Some(l), n)
      }
      None => (None, 0),
    };
    self.min_elev = data.iter().copied().fold(f32::INFINITY, f32::min);
    self.max_elev = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    self.data = data;
    self.water_mask = mask;
    self.labels = labels;
    self.n_landmasses = n;
    self.lasso = None;
    self.lasso_ring = None;
    water_level
  }

  /// The fetched OSM rings, while they still describe the water. A raised
  /// sea, or letters carved into a lake, make coastlines OSM never drew.
  pub(crate) fn current_water_rings(&self) -> Option<&Vec<RingGroup>> {
    if self.edits.sea_level.is_some() || self.edits.water_carved {
      None
    } else {
      self.water_rings.as_ref()
    }
  }

  /// Land the raised sea has drowned, which the map still shows as streets.
  pub(crate) fn drowned(&self) -> Option<&[bool]> {
    self.edits.drowned.as_deref()
  }

  /// Pixels the carvings touched.
  pub(crate) fn carved(&self) -> Option<&[bool]> {
    self.edits.carved.as_deref()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// West to east, three rows alike: sea (OSM water) at 0 m, a 5 m beach, a
  /// 40 m hill, a 10 m valley, a 60 m hill, a 30 m plateau. Built the way
  /// fetch_elevation builds it, water flattened and land labelled.
  fn two_hills(osm_water: bool) -> CachedElevation {
    let cols = [0.0f32, 5.0, 40.0, 40.0, 10.0, 60.0, 60.0, 30.0, 30.0];
    let (w, h) = (cols.len(), 3usize);
    let mut data: Vec<f32> = (0..w * h).map(|i| cols[i % w]).collect();
    let mask: Option<Vec<bool>> = osm_water.then(|| (0..w * h).map(|i| i % w == 0).collect());
    if let Some(m) = &mask {
      flatten_water(&mut data, m, w);
    }
    let (labels, n) = match &mask {
      Some(m) => {
        let (l, n) = label_landmasses(m, w, h);
        (Some(l), n)
      }
      None => (None, 0),
    };
    CachedElevation {
      data,
      width: w,
      height: h,
      min_elev: 0.0,
      max_elev: 60.0,
      south: 45.0,
      north: 45.01,
      west: -73.0,
      east: -72.97,
      water_mask: mask,
      water_rings: Some(vec![]),
      labels,
      n_landmasses: n,
      lasso: None,
      lasso_ring: None,
      edits: Default::default(),
    }
  }

  #[test]
  fn a_rising_sea_splits_the_land_and_going_back_restores_it() {
    let mut c = two_hills(true);
    let fetched = c.data.clone();
    assert_eq!(c.n_landmasses, 1);
    c.lasso = Some(vec![true; c.width * c.height]);

    c.set_sea_level(Some(20.0));
    assert_eq!(c.n_landmasses, 2, "the valley floods: two islands");
    assert!(c.lasso.is_none(), "a selection doesn't survive a new coastline");
    assert!(c.current_water_rings().is_none(), "OSM never drew this coastline");

    c.set_sea_level(Some(50.0));
    assert_eq!(c.n_landmasses, 1, "only the 60 m hill is left");

    c.set_sea_level(Some(61.0));
    assert_eq!(c.n_landmasses, 0, "nothing is left");

    c.set_sea_level(None);
    assert_eq!(c.data, fetched, "back to the fetch, exactly");
    assert_eq!(c.n_landmasses, 1);
    assert!(c.current_water_rings().is_some());
  }

  #[test]
  fn montreal_after_a_30_m_rise_still_exports_watertight_islands() {
    // The real Montreal water (fixture) over a west-to-east ramp from 10 m
    // to 48 m: at +30 m the western half drowns and the Island of Montreal
    // loses its west end. What's left has to select and mesh like any other
    // landmass.
    let (w, h) = (760usize, 500usize);
    let geo = crate::water::geometry_from_overpass(&crate::test_support::montreal_fixture()).unwrap();
    let ramp: Vec<f32> = (0..w * h).map(|i| 10.0 + ((i % w) as f32) * 0.05).collect();
    let bbox = (45.3, 45.75, -74.25, -73.3);
    let (mut c, _, _) = crate::commands::build_terrain(ramp, w, h, bbox, Some(Ok(geo)));
    let at = |c: &CachedElevation, lat: f64, lon: f64| {
      let col = (((lon - bbox.2) / (bbox.3 - bbox.2)) * ((w - 1) as f64)) as usize;
      let row = (((bbox.1 - lat) / (bbox.1 - bbox.0)) * ((h - 1) as f64)) as usize;
      c.labels.as_ref().unwrap()[row * w + col]
    };
    assert_ne!(at(&c, 45.47, -73.85), 0, "the West Island, as fetched");

    c.set_sea_level(Some(30.0));
    assert_eq!(at(&c, 45.47, -73.85), 0, "the West Island is under");
    let downtown = at(&c, 45.5, -73.57);
    assert_ne!(downtown, 0, "downtown, at 37 m, isn't");
    assert_eq!(at(&c, 45.5, -73.62), downtown, "and it's still one island");
    let labels = c.labels.clone().unwrap();
    let (crop, fp, cw, ch, s, n, wst, e) = crate::selection::crop_to_landmasses(&c, &labels, &[downtown]).unwrap();
    let stl = crate::mesh::build_stl(&crop, &fp, cw, ch, 1.5, 2.0, s, n, wst, e).unwrap();
    crate::test_support::assert_watertight(&stl);
  }

  #[test]
  fn without_osm_water_the_sea_comes_in_from_the_edges() {
    // Water mask off (or Overpass down): a raised sea still makes
    // coastlines, and landmasses to click.
    let mut c = two_hills(false);
    assert!(c.labels.is_none());
    c.set_sea_level(Some(20.0));
    assert_eq!(c.n_landmasses, 2);
    c.set_sea_level(Some(0.0));
    assert!(c.labels.is_none() && c.water_mask.is_none(), "sea level 0 is the fetch");
  }

  /// A 101 px lake over a 0.1° bbox at 45°N (about 8 × 11 km), 0 m deep.
  fn lake() -> CachedElevation {
    let (w, h) = (101usize, 101usize);
    CachedElevation {
      data: vec![0.0; w * h],
      width: w,
      height: h,
      min_elev: 0.0,
      max_elev: 0.0,
      south: 45.0,
      north: 45.1,
      west: -73.1,
      east: -73.0,
      water_mask: Some(vec![true; w * h]),
      water_rings: Some(vec![]),
      labels: Some(vec![0; w * h]),
      n_landmasses: 0,
      lasso: None,
      lasso_ring: None,
      edits: Default::default(),
    }
  }

  fn hi(relief_m: f32) -> Carving {
    Carving { text: "HI".into(), lat: 45.05, lng: -73.05, letter_height_m: 2000.0, relief_m }
  }

  #[test]
  fn a_name_carved_at_sea_outlives_the_flood() {
    let mut c = lake();
    c.carve(hi(100.0));
    assert_eq!(c.n_landmasses, 2, "H and I, two islands");
    assert_eq!(c.edits.carved_top, Some(100.0));
    assert!(c.current_water_rings().is_none(), "OSM never drew these");

    c.set_sea_level(Some(60.0));
    assert_eq!(c.n_landmasses, 2, "still there at +60 m");
    c.set_sea_level(Some(101.0));
    assert_eq!(c.n_landmasses, 0);
  }

  #[test]
  fn undoing_a_carving_restores_the_fetch() {
    let mut c = two_hills(true);
    let fetched = c.data.clone();
    c.carve(Carving { text: "X".into(), lat: 45.005, lng: -72.985, letter_height_m: 700.0, relief_m: 25.0 });
    assert_ne!(c.data, fetched);
    assert!(c.carved().is_some_and(|m| m.iter().any(|&p| p)));
    c.set_sea_level(Some(20.0));
    c.uncarve();
    c.set_sea_level(None);
    assert_eq!(c.data, fetched);
    assert!(c.carved().is_none());
    assert!(c.current_water_rings().is_some());
  }
}
