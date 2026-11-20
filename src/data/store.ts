import { create } from "zustand"

export interface Bounds {
  north: number
  south: number
  east: number
  west: number
}

export interface Heightmap {
  data: number[]
  width: number
  height: number
  minElev: number
  maxElev: number
  /** Elevation all water pixels were flattened to; null when masking was off,
   *  failed, or found no water in the bbox. */
  waterLevel: number | null
  waterMaskError: string | null
  /** Number of connected land components; null when no mask was built. */
  landmassCount: number | null
  /** Water mask at preview resolution (1 = water); null when no mask was built. */
  water: number[] | null
  /** Land area left above water; null when no mask was built. */
  landKm2: number | null
}

export interface LandmassInfo {
  id: number
  areaKm2: number
  /** Sea level at which the last of it goes under: its highest point. */
  submergedAt: number
}

/** A labelled stop on the sea-level slider. */
export interface SeaMark {
  level: number
  label: string
  title: string
}

/** What set_sea_level, carve_text and undo_carving send back. */
export interface TerrainUpdate {
  heightmap: Heightmap
  overlay: string | null
  /** Highest point the latest carving left. */
  carvedTop: number | null
}

/** What the fetch looked like before anyone touched the sea. */
export interface FetchedStats {
  maxElev: number
  landKm2: number | null
}

export interface LassoInfo {
  areaKm2: number
}

export interface MapHome {
  lat: number
  lng: number
  zoom: number
}

// DEM products available via OpenTopography globaldem endpoint
export const DEM_TYPES = [
  { value: "SRTMGL3", label: "SRTM GL3 — 90 m" },
  { value: "SRTMGL1", label: "SRTM GL1 — 30 m" },
  { value: "AW3D30",  label: "AW3D30 — 30 m" },
  { value: "COP30",   label: "Copernicus — 30 m" },
] as const

export type DemType = (typeof DEM_TYPES)[number]["value"]

/** Pixels sent to the 3D preview over IPC. */
export const PREVIEW_RES = 256

const DEFAULT_HOME: MapHome = { lat: 45.54, lng: -73.65, zoom: 10 }

interface State {
  apiKey: string
  bounds: Bounds | null
  heightmap: Heightmap | null
  /** Bounds snapshot of the last successful fetch (bbox inputs may be edited after). */
  fetchedBounds: Bounds | null
  /** Data-URL PNG of the landmass segmentation, draped on the map. */
  landmassOverlay: string | null
  /** Shift-click accumulates multiple landmasses into one selection. */
  selectedLandmasses: LandmassInfo[]
  /** Freehand lasso selection (clipped to land); exclusive with landmass selection. */
  lassoSelection: LassoInfo | null
  /** Metres above today's sea level (0 = the coastline as fetched). */
  seaLevel: number
  fetchedStats: FetchedStats | null
  seaMarks: SeaMark[]
  /** Text for the carve tool, and how many carvings are in. */
  carveText: string
  carvings: number
  /** Letter height and relief in metres; null = scaled to the fetch. */
  carveSizeM: number | null
  carveReliefM: number | null
  /** What the carve tool last had to say. */
  carveNote: string | null
  verticalScale: number
  demType: DemType
  /** Max pixel dimension of the mesh written to the STL file (0 = full res). */
  exportRes: number
  /** Thickness (mm) of the solid slab under the lowest terrain in exports. */
  baseMm: number
  /** Edge-smoothing supersample factor for exports (1 = off, 2–4 = smoother
   *  borders at factor× the mesh dimensions). */
  smoothFactor: number
  waterMask: boolean
  mapHome: MapHome
  setApiKey: (key: string) => void
  setBounds: (bounds: Bounds | null) => void
  setHeightmap: (hm: Heightmap | null) => void
  setFetchedBounds: (b: Bounds | null) => void
  setLandmassOverlay: (url: string | null) => void
  setSelectedLandmasses: (lms: LandmassInfo[]) => void
  setLassoSelection: (l: LassoInfo | null) => void
  setSeaLevel: (m: number) => void
  setFetchedStats: (s: FetchedStats | null) => void
  setSeaMarks: (marks: SeaMark[]) => void
  setCarveText: (t: string) => void
  setCarvings: (n: number) => void
  setCarveSizeM: (m: number | null) => void
  setCarveReliefM: (m: number | null) => void
  setCarveNote: (note: string | null) => void
  /** Show a terrain update: preview, overlay, and no selection. */
  applyUpdate: (u: TerrainUpdate) => void
  setVerticalScale: (scale: number) => void
  setDemType: (t: DemType) => void
  setExportRes: (r: number) => void
  setBaseMm: (mm: number) => void
  setSmoothFactor: (f: number) => void
  setWaterMask: (v: boolean) => void
  setMapHome: (home: MapHome) => void
}

