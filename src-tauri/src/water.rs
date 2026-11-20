use crate::grid::rasterize_rings;

/// Water polygon rings belonging to one OSM element (way or multipolygon
/// relation). Each group must be filled with the even-odd rule on its own
/// and ORed with the rest — one even-odd pass over all elements would make
/// overlapping polygons cancel out.
pub(crate) type RingGroup = Vec<Vec<[f64; 2]>>;

/// Water geometry for a bbox: closed polygon ring groups (lakes, rivers) and
/// raw `natural=coastline` polylines. Coastlines are directional open ways —
/// land on the left — that need closing along the bbox before they can be
/// filled; see `ocean::ocean_ring_group`.
pub(crate) struct WaterGeometry {
  pub(crate) water_groups: Vec<RingGroup>,
  pub(crate) coastlines: Vec<Vec<[f64; 2]>>,
}

/// Fetch water polygon rings and coastline ways from Overpass, grouped per
/// element. Retries across mirror endpoints; errors propagate so the caller
/// can surface them instead of silently rendering unmasked terrain.
pub(crate) async fn fetch_water_geometry(
  south: f64,
  north: f64,
  west: f64,
  east: f64
) -> Result<WaterGeometry, String> {
  const ENDPOINTS: [&str; 3] = [
    "https://overpass-api.de/api/interpreter",
    "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
  ];
  const ATTEMPTS: usize = 4;

  let mut last_err = String::new();
  for attempt in 0..ATTEMPTS {
    let endpoint = ENDPOINTS[attempt % ENDPOINTS.len()];
    match try_fetch_water_geometry(endpoint, south, north, west, east).await {
      Ok(geometry) => {
        return Ok(geometry);
      }
      Err(e) => {
        last_err = e;
      }
    }
    if attempt + 1 < ATTEMPTS {
      tokio::time::sleep(std::time::Duration::from_secs(2 * ((attempt as u64) + 1))).await;
    }
  }
  Err(format!("Overpass water query failed after {ATTEMPTS} attempts: {last_err}"))
}

/// Stitch a collection of way segments (each a Vec of [lat,lon] points) into
/// one or more closed rings by chaining them end-to-end. Returns each closed
/// ring as a Vec<[f64;2]>. Segments are consumed in order and flipped if needed
/// to make endpoints match.
fn assemble_rings(mut segments: Vec<Vec<[f64; 2]>>) -> Vec<Vec<[f64; 2]>> {
  let close_enough = |a: [f64; 2], b: [f64; 2]| {
    (a[0] - b[0]).abs() < 1e-7 && (a[1] - b[1]).abs() < 1e-7
  };

  let mut rings: Vec<Vec<[f64; 2]>> = Vec::new();

  while !segments.is_empty() {
    let mut ring = segments.remove(0);
    let mut changed = true;
    while changed {
      changed = false;
      // If already closed, stop
      if ring.len() >= 3 && close_enough(*ring.first().unwrap(), *ring.last().unwrap()) {
        break;
      }
      let tail = *ring.last().unwrap();
      let mut found = None;
      for (i, seg) in segments.iter().enumerate() {
        if close_enough(tail, seg[0]) {
          found = Some((i, false));
          break;
        }
        if close_enough(tail, *seg.last().unwrap()) {
          found = Some((i, true));
          break;
        }
      }
      if let Some((i, flip)) = found {
        let mut seg = segments.remove(i);
        if flip {
          seg.reverse();
        }
        seg.remove(0); // drop duplicate junction point
        ring.extend(seg);
        changed = true;
      }
    }
    if ring.len() >= 3 {
      rings.push(ring);
    }
  }
  rings
}

fn member_segments(members: &[serde_json::Value], role: &str) -> Vec<Vec<[f64; 2]>> {
  members
    .iter()
    .filter(|m| m["role"].as_str() == Some(role))
    .filter_map(|m| {
      let geom = m["geometry"].as_array()?;
      let pts: Vec<[f64; 2]> = geom
        .iter()
        .filter_map(|g| Some([g["lat"].as_f64()?, g["lon"].as_f64()?]))
        .collect();
      if pts.len() >= 2 {
        Some(pts)
      } else {
        None
      }
    })
    .collect()
}

