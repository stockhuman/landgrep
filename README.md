# landgrep

Draw a box on a map and get a 3D-printable model of the land inside it.

landgrep fetches elevation for the box from [OpenTopography](https://opentopography.org),
masks the water with OpenStreetMap (lakes, rivers, and the sea, closed from coastlines),
flattens it, and splits what's left into landmasses you can click. Export the whole box,
any set of landmasses, or a freehand lasso as a watertight STL on a solid base.

It is also one letter away from _landgrab_, so it can raise the sea and carve your name
into whatever is left.

## Running it

You need Rust, Node, pnpm and a free OpenTopography API key, which goes in Settings.

```bash
pnpm install
pnpm tauri dev
```

## Using it

1. **✏ Draw** a box on the map (hold shift for a square), or type its bounds, then
   **Fetch Terrain**.
2. Click a landmass to select it; shift-click to add more. Or **◌ Lasso** a region. The
   lasso is clipped to land, so it only needs to be rough, and it can split landmasses that
   bridges or causeways have fused.
3. **Export STL** in the preview exports the whole box; **Export Landmass** or **Export
   Selection** in the bottom bar exports the selection. **Export Mask** saves the water
   mask as a PNG.

Settings has the elevation source (90 m SRTM down to 30 m Copernicus), vertical
exaggeration, export resolution, edge smoothing and base thickness.

### Sea level

The slider under the 3D preview raises the sea. Water comes in from the edge of the map and
through the rivers and lakes already on it, and only reaches what it can flow to: a valley
behind a ridge stays dry until the water tops the ridge. The new coastline is real. Its
islands select, lasso and export like any other landmass, and a selected landmass tells you
the sea level that finishes it off.

Presets: the ice melts (+70 m, per the USGS) and, when the box covers them, the Champlain
Sea (Montreal, roughly 13,000 years ago) and Laval.

### Geoglyphs

**✍ Carve** writes text into the terrain, centred where you click, in capitals kilometres
tall, raised or engraved. Letters raised out of a lake become land, and carvings go in
before the sea rises, so your name outlives the flood. **Undo** takes them back one at a
time.

## The model

Exports are 200 mm along their longest side, with +x east, +y north and z up. Relief is
true to scale times the vertical exaggeration, measured from the lowest point of the
selection, on top of the base slab. The 3D preview draws the same thing.

## Tests

```bash
cd src-tauri && cargo test
```

The fixtures are real Overpass responses for Montreal and western Sicily.
`LANDGREP_LIVE_OVERPASS=1` adds two checks against the live API.

## Data

Elevation: SRTM, ALOS AW3D30 or Copernicus GLO-30, served by OpenTopography. Water: ©
OpenStreetMap contributors, via the Overpass API.
