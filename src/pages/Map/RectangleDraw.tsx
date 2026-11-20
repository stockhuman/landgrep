import { useEffect, useRef, useState } from "react"
import { Rectangle, useMap, useMapEvents } from "react-leaflet"
import { LatLng } from "leaflet"
import { useStore } from "../../data/store"

interface Props {
  drawMode: boolean
  onDrawEnd: () => void
}

type Corner = [[number, number], [number, number]]

export default function RectangleDraw({ drawMode, onDrawEnd }: Props) {
  const map = useMap()
  const { bounds, setBounds } = useStore()
  const startRef = useRef<LatLng | null>(null)
  const [drag, setDrag] = useState<Corner | null>(null)

  // (ToolCursor in ./index owns dragging and the cursor)
  useEffect(() => {
    if (!drawMode) {
      startRef.current = null
      setDrag(null)
    }
  }, [drawMode])

  function squareConstrain(start: LatLng, end: LatLng): LatLng {
    // Work in pixel space so the square looks square on screen.
    const sp = map.latLngToContainerPoint(start)
    const ep = map.latLngToContainerPoint(end)
    const dx = ep.x - sp.x
    const dy = ep.y - sp.y
    const size = Math.max(Math.abs(dx), Math.abs(dy))
    const cx = sp.x + Math.sign(dx) * size
    const cy = sp.y + Math.sign(dy) * size
    return map.containerPointToLatLng([cx, cy])
  }

  useMapEvents({
    mousedown(e) {
      if (!drawMode) return
      startRef.current = e.latlng
    },
    mousemove(e) {
      if (!drawMode || !startRef.current) return
      const s = startRef.current
      const end = e.originalEvent.shiftKey ? squareConstrain(s, e.latlng) : e.latlng
      setDrag([
        [Math.min(s.lat, end.lat), Math.min(s.lng, end.lng)],
        [Math.max(s.lat, end.lat), Math.max(s.lng, end.lng)],
      ])
    },
    mouseup(e) {
      if (!drawMode || !startRef.current) return
      const s = startRef.current
      const end = e.originalEvent.shiftKey ? squareConstrain(s, e.latlng) : e.latlng
      const north = Math.max(s.lat, end.lat)
      const south = Math.min(s.lat, end.lat)
      const east  = Math.max(s.lng, end.lng)
      const west  = Math.min(s.lng, end.lng)
      if (Math.abs(north - south) > 0.001 && Math.abs(east - west) > 0.001) {
        setBounds({ north, south, east, west })
      }
      startRef.current = null
      setDrag(null)
      onDrawEnd()
    },
  })

  const display: Corner | null =
    drag ??
    (bounds ? [[bounds.south, bounds.west], [bounds.north, bounds.east]] : null)

  if (!display) return null
  return (
    <Rectangle
      bounds={display}
      pathOptions={{
        color: "#4a7fd4",
        weight: 2,
        fillOpacity: drag ? 0.1 : 0.15,
        dashArray: drag ? "6 4" : undefined,
      }}
    />
  )
}
