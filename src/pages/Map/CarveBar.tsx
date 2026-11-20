import { invoke } from "@tauri-apps/api/core"
import { useStore, PREVIEW_RES, type TerrainUpdate } from "../../data/store"
import { carveScale, fmtMetres } from "../../data/carve"
import styles from "./Map.module.css"

/** Options for the carve tool: what to carve, how big, how high; undo. */
export default function CarveBar({ onDone }: { onDone: () => void }) {
  const {
    fetchedBounds,
    verticalScale,
    carveText,
    setCarveText,
    carveSizeM,
    setCarveSizeM,
    carveReliefM,
    setCarveReliefM,
    carvings,
    setCarvings,
    carveNote,
    setCarveNote,
    applyUpdate,
  } = useStore()
  if (!fetchedBounds) return null
  const scale = carveScale(fetchedBounds, verticalScale)
  const size = carveSizeM ?? scale.size
  const relief = carveReliefM ?? scale.relief

  async function undo() {
    try {
      applyUpdate(await invoke<TerrainUpdate>("undo_carving", { previewRes: PREVIEW_RES }))
      setCarvings(Math.max(0, carvings - 1))
      setCarveNote(null)
    } catch (e) {
      setCarveNote(String(e))
    }
  }

  return (
    <div className={styles.carveBar}>
      <div className={styles.carveHeader}>
        Carve
        <button className={styles.closeBtn} onClick={onDone} title='Done carving'>
          ✕
        </button>
      </div>
      <input
        className={styles.carveText}
        value={carveText}
        maxLength={60}
        onChange={e => setCarveText(e.target.value)}
        placeholder='Your name here'
        aria-label='Text to carve'
      />
      <label className={styles.carveField}>
        <span>Letters {fmtMetres(size)}</span>
        <input
          type='range'
          className={styles.carveRange}
          min={scale.sizeMin}
          max={scale.sizeMax}
          step={scale.sizeMin}
          value={size}
          onChange={e => setCarveSizeM(parseFloat(e.target.value))}
        />
      </label>
      <label className={styles.carveField}>
        <span>
          {relief < 0 ? "Engraved" : "Raised"} {fmtMetres(Math.abs(relief))}
        </span>
        <input
          type='range'
          className={styles.carveRange}
          min={scale.reliefMin}
          max={scale.reliefMax}
          step={scale.reliefStep}
          value={relief}
          onChange={e => setCarveReliefM(parseFloat(e.target.value))}
        />
      </label>
      <span className={styles.carveNote}>{carveNote ?? "Click the map to carve it in."}</span>
      <button className={styles.drawBtn} onClick={undo} disabled={carvings === 0}>
        Undo
      </button>
    </div>
  )
}
