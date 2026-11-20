import { useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { useStore, PREVIEW_RES, type Heightmap, type Bounds } from "../../data/store"
import { seaMarks } from "../../data/sea"
import type { DrawTool } from "./index"
import styles from "./Map.module.css"

interface Props {
  tool: DrawTool
  setTool: (t: DrawTool) => void
}

export default function BottomPanel({ tool, setTool }: Props) {
  const {
    apiKey, bounds, setBounds, setHeightmap, demType, waterMask,
    heightmap, verticalScale, exportRes, baseMm, smoothFactor,
    selectedLandmasses, setSelectedLandmasses, setLandmassOverlay, setFetchedBounds,
    lassoSelection, setLassoSelection, setSeaLevel, setFetchedStats, setSeaMarks,
    setCarvings, setCarveSizeM, setCarveReliefM, setCarveNote,
  } = useStore()
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const canFetch =
    !!apiKey &&
    !!bounds &&
    bounds.north > bounds.south &&
    bounds.east > bounds.west

  async function handleFetch() {
    if (!bounds || !apiKey) return
    setLoading(true)
    setError(null)
    try {
      const result = await invoke<Heightmap>("fetch_elevation", {
        south: bounds.south,
        north: bounds.north,
        west: bounds.west,
        east: bounds.east,
        apiKey,
        demType,
        previewRes: PREVIEW_RES,
        waterMask,
      })
      setHeightmap(result)
      setFetchedBounds(bounds)
      setSelectedLandmasses([])
      setLassoSelection(null)
      setSeaLevel(0)
      setFetchedStats({ maxElev: result.maxElev, landKm2: result.landKm2 })
      setCarvings(0)
      setCarveSizeM(null)
      setCarveReliefM(null)
      setCarveNote(null)
      if (result.landmassCount) {
        setLandmassOverlay(await invoke<string>("landmass_overlay", {}).catch(() => null))
      } else {
        setLandmassOverlay(null)
      }
      setSeaMarks(await seaMarks(bounds, result))
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  async function handleExport(selection: { landmasses?: number[]; lasso?: boolean }) {
    try {
      const path = await invoke<string>("export_stl", {
        verticalScale,
        exportRes,
        baseMm,
        smoothFactor,
        ...selection,
      })
      alert(`Saved to ${path}`)
    } catch (e) {
      alert(`Export failed: ${e}`)
    }
  }

  async function clearSelection() {
    setSelectedLandmasses([])
    setLassoSelection(null)
    setLandmassOverlay(await invoke<string>("landmass_overlay", {}).catch(() => null))
  }

  const fmtArea = (km2: number) =>
    km2 >= 100 ? km2.toFixed(0) : km2 >= 1 ? km2.toFixed(1) : km2.toFixed(2)

  function setCoord(field: keyof Bounds, raw: string) {
    const val = parseFloat(raw)
    if (isNaN(val)) return
    setBounds({ ...(bounds ?? { north: 0, south: 0, east: 0, west: 0 }), [field]: val } as Bounds)
  }

  const coord = (field: keyof Bounds, label: string) => (
    <label className={styles.coordGroup}>
      <span className={styles.coordLabel}>{label}</span>
      <input
        type="number"
        className={styles.coordInput}
        value={bounds?.[field]?.toFixed(4) ?? ""}
        onChange={e => setCoord(field, e.target.value)}
        step={0.001}
        placeholder="—"
      />
    </label>
  )

  return (
    <div className={styles.bottomPanel}>
      <button
        className={`${styles.drawBtn} ${tool === "rect" ? styles.drawBtnActive : ""}`}
        onClick={() => setTool(tool === "rect" ? null : "rect")}
        title="Draw a rectangle on the map"
      >
        {tool === "rect" ? "✕ Cancel" : "✏ Draw"}
      </button>

      <div className={styles.divider} />

      {coord("north", "N")}
      {coord("south", "S")}
      {coord("east", "E")}
      {coord("west", "W")}

      <div className={styles.divider} />

      <button
        className={styles.fetchBtn}
        disabled={!canFetch || loading}
        onClick={handleFetch}
      >
        {loading ? "Fetching…" : "Fetch Terrain"}
      </button>

      {error && <span className={styles.error}>{error}</span>}

      {heightmap && (
        <>
          <div className={styles.divider} />
          <button
            className={`${styles.drawBtn} ${tool === "lasso" ? styles.drawBtnActive : ""}`}
            onClick={() => setTool(tool === "lasso" ? null : "lasso")}
            title="Freehand-select terrain to export — the drawn outline is clipped to land, so it only needs to be rough"
          >
            {tool === "lasso" ? "✕ Cancel" : "◌ Lasso"}
          </button>
          <button
            className={`${styles.drawBtn} ${tool === "carve" ? styles.drawBtnActive : ""}`}
            onClick={() => setTool(tool === "carve" ? null : "carve")}
            title="Carve text into the terrain, letters kilometres tall"
          >
            {tool === "carve" ? "✕ Done" : "✍ Carve"}
          </button>
        </>
      )}

      {lassoSelection ? (
        <>
          <div className={styles.divider} />
          <span className={styles.landmassInfo}>
            Lasso · {fmtArea(lassoSelection.areaKm2)} km²
          </span>
          <button className={styles.fetchBtn} onClick={() => handleExport({ lasso: true })}>
            Export Selection
          </button>
          <button className={styles.drawBtn} onClick={clearSelection} title="Clear selection">
            ✕
          </button>
        </>
      ) : selectedLandmasses.length > 0 ? (
        <>
          <div className={styles.divider} />
          <span className={styles.landmassInfo}>
            {selectedLandmasses.length === 1
              ? "Landmass"
              : `${selectedLandmasses.length} landmasses`}
            {" · "}
            {fmtArea(selectedLandmasses.reduce((sum, lm) => sum + lm.areaKm2, 0))} km²
            {" · gone at +"}
            {Math.max(0, Math.ceil(Math.max(...selectedLandmasses.map(lm => lm.submergedAt))))} m
          </span>
          <button
            className={styles.fetchBtn}
            onClick={() => handleExport({ landmasses: selectedLandmasses.map(lm => lm.id) })}
          >
            Export Landmass
          </button>
          <button className={styles.drawBtn} onClick={clearSelection} title="Clear selection">
            ✕
          </button>
        </>
      ) : (
        heightmap?.landmassCount ? (
          <>
            <div className={styles.divider} />
            <span className={styles.landmassHint}>
              {heightmap.landmassCount} landmass{heightmap.landmassCount === 1 ? "" : "es"} — click one on the map, shift-click to add
            </span>
          </>
        ) : null
      )}
    </div>
  )
}
