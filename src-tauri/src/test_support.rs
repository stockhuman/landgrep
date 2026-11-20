use crate::CachedElevation;
use crate::landmass::label_landmasses;

fn gz_fixture(path: &str) -> serde_json::Value {
  use std::io::Read;
  let file = std::fs::File::open(path).unwrap();
  let mut json = String::new();
  flate2::read::GzDecoder::new(file).read_to_string(&mut json).unwrap();
  serde_json::from_str(&json).unwrap()
}

/// Real Overpass response for the Montreal region (bbox 45.30,-74.25 →
/// 45.75,-73.30, fetched 2026-07-13): 1697 ways + 107 relations including
/// multipolygons with inner rings (islands).
pub(crate) fn montreal_fixture() -> serde_json::Value {
  gz_fixture(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/overpass_montreal.json.gz"))
}

/// Real Overpass response for western Sicily (bbox 37.0333,12.0355 →
/// 38.346,13.5626, fetched 2026-07-18): 212 coastline ways + 2833 water
/// ways + 6 relations. Exercises coastline closing on real data, including
/// an island fragment (Marettimo) whose chain dangles outside the bbox.
pub(crate) fn sicily_fixture() -> serde_json::Value {
  gz_fixture(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/overpass_sicily.json.gz"))
}

/// 2-manifold check: every directed edge appears exactly once with its
/// reverse present (no holes, no duplicate faces), all floats finite, and
/// enclosed volume positive (outward normals).
pub(crate) fn assert_watertight(stl: &[u8]) {
  use std::collections::HashMap;
  let n = u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize;
  assert_eq!(stl.len(), 84 + n * 50);
  let vert = |t: usize, k: usize| -> [f32; 3] {
    let off = 84 + t * 50 + 12 + k * 12;
    let f = |o: usize| f32::from_le_bytes(stl[o..o + 4].try_into().unwrap());
    [f(off), f(off + 4), f(off + 8)]
  };
  let key = |v: [f32; 3]| (v[0].to_bits(), v[1].to_bits(), v[2].to_bits());

  let mut edges: HashMap<_, i32> = HashMap::new();
  let mut vol6 = 0f64;
  for t in 0..n {
    let vs = [vert(t, 0), vert(t, 1), vert(t, 2)];
    for v in vs.iter().flatten() {
      assert!(v.is_finite(), "non-finite coordinate in triangle {t}");
    }
    for e in 0..3 {
      *edges.entry((key(vs[e]), key(vs[(e + 1) % 3]))).or_insert(0) += 1;
    }
    let [a, b, c] = vs.map(|v| v.map(|x| x as f64));
    vol6 +=
      a[0] * (b[1] * c[2] - b[2] * c[1]) -
      a[1] * (b[0] * c[2] - b[2] * c[0]) +
      a[2] * (b[0] * c[1] - b[1] * c[0]);
  }
  for (&(a, b), &cnt) in &edges {
    assert_eq!(cnt, 1, "directed edge repeated — overlapping faces");
    assert_eq!(edges.get(&(b, a)), Some(&1), "unmatched edge — hole in mesh");
  }
  assert!(vol6 > 0.0, "non-positive volume — normals inverted");
}

pub(crate) fn two_block_cache() -> (CachedElevation, Vec<u32>) {
  // 8×8: land block A rows/cols 1–2, land block B rows/cols 5–6, water else.
  let (w, h) = (8usize, 8usize);
  let mut water = vec![true; w * h];
  let mut data = vec![0.0f32; w * h];
  for r in 1..=2 {
    for c in 1..=2 {
      water[r * w + c] = false;
      data[r * w + c] = 100.0;
    }
  }
  for r in 5..=6 {
    for c in 5..=6 {
      water[r * w + c] = false;
      data[r * w + c] = 200.0;
    }
  }
  let (labels, n) = label_landmasses(&water, w, h);
  assert_eq!(n, 2);
  let cache = CachedElevation {
    data,
    width: w,
    height: h,
    min_elev: 0.0,
    max_elev: 200.0,
    south: 45.0,
    north: 46.0,
    west: -74.0,
    east: -73.0,
    water_mask: Some(water),
    water_rings: None,
    labels: Some(labels.clone()),
    n_landmasses: n,
    lasso: None,
    lasso_ring: None,
    edits: Default::default(),
  };
  (cache, labels)
}