async fn try_fetch_water_geometry(
  endpoint: &str,
  south: f64,
  north: f64,
  west: f64,
  east: f64
) -> Result<WaterGeometry, String> {
  // `natural=water` covers modern polygon usage; `waterway=riverbank` is the
  // legacy tag. `natural=coastline` is the only representation of open sea —
  // directional open ways, closed along the bbox later.
  let query = format!(
    "[out:json][timeout:60];\
        (\
          way[\"natural\"=\"water\"]({south},{west},{north},{east});\
          way[\"waterway\"=\"riverbank\"]({south},{west},{north},{east});\
          way[\"natural\"=\"coastline\"]({south},{west},{north},{east});\
          relation[\"natural\"=\"water\"]({south},{west},{north},{east});\
          relation[\"waterway\"=\"riverbank\"]({south},{west},{north},{east});\
        );\
        out geom;"
  );

  let client = reqwest::Client
    ::builder()
    .timeout(std::time::Duration::from_secs(75))
    .user_agent(concat!("landgrep/", env!("CARGO_PKG_VERSION")))
    .build()
    .map_err(|e| e.to_string())?;

  let resp = client
    .get(endpoint)
    .query(&[("data", &query)])
    .header("Accept", "application/json")
    .send().await
    .map_err(|e| e.to_string())?;

  // Judge the response by its content, not its HTTP status: overpass-api.de
  // has been observed returning 406 while streaming a complete, valid payload.
  // Real failures (rate limit, gateway errors) don't parse as JSON.
  let status = resp.status();
  let text = resp.text().await.map_err(|e| e.to_string())?;
  let body: serde_json::Value = serde_json
    ::from_str(&text)
    .map_err(|_| {
      let snippet: String = text.chars().take(200).collect();
      format!("HTTP {status}, non-JSON response: {snippet}")
    })?;

  geometry_from_overpass(&body)
}

/// Extract per-element ring groups and coastline polylines from an Overpass
/// response. Keeping the element grouping (rather than one flat ring list)
/// preserves the fill semantics: even-odd within an element, union across
/// elements.
pub(crate) fn geometry_from_overpass(body: &serde_json::Value) -> Result<WaterGeometry, String> {
  // Overpass reports rate limiting / mid-stream timeouts as a `remark` on an
  // otherwise valid (but truncated) response. Truncation silently drops the
  // large relations — exactly the big rivers and lakes — so treat it as failure.
  if let Some(remark) = body["remark"].as_str() {
    return Err(format!("Overpass remark: {remark}"));
  }

  let mut water_groups: Vec<RingGroup> = Vec::new();
  let mut coastlines: Vec<Vec<[f64; 2]>> = Vec::new();

  let elements = body["elements"].as_array().ok_or("no elements")?;
  for el in elements {
    match el["type"].as_str() {
      Some("way") => {
        if let Some(geom) = el["geometry"].as_array() {
          let pts: Vec<[f64; 2]> = geom
            .iter()
            .filter_map(|g| Some([g["lat"].as_f64()?, g["lon"].as_f64()?]))
            .collect();
          if el["tags"]["natural"].as_str() == Some("coastline") {
            if pts.len() >= 2 {
              coastlines.push(pts);
            }
          } else if pts.len() >= 3 {
            water_groups.push(vec![pts]);
          }
        }
      }
      Some("relation") => {
        // Assemble outer segments into closed rings before rasterizing —
        // rasterizing each open segment independently produces wrong fills.
        // Inner rings participate in the same even-odd pass so islands
        // stay land instead of being flooded.
        if let Some(members) = el["members"].as_array() {
          let mut rings = assemble_rings(member_segments(members, "outer"));
          rings.extend(assemble_rings(member_segments(members, "inner")));
          if !rings.is_empty() {
            water_groups.push(rings);
          }
        }
      }
      _ => {}
    }
  }

  Ok(WaterGeometry { water_groups, coastlines })
}

