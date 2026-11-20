use serde::{ Deserialize, Serialize };

use crate::{ CachedElevation, ElevationState };
use crate::geotiff::parse_geotiff;
use crate::glyphs::Carving;
use crate::grid::{ downsample, smooth_upsample, upsample_bilinear };
use crate::landmass::{ LANDMASS_PALETTE, label_landmasses, landmass_palette_slots };
use crate::selection::{
  SmoothSelection,
  cell_area_m2,
  crop_to_footprint,
  crop_to_landmasses,
  lasso_footprint,
  vector_footprint,
};
use crate::mesh::build_stl;
use crate::ocean::{ flood_ocean_from_border, ocean_ring_group };
use crate::water::{ WaterGeometry, fetch_water_geometry, flatten_water, rasterize_ring_groups };

#[derive(Debug, Serialize, Deserialize)]
pub struct Heightmap {
  pub data: Vec<f32>,
  pub width: usize,
  pub height: usize,
  #[serde(rename = "minElev")]
  pub min_elev: f32,
  #[serde(rename = "maxElev")]
  pub max_elev: f32,
  /// Elevation all water pixels were flattened to; None when masking was off,
  /// failed, or found no water in the bbox.
  #[serde(rename = "waterLevel")]
  pub water_level: Option<f32>,
  #[serde(rename = "waterMaskError")]
  pub water_mask_error: Option<String>,
  /// Number of connected land components; None when no mask was built.
  #[serde(rename = "landmassCount")]
  pub landmass_count: Option<u32>,
  /// Water mask at preview resolution (1 = water), so the preview can tell
  /// water from dry land that sits below the water level.
  pub water: Option<Vec<u8>>,
  /// Land area left above water; None when no mask was built.
  #[serde(rename = "landKm2")]
  pub land_km2: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct LandmassInfo {
  pub id: u32,
  #[serde(rename = "areaKm2")]
  pub area_km2: f64,
  /// Sea level at which the last of it goes under: its highest point.
  #[serde(rename = "submergedAt")]
  pub submerged_at: f32,
}

/// What changing the terrain (sea level, carvings) sends back, in one round
/// trip: the new preview and landmass overlay.
#[derive(Debug, Serialize)]
pub struct TerrainUpdate {
  pub heightmap: Heightmap,
  /// Landmass overlay for the new coastline (data URL); None without a mask.
  pub overlay: Option<String>,
  /// Highest point the latest carving left.
  #[serde(rename = "carvedTop")]
  pub carved_top: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct LassoInfo {
  #[serde(rename = "areaKm2")]
  pub area_km2: f64,
}

fn encode_png_rgba(rgba: &[u8], width: usize, height: usize) -> Result<Vec<u8>, String> {
  let mut out = Vec::new();
  {
    let mut enc = png::Encoder::new(&mut out, width as u32, height as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
  }
  Ok(out)
}

/// Downsampled preview of the cached terrain, plus the stats the UI shows.
fn heightmap_preview(
  c: &CachedElevation,
  preview_res: usize,
  water_level: Option<f32>,
  water_mask_error: Option<String>
) -> Heightmap {
  let cap = if preview_res == 0 { 256 } else { preview_res };
  let (data, width, height) = downsample(&c.data, c.width, c.height, cap);
  let water = c.water_mask.as_ref().map(|m| {
    downsample(m, c.width, c.height, cap)
      .0.into_iter()
      .map(u8::from)
      .collect()
  });
  let land_km2 = c.water_mask.as_ref().map(|m| {
    let land_px = m
      .iter()
      .filter(|&&w| !w)
      .count();
    ((land_px as f64) * cell_area_m2(c)) / 1.0e6
  });
  Heightmap {
    data,
    width,
    height,
    min_elev: c.data.iter().copied().fold(f32::INFINITY, f32::min),
    max_elev: c.data.iter().copied().fold(f32::NEG_INFINITY, f32::max),
    water_level,
    water_mask_error,
    landmass_count: c.labels.as_ref().map(|_| c.n_landmasses),
    water,
    land_km2,
  }
}

#[tauri::command]
pub async fn fetch_elevation(
  state: tauri::State<'_, ElevationState>,
  south: f64,
  north: f64,
  west: f64,
  east: f64,
  api_key: String,
  dem_type: String,
  preview_res: usize,
  water_mask: bool
) -> Result<Heightmap, String> {
  let url = format!(
    "https://portal.opentopography.org/API/globaldem\
         ?demtype={dem_type}\
         &south={south}&north={north}&west={west}&east={east}\
         &outputFormat=GTiff\
         &API_Key={api_key}"
  );

  let bytes = reqwest
    ::get(&url).await
    .map_err(|e| e.to_string())?
    .error_for_status()
    .map_err(|e| format!("API error: {e}"))?
    .bytes().await
    .map_err(|e| e.to_string())?;

  let (full_data, width, height) = parse_geotiff(bytes.to_vec())?;
  let water = if water_mask {
    Some(fetch_water_geometry(south, north, west, east).await)
  } else {
    None
  };

  let (cache, water_level, water_mask_error) = build_terrain(
    full_data,
    width,
    height,
    (south, north, west, east),
    water
  );
  let preview = heightmap_preview(&cache, preview_res, water_level, water_mask_error);
  *state.0.lock().unwrap() = Some(cache);
  Ok(preview)
}

/// Everything fetch_elevation does once the downloads are in: mask the water
/// (closing coastlines into an ocean, or flood-filling one), flatten it and
/// label the landmasses. `water` is the Overpass result, None when the water
/// mask is off. No network, so fixtures can drive it. Returns the cache, the
/// water level and any water-mask error for the preview.
pub(crate) fn build_terrain(
  mut full_data: Vec<f32>,
  width: usize,
  height: usize,
  (south, north, west, east): (f64, f64, f64, f64),
  water: Option<Result<WaterGeometry, String>>
) -> (CachedElevation, Option<f32>, Option<String>) {
  let mut water_level: Option<f32> = None;
  let mut water_mask_error: Option<String> = None;
  let mut cached_mask: Option<Vec<bool>> = None;
  let mut cached_rings: Option<Vec<crate::water::RingGroup>> = None;
  let mut labels: Option<Vec<u32>> = None;
  let mut n_landmasses = 0u32;
  if let Some(result) = water {
    match result {
      Ok(geo) => {
        let mut rings = geo.water_groups;
        // Open sea only exists in OSM as natural=coastline ways; close them
        // along the bbox into an ocean ring group so the sea is masked like
        // any other water. Incomplete coastline data falls back to an
        // elevation flood-fill from the raster border.
        let coastline_closed = match ocean_ring_group(&geo.coastlines, south, north, west, east) {
          Ok(Some(group)) => {
            rings.push(group);
            true
          }
          Ok(None) => geo.coastlines.is_empty(), // inland bbox: nothing to close
          Err(_) => false,
        };
        let mut mask = rasterize_ring_groups(&rings, south, north, west, east, width, height);
        let mut rings_cover_all_water = true;
        if !coastline_closed {
          let ocean = flood_ocean_from_border(&full_data, width, height);
          if ocean.iter().any(|&o| o) {
            for (m, &o) in mask.iter_mut().zip(&ocean) {
              *m |= o;
            }
            // the flood-filled ocean has no vector form — smoothed exports
            // must fall back to the raster footprint upsample
            rings_cover_all_water = false;
          }
        }
        water_level = flatten_water(&mut full_data, &mask, width);
        let (l, n) = label_landmasses(&mask, width, height);
        labels = Some(l);
        n_landmasses = n;
        cached_mask = Some(mask);
        cached_rings = rings_cover_all_water.then_some(rings);
      }
      Err(e) => {
        water_mask_error = Some(e);
      }
    }
  }

  let min_elev = full_data.iter().cloned().fold(f32::INFINITY, f32::min);
  let max_elev = full_data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

  let cache = CachedElevation {
    data: full_data,
    width,
    height,
    min_elev,
    max_elev,
    south,
    north,
    west,
    east,
    water_mask: cached_mask,
    water_rings: cached_rings,
    labels,
    n_landmasses,
    lasso: None,
    lasso_ring: None,
    edits: Default::default(),
  };
  (cache, water_level, water_mask_error)
}

fn terrain_update(
  c: &CachedElevation,
  preview_res: usize,
  water_level: Option<f32>
) -> Result<TerrainUpdate, String> {
  let overlay = match &c.labels {
    Some(labels) => {
      Some(render_landmass_overlay(labels, &Paint::of(c), c.width, c.height, c.n_landmasses, &[])?)
    }
    None => None,
  };
  Ok(TerrainUpdate {
    heightmap: heightmap_preview(c, preview_res, water_level, None),
    overlay,
    carved_top: c.edits.carved_top,
  })
}

/// Raise the sea to `level` metres (None, or ≤ 0, puts the fetched
/// coastline back). The slider calls this as it moves. Any selection is
/// dropped: landmass ids don't survive a new coastline.
#[tauri::command]
pub async fn set_sea_level(
  state: tauri::State<'_, ElevationState>,
  level: Option<f32>,
  preview_res: usize
) -> Result<TerrainUpdate, String> {
  let mut guard = state.0.lock().unwrap();
  let c = guard.as_mut().ok_or("No terrain loaded — fetch first")?;
  let water_level = c.set_sea_level(level);
  terrain_update(c, preview_res, water_level)
}

/// Carve `text` into the terrain, centred on (lat, lng): capitals
/// `letter_height_m` tall, raised by `relief_m` (negative engraves). Letters
/// raised out of water become land. Survives sea-level changes; undo with
/// `undo_carving`.
#[tauri::command]
pub async fn carve_text(
  state: tauri::State<'_, ElevationState>,
  text: String,
  lat: f64,
  lng: f64,
  letter_height_m: f64,
  relief_m: f32,
  preview_res: usize
) -> Result<TerrainUpdate, String> {
  let text = text.trim().to_string();
  if text.is_empty() {
    return Err("Nothing to carve".into());
  }
  if text.chars().count() > 60 {
    return Err("That's a lot of name. 60 characters, tops.".into());
  }
  if !(letter_height_m > 0.0 && letter_height_m.is_finite() && relief_m.is_finite()) {
    return Err("Letters need a positive size and a finite relief".into());
  }
  let mut guard = state.0.lock().unwrap();
  let c = guard.as_mut().ok_or("No terrain loaded — fetch first")?;
  let water_level = c.carve(Carving { text, lat, lng, letter_height_m, relief_m });
  if c.edits.carved_top.is_none() {
    c.uncarve();
    return Err("That missed the terrain".into());
  }
  terrain_update(c, preview_res, water_level)
}

/// Take back the latest carving.
#[tauri::command]
pub async fn undo_carving(
  state: tauri::State<'_, ElevationState>,
  preview_res: usize
) -> Result<TerrainUpdate, String> {
  let mut guard = state.0.lock().unwrap();
  let c = guard.as_mut().ok_or("No terrain loaded — fetch first")?;
  let water_level = c.uncarve();
  terrain_update(c, preview_res, water_level)
}

#[tauri::command]
pub async fn export_stl(
  app: tauri::AppHandle,
  state: tauri::State<'_, ElevationState>,
  vertical_scale: f32,
  export_res: usize,
  base_mm: f32,
  landmasses: Option<Vec<u32>>,
  lasso: Option<bool>,
  smooth_factor: Option<usize>
) -> Result<String, String> {
  let ids = landmasses.unwrap_or_default();
  let use_lasso = lasso.unwrap_or(false);
  let smooth = smooth_factor.unwrap_or(1).clamp(1, 4);

  let (crop, water_rings, lasso_ring) = {
    let guard = state.0.lock().unwrap();
    let c = guard.as_ref().ok_or("No terrain loaded — fetch first")?;
    let crop = if use_lasso {
      let keep = c.lasso.as_ref().ok_or("No lasso selection — draw one first")?;
      crop_to_footprint(c, keep)?
    } else if ids.is_empty() {
      let n = c.width * c.height;
      (c.data.clone(), vec![true; n], c.width, c.height, c.south, c.north, c.west, c.east)
    } else {
      let labels = c.labels
        .as_ref()
        .ok_or("No landmass data — fetch with the water mask enabled")?;
      crop_to_landmasses(c, labels, &ids)?
    };
    // vector geometry only needed for the smoothed-export path
    let water_rings = if smooth > 1 { c.current_water_rings().cloned() } else { None };
    // The vector lasso is only clipped to water through the rings, so with
    // water the rings don't describe (a raised sea, or the raster ocean
    // fallback) it would put the water back: smooth the raster lasso instead.
    let rings_clip_lasso = water_rings.is_some() || c.water_mask.is_none();
    let lasso_ring = if smooth > 1 && use_lasso && rings_clip_lasso {
      c.lasso_ring.clone()
    } else {
      None
    };
    (crop, water_rings, lasso_ring)
  };
  let (data, footprint, width, height, south, north, west, east) = crop;

  let (mesh_data, mesh_footprint, mesh_w, mesh_h) = if export_res > 0 {
    let (d, mw, mh) = downsample(&data, width, height, export_res);
    let (f, _, _) = downsample(&footprint, width, height, export_res);
    (d, f, mw, mh)
  } else {
    (data, footprint, width, height)
  };

  // Edge smoothing: supersample the (possibly downsampled) mesh grid so
  // footprint borders stop tracing pixel edges. When the original vector
  // geometry is available the footprint is re-rasterized from it at the
  // supersampled resolution — true OSM shorelines instead of a blur-rounded
  // staircase; otherwise fall back to the raster blur+threshold upsample.
  let (mesh_data, mesh_footprint, mesh_w, mesh_h) = if smooth > 1 {
    let sel = if use_lasso {
      lasso_ring.as_deref().map(SmoothSelection::Lasso)
    } else if !ids.is_empty() && water_rings.is_some() {
      Some(SmoothSelection::Landmasses(&mesh_footprint, mesh_w, mesh_h))
    } else {
      None
    };
    match sel {
      Some(sel) => {
        let (d, uw, uh) = upsample_bilinear(&mesh_data, mesh_w, mesh_h, smooth);
        let fp = vector_footprint(water_rings.as_deref(), sel, south, north, west, east, uw, uh)?;
        (d, fp, uw, uh)
      }
      None => smooth_upsample(&mesh_data, &mesh_footprint, mesh_w, mesh_h, smooth)?,
    }
  } else {
    (mesh_data, mesh_footprint, mesh_w, mesh_h)
  };

  use tauri_plugin_dialog::DialogExt;
  let path = app
    .dialog()
    .file()
    .set_title("Export terrain as STL")
    .add_filter("STL file", &["stl"])
    .blocking_save_file();

  let path = match path {
    Some(p) => p.into_path().map_err(|e| e.to_string())?,
    None => {
      return Err("Cancelled".into());
    }
  };

  let stl = build_stl(
    &mesh_data,
    &mesh_footprint,
    mesh_w,
    mesh_h,
    vertical_scale,
    base_mm,
    south,
    north,
    west,
    east
  )?;
  std::fs::write(&path, stl).map_err(|e| e.to_string())?;

  Ok(path.to_string_lossy().into_owned())
}

/// Identify the landmass under a map click. Returns None over water or when
/// the point falls outside the fetched bounds.
#[tauri::command]
pub async fn landmass_at(
  state: tauri::State<'_, ElevationState>,
  lat: f64,
  lng: f64
) -> Result<Option<LandmassInfo>, String> {
  let guard = state.0.lock().unwrap();
  let c = guard.as_ref().ok_or("No terrain loaded — fetch first")?;
  let labels = c.labels.as_ref().ok_or("No landmass data — fetch with the water mask enabled")?;

  if lat < c.south || lat > c.north || lng < c.west || lng > c.east {
    return Ok(None);
  }
  let col = (((lng - c.west) / (c.east - c.west)) * ((c.width as f64) - 1.0)).round() as usize;
  let row = (((c.north - lat) / (c.north - c.south)) * ((c.height as f64) - 1.0)).round() as usize;
  let id = labels[row.min(c.height - 1) * c.width + col.min(c.width - 1)];
  if id == 0 {
    return Ok(None);
  }

  let (mut px_count, mut highest) = (0usize, f32::NEG_INFINITY);
  for (&l, &elev) in labels.iter().zip(&c.data) {
    if l == id {
      px_count += 1;
      highest = highest.max(elev);
    }
  }
  Ok(
    Some(LandmassInfo {
      id,
      area_km2: ((px_count as f64) * cell_area_m2(c)) / 1.0e6,
      submerged_at: highest,
    })
  )
}

/// Store a freehand lasso selection (points are [lat, lng] pairs) and return
/// its land area. The mask lives in the cache until the next fetch; export
/// with `export_stl { lasso: true }`.
#[tauri::command]
pub async fn lasso_select(
  state: tauri::State<'_, ElevationState>,
  points: Vec<[f64; 2]>
) -> Result<LassoInfo, String> {
  let mut guard = state.0.lock().unwrap();
  let c = guard.as_mut().ok_or("No terrain loaded — fetch first")?;
  let mask = lasso_footprint(c, &points)?;
  let px = mask
    .iter()
    .filter(|&&m| m)
    .count();
  let area_km2 = ((px as f64) * cell_area_m2(c)) / 1.0e6;
  c.lasso = Some(mask);
  c.lasso_ring = Some(points);
  Ok(LassoInfo { area_km2 })
}

/// Render the landmass segmentation as a translucent RGBA PNG (data URL) for
/// use as a georeferenced Leaflet image overlay. Water is transparent, except
/// land a raised sea has drowned, which is painted as water. With `selected`,
/// that landmass is highlighted and the rest fade back.
#[tauri::command]
pub async fn landmass_overlay(
  state: tauri::State<'_, ElevationState>,
  selected: Option<Vec<u32>>
) -> Result<String, String> {
  let selected = selected.unwrap_or_default();
  let (labels, paint, width, height, n_landmasses) = {
    let guard = state.0.lock().unwrap();
    let c = guard.as_ref().ok_or("No terrain loaded — fetch first")?;
    let labels = c.labels
      .clone()
      .ok_or("No landmass data — fetch with the water mask enabled")?;
    (labels, Paint::of(c), c.width, c.height, c.n_landmasses)
  };
  render_landmass_overlay(&labels, &paint, width, height, n_landmasses, &selected)
}

/// What the edits add to the map overlays, copied out of the cache so the
/// PNGs encode without holding the lock: land a raised sea has drowned
/// (painted as water, since the map under it still shows streets), and the
/// carvings (chalk, like a hill figure).
struct Paint {
  drowned: Option<Vec<bool>>,
  carved: Option<Vec<bool>>,
}

impl Paint {
  const DROWNED: ([u8; 3], u8) = ([74, 128, 181], 150); // the preview's water
  const CHALK: ([u8; 3], u8) = ([250, 250, 245], 200);

  fn of(c: &CachedElevation) -> Self {
    Paint { drowned: c.drowned().map(<[bool]>::to_vec), carved: c.carved().map(<[bool]>::to_vec) }
  }

  /// Nearest-sample both masks to the overlay's resolution.
  fn downsample(&self, width: usize, height: usize, max_dim: usize) -> Paint {
    let d = |m: &Option<Vec<bool>>| m.as_ref().map(|m| downsample(m, width, height, max_dim).0);
    Paint { drowned: d(&self.drowned), carved: d(&self.carved) }
  }

  fn drowned(&self, i: usize) -> bool {
    self.drowned.as_ref().is_some_and(|d| d[i])
  }

  fn carved(&self, i: usize) -> bool {
    self.carved.as_ref().is_some_and(|c| c[i])
  }
}

/// landmass_overlay's image, shared with the terrain-changing commands.
fn render_landmass_overlay(
  labels: &[u32],
  paint: &Paint,
  width: usize,
  height: usize,
  n_landmasses: u32,
  selected: &[u32]
) -> Result<String, String> {
  const MAX_DIM: usize = 1024;
  let slots = landmass_palette_slots(labels, n_landmasses);

  // nearest-sample downsample, same scheme as elevation preview
  let (labels, w, h) = downsample(labels, width, height, MAX_DIM);
  let paint = paint.downsample(width, height, MAX_DIM);

  let mut rgba = vec![0u8; w * h * 4];
  for (i, &id) in labels.iter().enumerate() {
    let (rgb, alpha) = if id == 0 {
      if !paint.drowned(i) {
        continue; // water stays transparent
      }
      Paint::DROWNED
    } else if selected.contains(&id) {
      ([255u8, 140, 0], 165u8)
    } else if paint.carved(i) {
      Paint::CHALK
    } else if !selected.is_empty() {
      ([130u8, 130, 130], 40u8)
    } else {
      (LANDMASS_PALETTE[slots[id as usize]], 95u8)
    };
    rgba[i * 4..i * 4 + 3].copy_from_slice(&rgb);
    rgba[i * 4 + 3] = alpha;
  }

  let png = encode_png_rgba(&rgba, w, h)?;
  use base64::Engine;
  Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

/// Render the lasso selection as a translucent RGBA PNG (data URL): selected
/// pixels highlighted, remaining land faded back, water transparent — same
/// styling (and the same Paint) as landmass_overlay with a selection.
#[tauri::command]
pub async fn lasso_overlay(state: tauri::State<'_, ElevationState>) -> Result<String, String> {
  const MAX_DIM: usize = 1024;
  let (lasso, land, paint, width, height) = {
    let guard = state.0.lock().unwrap();
    let c = guard.as_ref().ok_or("No terrain loaded — fetch first")?;
    let lasso = c.lasso.clone().ok_or("No lasso selection — draw one first")?;
    let land: Option<Vec<bool>> = c.water_mask
      .as_ref()
      .map(|w|
        w
          .iter()
          .map(|&is_water| !is_water)
          .collect()
      );
    (lasso, land, Paint::of(c), c.width, c.height)
  };

  let (lasso, w, h) = downsample(&lasso, width, height, MAX_DIM);
  let land = land.map(|l| downsample(&l, width, height, MAX_DIM).0);
  let paint = paint.downsample(width, height, MAX_DIM);

  let mut rgba = vec![0u8; w * h * 4];
  for i in 0..w * h {
    let (rgb, alpha): ([u8; 3], u8) = if lasso[i] {
      ([255, 140, 0], 165)
    } else if paint.carved(i) {
      Paint::CHALK
    } else if land.as_ref().is_some_and(|l| l[i]) {
      ([130, 130, 130], 40)
    } else if paint.drowned(i) {
      Paint::DROWNED
    } else {
      continue;
    };
    rgba[i * 4..i * 4 + 3].copy_from_slice(&rgb);
    rgba[i * 4 + 3] = alpha;
  }

  let png = encode_png_rgba(&rgba, w, h)?;
  use base64::Engine;
  Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

#[tauri::command]
pub async fn export_mask(
  app: tauri::AppHandle,
  state: tauri::State<'_, ElevationState>
) -> Result<String, String> {
  let (mask, width, height) = {
    let guard = state.0.lock().unwrap();
    let c = guard.as_ref().ok_or("No terrain loaded — fetch first")?;
    let mask = c.water_mask
      .clone()
      .ok_or("No water mask cached — enable the water mask and fetch again")?;
    (mask, c.width, c.height)
  };

  use tauri_plugin_dialog::DialogExt;
  let path = app
    .dialog()
    .file()
    .set_title("Export water mask as PNG")
    .add_filter("PNG image", &["png"])
    .blocking_save_file();

  let path = match path {
    Some(p) => p.into_path().map_err(|e| e.to_string())?,
    None => {
      return Err("Cancelled".into());
    }
  };

  const WATER: [u8; 3] = [60, 110, 200];
  const LAND: [u8; 3] = [225, 222, 210];
  let mut rgb = Vec::with_capacity(width * height * 3);
  for &is_water in &mask {
    rgb.extend_from_slice(if is_water { &WATER } else { &LAND });
  }

  let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
  let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width as u32, height as u32);
  enc.set_color(png::ColorType::Rgb);
  enc.set_depth(png::BitDepth::Eight);
  let mut writer = enc.write_header().map_err(|e| e.to_string())?;
  writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
  writer.finish().map_err(|e| e.to_string())?;

  Ok(path.to_string_lossy().into_owned())
}
