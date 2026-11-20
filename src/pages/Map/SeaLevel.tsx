import { useRef, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { useStore, PREVIEW_RES, type TerrainUpdate } from "../../data/store"
import { seaReport } from "../../data/sea"
import styles from "./Map.module.css"

/**
 * Sea-level slider for the terrain preview. The backend rebuilds the
 * coastline for each level (water mask, flattened terrain, landmasses), so
 * new islands can be selected and exported like any other. Calls are
 * serialized: while one is in flight, only the latest slider position is
 * sent next.
 */
export default function SeaLevel() {
  const { heightmap, seaLevel, setSeaLevel, fetchedStats, seaMarks, applyUpdate } = useStore()
  const inFlight = useRef(false)
  const wanted = useRef(seaLevel)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  if (!heightmap || !fetchedStats) return null
  const top = Math.max(1, Math.ceil(fetchedStats.maxElev))

  async function send() {
    if (inFlight.current) return
    inFlight.current = true
    setBusy(true)
    setError(null)
    try {
      for (;;) {
        const level = wanted.current
        applyUpdate(
          await invoke<TerrainUpdate>("set_sea_level", {
            level: level > 0 ? level : null,
            previewRes: PREVIEW_RES,
          }),
        )
        if (wanted.current === level) break
      }
    } catch (e) {
      setError(String(e))
    } finally {
      inFlight.current = false
      setBusy(false)
    }
  }

  function go(level: number) {
    const m = Math.min(Math.max(Math.round(level), 0), top)
    setSeaLevel(m)
    wanted.current = m
    void send()
  }

  return (
    <div className={styles.sea}>
      <div className={styles.seaHeader}>
        <span className={styles.seaTitle}>Sea level</span>
        <span className={styles.seaValue}>{seaLevel > 0 ? `+${seaLevel} m` : "today"}</span>
      </div>
      <input
        type='range'
        className={styles.seaRange}
        min={0}
        max={top}
        step={1}
        value={seaLevel}
        onChange={e => go(parseFloat(e.target.value))}
        aria-label='Sea level rise in metres'
      />
      <div className={styles.seaMarks}>
        {seaLevel > 0 && (
          <button className={styles.seaMark} onClick={() => go(0)} title='Put the coastline back'>
            Today
          </button>
        )}
        {seaMarks.map(m => (
          <button
            key={m.label}
            className={`${styles.seaMark} ${seaLevel === m.level ? styles.seaMarkActive : ""}`}
            onClick={() => go(m.level)}
            title={m.title}
          >
            {m.label} · +{m.level} m
          </button>
        ))}
      </div>
      <div className={styles.seaReport}>
        {error ?? seaReport(seaLevel, heightmap, fetchedStats.landKm2, seaMarks)}
        {busy && " …"}
      </div>
    </div>
  )
}
