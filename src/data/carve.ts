import type { Bounds } from "./store"

/** 1, 1.5, 2, 2.5, 3 … × a power of ten: slider stops that read well. */
const nice = (v: number) => {
  const p = 10 ** Math.floor(Math.log10(v))
  return (Math.round((v / p) * 2) / 2) * p
}

/**
 * Sizes for the carve tool, scaled to the fetch. Letters default to an
 * eighth of the map's height; relief to whatever stands 1.5 mm proud of the
 * terrain in a 200 mm print at the current vertical exaggeration (so on a
 * Montreal-sized map, a few hundred metres).
 */
export function carveScale(b: Bounds, verticalScale: number) {
  const latM = (b.north - b.south) * 110540
  const lonM = (b.east - b.west) * 111320 * Math.cos((((b.north + b.south) / 2) * Math.PI) / 180)
  const relief = nice((1.5 * Math.max(latM, lonM)) / (200 * verticalScale))
  return {
    size: nice(latM / 8),
    sizeMin: nice(latM / 40),
    sizeMax: nice(latM / 2),
    relief,
    reliefMin: -2 * relief,
    reliefMax: 4 * relief,
    reliefStep: relief / 10,
  }
}

// The Island of Montreal, roughly: traced from its landmass in the Overpass
// fixture. (A bounding box would take in Laval, and Laval has its own rules.)
// prettier-ignore
const MONTREAL: [number, number][] = [
  [45.42, -73.98], [45.47, -73.94], [45.46, -73.9], [45.48, -73.87], [45.52, -73.83],
  [45.51, -73.8], [45.51, -73.76], [45.53, -73.73], [45.55, -73.69], [45.58, -73.66],
  [45.63, -73.62], [45.65, -73.59], [45.67, -73.55], [45.7, -73.51], [45.7, -73.48],
  [45.57, -73.51], [45.47, -73.55], [45.44, -73.59], [45.42, -73.62], [45.42, -73.66],
  [45.43, -73.69], [45.44, -73.73], [45.44, -73.76], [45.45, -73.8], [45.43, -73.83],
  [45.42, -73.87], [45.41, -73.9], [45.4, -73.94],
]

/** Even-odd point-in-polygon test, [lat, lng] rings. */
function inside(ring: [number, number][], lat: number, lng: number) {
  let hit = false
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [ai, bi] = ring[i]
    const [aj, bj] = ring[j]
    if (ai > lat !== aj > lat && lng < ((bj - bi) * (lat - ai)) / (aj - ai) + bi) hit = !hit
  }
  return hit
}

/** What to say after a carving whose highest point is `top` metres. */
export function carveReport(top: number | null, at: { lat: number; lng: number }) {
  if (top == null) return null
  if (inside(MONTREAL, at.lat, at.lng) && top > 232.5) {
    return `Tallest thing in Montreal: ${Math.round(top)} m. The urban plan caps buildings at Mont Royal's 232.5 m.`
  }
  return `Carved. The top is ${Math.round(top)} m above sea level.`
}

export const fmtMetres = (m: number) =>
  Math.abs(m) >= 1000 ? `${+(m / 1000).toFixed(1)} km` : `${Math.round(m)} m`
