use std::sync::Mutex;

mod commands;
mod edits;
mod flood;
mod geotiff;
mod glyphs;
mod grid;
mod landmass;
mod mesh;
mod ocean;
mod selection;
#[cfg(test)]
mod test_support;
mod water;

struct CachedElevation {
  data: Vec<f32>,
  width: usize,
  height: usize,
  #[allow(unused)]
  min_elev: f32,
  #[allow(unused)]
  max_elev: f32,
  south: f64,
  north: f64,
  west: f64,
  east: f64,
  water_mask: Option<Vec<bool>>,
  /// Water polygon rings from the fetch, kept so smoothed exports can
  /// re-rasterize the true OSM shorelines at export resolution instead of
  /// upsampling the DEM-res mask.
  water_rings: Option<Vec<water::RingGroup>>,
  /// Connected-component id per pixel: 0 = water, 1..=n = landmass.
  labels: Option<Vec<u32>>,
  n_landmasses: u32,
  /// Freehand lasso selection (drawn polygon ∩ land) at full resolution.
  lasso: Option<Vec<bool>>,
  /// The lasso's drawn ring, kept for export-resolution re-rasterization.
  lasso_ring: Option<Vec<[f64; 2]>>,
  /// Sea level and carvings; `data`, `water_mask` and `labels` follow them.
  edits: edits::Edits,
}

pub struct ElevationState(Mutex<Option<CachedElevation>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder
    ::default()
    .manage(ElevationState(Mutex::new(None)))
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_dialog::init())
    .invoke_handler(
      tauri::generate_handler![
        commands::fetch_elevation,
        commands::export_stl,
        commands::export_mask,
        commands::landmass_at,
        commands::landmass_overlay,
        commands::lasso_select,
        commands::lasso_overlay,
        commands::set_sea_level,
        commands::carve_text,
        commands::undo_carving
      ]
    )
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
