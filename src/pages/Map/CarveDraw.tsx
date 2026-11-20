import { useMapEvents } from "react-leaflet"
import { invoke } from "@tauri-apps/api/core"
import { useStore, PREVIEW_RES, type TerrainUpdate } from "../../data/store"
import { carveReport, carveScale } from "../../data/carve"

/**
 * Carve tool: each click on the map carves the current text into the
 * terrain, centred on the click (see CarveBar for the text and sizes).
 */
export default function CarveDraw({ active }: { active: boolean }) {
  useMapEvents({
    async click(e) {
      if (!active) return
      const s = useStore.getState()
      if (!s.fetchedBounds) return
      const scale = carveScale(s.fetchedBounds, s.verticalScale)
      try {
        const update = await invoke<TerrainUpdate>("carve_text", {
          text: s.carveText,
          lat: e.latlng.lat,
          lng: e.latlng.lng,
          letterHeightM: s.carveSizeM ?? scale.size,
          reliefM: s.carveReliefM ?? scale.relief,
          previewRes: PREVIEW_RES,
        })
        s.applyUpdate(update)
        s.setCarvings(s.carvings + 1)
        s.setCarveNote(carveReport(update.carvedTop, e.latlng))
      } catch (err) {
        s.setCarveNote(String(err))
      }
    },
  })
  return null
}