const load = (key: string, fallback: string) =>
  localStorage.getItem(key) ?? fallback

const loadJson = <T>(key: string, fallback: T): T => {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : fallback
  } catch {
    return fallback
  }
}

export const useStore = create<State>()(set => ({
  apiKey: load("ot-api-key", ""),
  bounds: null,
  heightmap: null,
  fetchedBounds: null,
  landmassOverlay: null,
  selectedLandmasses: [],
  lassoSelection: null,
  seaLevel: 0,
  fetchedStats: null,
  seaMarks: [],
  carveText: load("carve-text", "MINE"),
  carvings: 0,
  carveSizeM: null,
  carveReliefM: null,
  carveNote: null,
  verticalScale: parseFloat(load("vertical-scale", "1.5")),
  demType: (load("dem-type", "SRTMGL3") as DemType),
  exportRes: parseInt(load("export-res", "512")),
  baseMm: parseFloat(load("base-mm", "2")),
  smoothFactor: parseInt(load("smooth-factor", "1")),
  waterMask: load("water-mask", "true") === "true",
  mapHome: loadJson("map-home", DEFAULT_HOME),
  setApiKey: key => {
    localStorage.setItem("ot-api-key", key)
    set({ apiKey: key })
  },
  setBounds: bounds => set({ bounds }),
  setHeightmap: hm => set({ heightmap: hm }),
  setFetchedBounds: b => set({ fetchedBounds: b }),
  setLandmassOverlay: url => set({ landmassOverlay: url }),
  setSelectedLandmasses: lms => set({ selectedLandmasses: lms }),
  setLassoSelection: l => set({ lassoSelection: l }),
  setSeaLevel: m => set({ seaLevel: m }),
  setFetchedStats: s => set({ fetchedStats: s }),
  setSeaMarks: marks => set({ seaMarks: marks }),
  setCarveText: t => {
    localStorage.setItem("carve-text", t)
    set({ carveText: t })
  },
  setCarvings: n => set({ carvings: n }),
  setCarveSizeM: m => set({ carveSizeM: m }),
  setCarveReliefM: m => set({ carveReliefM: m }),
  setCarveNote: note => set({ carveNote: note }),
  applyUpdate: u =>
    set({
      heightmap: u.heightmap,
      landmassOverlay: u.overlay,
      selectedLandmasses: [],
      lassoSelection: null,
    }),
  setVerticalScale: scale => {
    localStorage.setItem("vertical-scale", String(scale))
    set({ verticalScale: scale })
  },
  setDemType: t => {
    localStorage.setItem("dem-type", t)
    set({ demType: t })
  },
  setExportRes: r => {
    localStorage.setItem("export-res", String(r))
    set({ exportRes: r })
  },
  setBaseMm: mm => {
    localStorage.setItem("base-mm", String(mm))
    set({ baseMm: mm })
  },
  setSmoothFactor: f => {
    localStorage.setItem("smooth-factor", String(f))
    set({ smoothFactor: f })
  },
  setWaterMask: v => {
    localStorage.setItem("water-mask", String(v))
    set({ waterMask: v })
  },
  setMapHome: home => {
    localStorage.setItem("map-home", JSON.stringify(home))
    set({ mapHome: home })
  },
}))
