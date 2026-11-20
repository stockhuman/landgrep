import { invoke } from "@tauri-apps/api/core"
import type { Bounds, Heightmap, LandmassInfo, SeaMark } from "./store"

type Point = { lat: number; lng: number }

const LAVAL: Point = { lat: 45.58, lng: -73.74 } // Chomedey, Île Jésus
const MONT_ROYAL: Point = { lat: 45.5059, lng: -73.5875 } // the summit

const inside = (b: Bounds, p: Point) =>
  p.lat >= b.south && p.lat <= b.north && p.lng >= b.west && p.lng <= b.east

/**
 * Labelled stops for the sea-level slider, for a fresh fetch. Two apply
 * anywhere; the rest only show up when the fetch covers them.
 */
export async function seaMarks(bounds: Bounds, hm: Heightmap): Promise<SeaMark[]> {
  const top = Math.ceil(hm.maxElev)
  const marks: SeaMark[] = []
  if (top > 70) {
    marks.push({
      level: 70,
      label: "The ice melts",
      title: "If every glacier and ice sheet on Earth melted, the sea would rise about 70 m (USGS)",
    })
  }
  if (inside(bounds, MONT_ROYAL) && top > 171) {
    marks.push({
      level: 171,
      label: "Champlain Sea",
      title:
        "One estimate of the Champlain Sea's highest shoreline around Montreal, roughly 13,000 years ago. Mont Royal was an island.",
    })
  }
  if (inside(bounds, LAVAL) && hm.landmassCount) {
    const laval = await invoke<LandmassInfo | null>("landmass_at", LAVAL).catch(() => null)
    if (laval) {
      marks.push({
        level: Math.ceil(laval.submergedAt),
        label: "Laval",
        title: "The sea level at which the last of Laval goes under",
      })
    }
  }
  if (top > 0) {
    marks.push({
      level: top,
      label: "Nothing left",
      title: "The highest point in the region goes under",
    })
  }
  return marks.sort((a, b) => a.level - b.level)
}

/** One line on what the sea has done so far. */
export function seaReport(
  level: number,
  hm: Heightmap,
  fetchedLandKm2: number | null,
  marks: SeaMark[],
) {
  if (level <= 0) return "The coastline as fetched"
  if (hm.landKm2 == null) return `+${level} m`
  if (hm.landmassCount === 0) return "Nothing left. You've won."
  // floor: "100% gone" is for when nothing is left
  const lost = fetchedLandKm2 ? Math.floor((1 - hm.landKm2 / fetchedLandKm2) * 100) : null
  const n = hm.landmassCount ?? 0
  const parts = [
    `${n.toLocaleString()} landmass${n === 1 ? "" : "es"}`,
    `${fmtKm2(hm.landKm2)} km² of land${lost != null ? ` (${lost}% gone)` : ""}`,
  ]
  const laval = marks.find(m => m.label === "Laval")
  if (laval && level >= laval.level) parts.push("Laval has been dealt with")
  return parts.join(" · ")
}

const fmtKm2 = (km2: number) =>
  km2 >= 100 ? km2.toFixed(0) : km2 >= 1 ? km2.toFixed(1) : km2.toFixed(2)
