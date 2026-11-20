import { useEffect, useRef, useState } from "react"
import { Polygon, useMap, useMapEvents } from "react-leaflet"
import { invoke } from "@tauri-apps/api/core"
import { useStore, type LassoInfo } from "../../data/store"

interface Props {
  active: boolean
  onDrawEnd: () => void
}

/**
 * Freehand lasso selection: drag to outline a region, and on release the
 * backend rasterizes it onto the terrain grid clipped to land. The outline
 * only needs to be rough — water inside it is dropped — so this doubles as
 * the manual split tool for landmasses fused by causeways or bridges.
 */
export default function LassoDraw({ active, onDrawEnd }: Props) {
  const map = useMap()
  const { setLassoSelection, setSelectedLandmasses, setLandmassOverlay } = useStore()
  const pointsRef = useRef<[number, number][]>([])
  const drawingRef = useRef(false)
  const [points, setPoints] = useState<[number, number][]>([])

  // (ToolCursor in ./index owns dragging and the cursor)
  useEffect(() => {
    if (!active) {
      drawingRef.current = false
      pointsRef.current = []
      setPoints([])
    }
  }, [active])

  useMapEvents({
    mousedown(e) {
      if (!active) return
      drawingRef.current = true
      pointsRef.current = [[e.latlng.lat, e.latlng.lng]]
      setPoints(pointsRef.current)
    },
    mousemove(e) {
      if (!active || !drawingRef.current) return
      const pts = pointsRef.current
      const last = pts[pts.length - 1]
      const lp = map.latLngToContainerPoint({ lat: last[0], lng: last[1] })
      const cp = map.latLngToContainerPoint(e.latlng)
      // thin the freehand stream: mousemove fires every couple of pixels
      if (Math.hypot(cp.x - lp.x, cp.y - lp.y) < 4) return
      pointsRef.current = [...pts, [e.latlng.lat, e.latlng.lng]]
      setPoints(pointsRef.current)
    },
    async mouseup() {
      if (!active || !drawingRef.current) return
      drawingRef.current = false
      const pts = pointsRef.current
      pointsRef.current = []
      setPoints([])
      // defer leaving draw mode so the click Leaflet fires after this
      // mouseup still sees drawMode and can't clear the fresh selection
      setTimeout(onDrawEnd, 0)
      if (pts.length < 3) return
      try {
        const info = await invoke<LassoInfo>("lasso_select", { points: pts })
        setSelectedLandmasses([])
        setLassoSelection(info)
        setLandmassOverlay(await invoke<string>("lasso_overlay"))
      } catch (err) {
        alert(`Lasso selection failed: ${err}`)
      }
    },
  })

  if (points.length < 2) return null
  return (
    <Polygon
      positions={points}
      pathOptions={{
        color: "#ff8c00",
        weight: 2,
        fillOpacity: 0.08,
        dashArray: "6 4",
      }}
    />
  )
}