/// Rasterize ring groups onto a grid: even-odd fill per group, union across
/// groups. Resolution-agnostic — the same rings serve the DEM-res water mask
/// and the supersampled export footprint.
pub(crate) fn rasterize_ring_groups(
  groups: &[RingGroup],
  south: f64,
  north: f64,
  west: f64,
  east: f64,
  width: usize,
  height: usize
) -> Vec<bool> {
  let mut mask = vec![false; width * height];
  for g in groups {
    rasterize_rings(g, south, north, west, east, width, height, &mut mask);
  }
  mask
}

/// Water-polygon rings + rasterization in one step (no coastline closing).
/// Production code keeps the stages separate (rings are cached for
/// export-resolution re-rasterization); fixture tests pin this composition's
/// output.
#[cfg(test)]
pub(crate) fn mask_from_overpass(
  body: &serde_json::Value,
  south: f64,
  north: f64,
  west: f64,
  east: f64,
  width: usize,
  height: usize
) -> Result<Vec<bool>, String> {
  let geo = geometry_from_overpass(body)?;
  Ok(rasterize_ring_groups(&geo.water_groups, south, north, west, east, width, height))
}

/// Flatten masked pixels to just below the shoreline (land touching water),
/// not to 0 — zeroing gouges a crater wherever terrain sits well above sea
/// level (e.g. a 230 m reservoir) and makes coastal noise indistinguishable.
///
/// Shores, not all land: an inland pit says nothing about the water, and the
/// Montréal-Est quarry (about −60 m in COP30) used to sink every river in
/// the Montreal bbox 60-odd metres underground. And the shoreline's 1st
/// percentile, not its minimum: DEMs are noisiest right at the water's edge
/// (COP30 has −34 m "banks" on the Rivière des Prairies). Dry land below the
/// level stays where it is. Returns the level used, or None when the mask is
/// empty or all-water.
pub(crate) fn flatten_water(data: &mut [f32], mask: &[bool], width: usize) -> Option<f32> {
  let water_px = mask
    .iter()
    .filter(|&&w| w)
    .count();
  if water_px == 0 || water_px == mask.len() {
    return None;
  }

  let height = mask.len() / width;
  let touches_water = |i: usize| {
    let (row, col) = (i / width, i % width);
    (col > 0 && mask[i - 1]) ||
      (col + 1 < width && mask[i + 1]) ||
      (row > 0 && mask[i - width]) ||
      (row + 1 < height && mask[i + width])
  };
  let mut land_min = f32::INFINITY;
  let mut land_max = f32::NEG_INFINITY;
  let mut shore: Vec<f32> = Vec::new();
  for (i, &is_water) in mask.iter().enumerate() {
    if !is_water {
      land_min = land_min.min(data[i]);
      land_max = land_max.max(data[i]);
      if touches_water(i) {
        shore.push(data[i]);
      }
    }
  }
  // (a grid holding both water and land always has a shore; the guard is
  // for a width that doesn't divide the mask)
  let shore_low = if shore.is_empty() {
    land_min
  } else {
    let k = (shore.len() - 1) / 100;
    *shore.select_nth_unstable_by(k, |a, b| a.total_cmp(b)).1
  };
  let gap = ((land_max - land_min) * 0.02).max(1.0);
  let level = shore_low - gap;
  for (i, &is_water) in mask.iter().enumerate() {
    if is_water {
      data[i] = level;
    }
  }
  Some(level)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::montreal_fixture;

  #[test]
  fn mask_matches_fixture() {
    let body = montreal_fixture();
    let mask = mask_from_overpass(&body, 45.30, 45.75, -74.25, -73.3, 760, 500).unwrap();
    let water = mask
      .iter()
      .filter(|&&w| w)
      .count();
    // Reference count from an independent Python implementation of the same
    // even-odd fill over the identical fixture.
    assert_eq!(water, 54984);

    // The Island of Montreal must be isolated: mid-island is land while the
    // St. Lawrence (S/E), Rivière des Prairies (N) and Lac Saint-Louis (SW)
    // are all water.
    let px = |lat: f64, lon: f64| {
      let col = ((lon - -74.25) / (-73.3 - -74.25)) * 759.0;
      let row = ((45.75 - lat) / (45.75 - 45.30)) * 499.0;
      mask[(row as usize) * 760 + (col as usize)]
    };
    assert!(!px(45.50, -73.62), "mid-island should be land");
    assert!(px(45.47, -73.52), "St. Lawrence should be water");
    assert!(px(45.55, -73.68), "Rivière des Prairies should be water");
    assert!(px(45.40, -73.80), "Lac Saint-Louis should be water");
  }

  #[test]
  fn truncated_response_is_rejected() {
    let body =
      serde_json::json!({
      "remark": "runtime error: Query timed out in \"query\" at line 1",
      "elements": []
    });
    assert!(mask_from_overpass(&body, 0.0, 1.0, 0.0, 1.0, 10, 10).is_err());
  }

  #[test]
  fn assemble_rings_chains_and_flips_segments() {
    // A square split into three segments, middle one reversed, given out of
    // chaining order.
    let segments = vec![
      vec![[0.0, 0.0], [0.0, 1.0]],
      vec![[1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], // closing segment ahead of its turn
      vec![[1.0, 1.0], [0.0, 1.0]] // needs flipping to chain onto the tail
    ];
    let rings = assemble_rings(segments);
    assert_eq!(rings.len(), 1);
    let ring = &rings[0];
    assert_eq!(ring.first(), ring.last(), "ring should close");
    assert_eq!(ring.len(), 5);
  }

  #[test]
  fn assemble_rings_separates_disjoint_rings() {
    let segments = vec![
      vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [0.0, 0.0]],
      vec![[5.0, 5.0], [5.0, 6.0], [6.0, 6.0], [5.0, 5.0]]
    ];
    assert_eq!(assemble_rings(segments).len(), 2);
  }

  #[test]
  fn flatten_water_sits_just_below_the_shore() {
    // Reservoir scenario: a ~232 m reservoir with banks from 233 m, in a
    // bbox whose land runs 197–289 m. Flattening must land just below the
    // lowest bank — never at 0, and not down at the valley floor either,
    // which never touches the water.
    let mut data = vec![
      197.0, 210.0, 240.0, 289.0,
      205.0, 233.0, 232.0, 231.5,
      215.0, 236.0, 232.0, 232.0,
    ];
    let mask = vec![
      false, false, false, false,
      false, false, true,  true,
      false, false, true,  true,
    ];
    let land = data.clone();
    let level = flatten_water(&mut data, &mask, 4).unwrap();
    let expected_gap = ((289.0f32 - 197.0) * 0.02).max(1.0);
    assert_eq!(level, 233.0 - expected_gap);
    for (i, &is_water) in mask.iter().enumerate() {
      assert_eq!(data[i], if is_water { level } else { land[i] });
    }
  }

  #[test]
  fn inland_pits_dont_drag_the_water_down() {
    // The Montréal-Est quarry: a river with 20 m banks and a −57 m pit that
    // never touches it. The river stays just under its banks; the pit stays
    // a dry pit.
    let (w, h) = (6usize, 3usize);
    let mut data = vec![20.0f32; w * h];
    let mask: Vec<bool> = (0..w * h).map(|i| i % w == 0).collect(); // river down col 0
    data[w + 4] = -57.0;
    let level = flatten_water(&mut data, &mask, w).unwrap();
    assert!(level > 0.0 && level < 20.0, "river at {level} m");
    assert_eq!(data[w + 4], -57.0);
  }

  #[test]
  fn a_bad_bank_pixel_doesnt_drag_the_water_down() {
    // A lake down the middle of a 200-row strip, banks at 20 m, and one
    // −34 m bank pixel of DEM noise: the water still sits under the 20 m
    // banks, and the noise is left as a dry notch.
    let (w, h) = (3usize, 200usize);
    let mut data = vec![20.0f32; w * h];
    let mask: Vec<bool> = (0..w * h).map(|i| i % w == 1).collect();
    data[50 * w] = -34.0;
    let level = flatten_water(&mut data, &mask, w).unwrap();
    let gap = ((20.0f32 - -34.0) * 0.02).max(1.0);
    assert_eq!(level, 20.0 - gap);
    assert_eq!(data[50 * w], -34.0);
  }

  #[test]
  fn flatten_water_degenerate_masks_are_noops() {
    let mut data = vec![5.0, 10.0];
    assert_eq!(flatten_water(&mut data, &[false, false], 2), None);
    assert_eq!(flatten_water(&mut data, &[true, true], 2), None);
    assert_eq!(data, vec![5.0, 10.0]);
  }

  /// Live network check: set LANDGREP_LIVE_OVERPASS=1 to exercise the real
  /// endpoint/retry path against a small bbox over Lac Saint-Louis.
  #[test]
  fn live_overpass_fetch() {
    if std::env::var("LANDGREP_LIVE_OVERPASS").is_err() {
      return;
    }
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let geo = rt.block_on(fetch_water_geometry(45.38, 45.42, -73.85, -73.8)).unwrap();
    let mask = rasterize_ring_groups(&geo.water_groups, 45.38, 45.42, -73.85, -73.8, 100, 100);
    let water = mask
      .iter()
      .filter(|&&w| w)
      .count();
    assert!(water > 1000, "expected mostly water over Lac Saint-Louis, got {water} px");
  }

  #[test]
  fn coastline_ways_are_split_from_water_polygons() {
    let body =
      serde_json::json!({
      "elements": [
        {"type": "way", "tags": {"natural": "coastline"}, "geometry": [
          {"lat": 0.0, "lon": 0.5}, {"lat": 1.0, "lon": 0.5}
        ]},
        {"type": "way", "tags": {"natural": "water"}, "geometry": [
          {"lat": 0.1, "lon": 0.1}, {"lat": 0.1, "lon": 0.2},
          {"lat": 0.2, "lon": 0.2}, {"lat": 0.2, "lon": 0.1}
        ]}
      ]
    });
    let geo = geometry_from_overpass(&body).unwrap();
    assert_eq!(geo.coastlines.len(), 1);
    assert_eq!(geo.water_groups.len(), 1);
    // the coastline must NOT be treated as a fillable water ring
    assert_eq!(geo.coastlines[0].len(), 2);
  }

  #[test]
  fn overlapping_elements_fill_union_not_xor() {
    // Two overlapping lakes as separate ways: per-element grouping must OR
    // them; a single even-odd pass would carve a hole out of the overlap.
    let body =
      serde_json::json!({
      "elements": [
        {"type": "way", "geometry": [
          {"lat": 0.1, "lon": 0.1}, {"lat": 0.1, "lon": 0.6},
          {"lat": 0.6, "lon": 0.6}, {"lat": 0.6, "lon": 0.1}
        ]},
        {"type": "way", "geometry": [
          {"lat": 0.4, "lon": 0.4}, {"lat": 0.4, "lon": 0.9},
          {"lat": 0.9, "lon": 0.9}, {"lat": 0.9, "lon": 0.4}
        ]}
      ]
    });
    let mask = mask_from_overpass(&body, 0.0, 1.0, 0.0, 1.0, 100, 100).unwrap();
    let at = |lat: f64, lon: f64| mask[(((1.0 - lat) * 99.0) as usize) * 100 + ((lon * 99.0) as usize)];
    assert!(at(0.5, 0.5), "overlap must stay water");
    assert!(at(0.2, 0.2), "first lake");
    assert!(at(0.8, 0.8), "second lake");
    assert!(!at(0.05, 0.9), "outside both");
  }
}
