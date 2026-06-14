<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { emit, listen } from '@tauri-apps/api/event'
import { useTheme } from '../composables/useTheme'
import { setLocale } from '../locales'

const { t } = useI18n()
const { theme } = useTheme()

// 主题感知颜色
const getThemeColors = () => {
  const isDark = theme.value === 'dark'
  return {
    gridLine: isDark ? 'rgba(255,255,255,0.1)' : 'rgba(0,0,0,0.12)',
    gridLineZero: isDark ? 'rgba(255,255,255,0.2)' : 'rgba(0,0,0,0.2)',
    textLabel: isDark ? 'rgba(255,255,255,0.5)' : 'rgba(0,0,0,0.6)',
    curveStroke: isDark ? 'rgba(255,255,255,0.7)' : 'rgba(0,0,0,0.6)',
    curveFillTop: isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.05)',
    curveFillBottom: 'transparent',
    dotBorder: isDark ? 'rgba(255,255,255,0.3)' : 'rgba(255,255,255,0.8)',
    canvasBg: isDark ? 'rgba(0,0,0,0.2)' : 'rgba(0,0,0,0.03)',
  }
}

const enabled = ref(false)
const bands = ref<number[]>(new Array(10).fill(0))
const activePreset = ref('flat')
const frequencies = ref<number[]>([20, 60, 120, 250, 500, 1000, 2000, 4000, 8000, 16000])
const presets = ref<[string, number[]][]>([])

const PRESET_KEYS = ['flat', 'clear', 'warm', 'broadcast', 'bass-boost', 'treble-boost', 'podcast'] as const

// Canvas 相关
const canvasRef = ref<HTMLCanvasElement | null>(null)
const containerRef = ref<HTMLDivElement | null>(null)
let ctx: CanvasRenderingContext2D | null = null
let canvasW = 0
let canvasH = 0
let draggingBand = -1
let hoverBand = -1

// 悬停提示框
const hoverVisible = ref(false)
const hoverBandIdx = ref(0)
const hoverX = ref(0)
const hoverY = ref(0)

// 弹窗相关
const popupVisible = ref(false)
const popupBand = ref(0)
const popupX = ref(0)
const popupY = ref(0)

// 频率响应计算参数
const SAMPLE_RATE = 48000
const FREQ_MIN = 20
const FREQ_MAX = 20000
const DB_MIN = -12
const DB_MAX = 12
const NUM_POINTS = 200

// 频段颜色
const BAND_COLORS = [
  '#a78bfa', '#f472b6', '#fb7185', '#f97316',
  '#eab308', '#84cc16', '#22c55e', '#14b8a6',
  '#06b6d4', '#60a5fa',
]

const loadEqConfig = async () => {
  try {
    const config = await invoke<{ enabled: boolean; bands: number[] }>('get_eq_config')
    enabled.value = config.enabled
    bands.value = [...config.bands]
  } catch (e) {
    console.error('Failed to load EQ config:', e)
  }
}

const loadPresets = async () => {
  try {
    const [freqs, presetList] = await Promise.all([
      invoke<number[]>('get_eq_frequencies'),
      invoke<[string, number[]][]>('get_eq_presets'),
    ])
    frequencies.value = freqs
    presets.value = presetList
  } catch (e) {
    console.error('Failed to load EQ presets:', e)
  }
}

const applyPreset = (name: string) => {
  const preset = presets.value.find(([k]) => k === name)
  if (preset) {
    bands.value = [...preset[1]]
    activePreset.value = name
    syncToBackend()
    draw()
  }
}

const handleToggle = async () => {
  enabled.value = !enabled.value
  syncToBackend()
  await emit('eq-changed', { enabled: enabled.value })
  draw()
}

let syncTimer: ReturnType<typeof setTimeout> | null = null
const syncToBackend = () => {
  if (syncTimer) clearTimeout(syncTimer)
  syncTimer = setTimeout(async () => {
    try {
      await invoke('update_eq_config', {
        enabled: enabled.value,
        bands: bands.value,
      })
    } catch (e) {
      console.error('Failed to update EQ config:', e)
    }
  }, 30)
}

const handleReset = () => {
  bands.value = new Array(10).fill(0)
  activePreset.value = 'flat'
  syncToBackend()
  draw()
}

// ============ 频率响应计算 ============

function biquadMagnitude(b0: number, b1: number, b2: number, a1: number, a2: number, freq: number, sr: number): number {
  const w = 2.0 * Math.PI * freq / sr
  const cosW = Math.cos(w)
  const cos2W = Math.cos(2 * w)
  const sinW = Math.sin(w)
  const sin2W = Math.sin(2 * w)

  const numReal = b0 + b1 * cosW + b2 * cos2W
  const numImag = -(b1 * sinW + b2 * sin2W)
  const denReal = 1.0 + a1 * cosW + a2 * cos2W
  const denImag = -(a1 * sinW + a2 * sin2W)

  const numMag = Math.sqrt(numReal * numReal + numImag * numImag)
  const denMag = Math.sqrt(denReal * denReal + denImag * denImag)

  if (denMag < 1e-10) return 0
  return numMag / denMag
}

function peakingCoeffs(gainDb: number, freq: number, sr: number, q: number) {
  if (Math.abs(gainDb) < 0.01) {
    return { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 }
  }
  const A = Math.pow(10, gainDb / 40)
  const w0 = 2 * Math.PI * freq / sr
  const sinW0 = Math.sin(w0)
  const alpha = sinW0 / (2 * q)

  const b0 = 1 + alpha * A
  const b1 = -2 * Math.cos(w0)
  const b2 = 1 - alpha * A
  const a0 = 1 + alpha / A
  const a1 = -2 * Math.cos(w0)
  const a2 = 1 - alpha / A

  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
}

function computeResponse(): Float32Array {
  const response = new Float32Array(NUM_POINTS)
  const q = 1.414

  for (let p = 0; p < NUM_POINTS; p++) {
    const logF = Math.log10(FREQ_MIN) + (Math.log10(FREQ_MAX) - Math.log10(FREQ_MIN)) * p / (NUM_POINTS - 1)
    const freq = Math.pow(10, logF)

    let totalMag = 0
    for (let i = 0; i < frequencies.value.length; i++) {
      const { b0, b1, b2, a1, a2 } = peakingCoeffs(bands.value[i], frequencies.value[i], SAMPLE_RATE, q)
      const mag = biquadMagnitude(b0, b1, b2, a1, a2, freq, SAMPLE_RATE)
      totalMag += 20 * Math.log10(Math.max(mag, 1e-10))
    }

    response[p] = Math.max(DB_MIN, Math.min(DB_MAX, totalMag))
  }
  return response
}

// ============ Canvas 绘制 ============

const MARGIN = { top: 24, right: 24, bottom: 40, left: 48 }

function freqToX(freq: number): number {
  const logF = Math.log10(freq)
  const logMin = Math.log10(FREQ_MIN)
  const logMax = Math.log10(FREQ_MAX)
  return MARGIN.left + (logF - logMin) / (logMax - logMin) * (canvasW - MARGIN.left - MARGIN.right)
}

function xToFreq(x: number): number {
  const logMin = Math.log10(FREQ_MIN)
  const logMax = Math.log10(FREQ_MAX)
  const ratio = (x - MARGIN.left) / (canvasW - MARGIN.left - MARGIN.right)
  const logF = logMin + ratio * (logMax - logMin)
  return Math.pow(10, logF)
}

function dbToY(db: number): number {
  const ratio = (db - DB_MIN) / (DB_MAX - DB_MIN)
  return MARGIN.top + (1 - ratio) * (canvasH - MARGIN.top - MARGIN.bottom)
}

function yToDb(y: number): number {
  const ratio = 1 - (y - MARGIN.top) / (canvasH - MARGIN.top - MARGIN.bottom)
  return Math.max(DB_MIN, Math.min(DB_MAX, DB_MIN + ratio * (DB_MAX - DB_MIN)))
}

function draw() {
  if (!ctx || !canvasRef.value) return
  const canvas = canvasRef.value
  const dpr = window.devicePixelRatio || 1

  canvasW = canvas.clientWidth * dpr
  canvasH = canvas.clientHeight * dpr
  canvas.width = canvasW
  canvas.height = canvasH
  ctx.scale(dpr, dpr)
  canvasW = canvas.clientWidth
  canvasH = canvas.clientHeight

  ctx.clearRect(0, 0, canvasW, canvasH)

  drawGrid()
  drawResponseCurve()
  drawDots()
}

function drawGrid() {
  if (!ctx) return
  const colors = getThemeColors()

  // 水平线 (dB)
  const dbLines = [-12, -6, 0, 6, 12]
  ctx.textAlign = 'right'
  ctx.textBaseline = 'middle'
  ctx.font = '10px "DM Mono", monospace'

  for (const db of dbLines) {
    const y = dbToY(db)
    ctx.beginPath()
    ctx.strokeStyle = db === 0 ? colors.gridLineZero : colors.gridLine
    ctx.lineWidth = db === 0 ? 1 : 0.5
    ctx.moveTo(MARGIN.left, y)
    ctx.lineTo(canvasW - MARGIN.right, y)
    ctx.stroke()

    ctx.fillStyle = colors.textLabel
    ctx.fillText(`${db > 0 ? '+' : ''}${db} dB`, MARGIN.left - 6, y)
  }

  // 垂直线 (频率) - 对数分布
  ctx.textAlign = 'center'
  ctx.textBaseline = 'top'

  // 20-100Hz: 每 10Hz
  const freqs1 = [20, 30, 40, 50, 60, 70, 80, 90, 100]
  // 100Hz-1kHz: 每 100Hz
  const freqs2 = [200, 300, 400, 500, 600, 700, 800, 900, 1000]
  // 1kHz-10kHz: 每 1kHz
  const freqs3 = [2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000]
  // 10kHz-20kHz
  const freqs4 = [20000]

  const allFreqs = [...freqs1, ...freqs2, ...freqs3, ...freqs4]
  // 主刻度（有标签）
  const labelFreqs = new Set([20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000])

  for (const freq of allFreqs) {
    const x = freqToX(freq)
    const isLabel = labelFreqs.has(freq)

    ctx.beginPath()
    ctx.strokeStyle = isLabel ? colors.gridLine : colors.gridLine
    ctx.lineWidth = isLabel ? 0.8 : 0.4
    ctx.moveTo(x, MARGIN.top)
    ctx.lineTo(x, canvasH - MARGIN.bottom)
    ctx.stroke()

    if (isLabel) {
      const label = freq >= 1000 ? `${freq / 1000}kHz` : `${freq}Hz`
      ctx.fillStyle = colors.textLabel
      ctx.fillText(label, x, canvasH - MARGIN.bottom + 6)
    }
  }
}

function drawResponseCurve() {
  if (!ctx) return
  const colors = getThemeColors()
  const response = computeResponse()

  ctx.beginPath()
  ctx.strokeStyle = colors.curveStroke
  ctx.lineWidth = 1.5

  for (let i = 0; i < NUM_POINTS; i++) {
    const logF = Math.log10(FREQ_MIN) + (Math.log10(FREQ_MAX) - Math.log10(FREQ_MIN)) * i / (NUM_POINTS - 1)
    const freq = Math.pow(10, logF)
    const x = freqToX(freq)
    const y = dbToY(response[i])

    if (i === 0) ctx.moveTo(x, y)
    else ctx.lineTo(x, y)
  }
  ctx.stroke()

  // 填充曲线下方
  const lastX = freqToX(FREQ_MAX)
  const firstX = freqToX(FREQ_MIN)
  ctx.lineTo(lastX, dbToY(0))
  ctx.lineTo(firstX, dbToY(0))
  ctx.closePath()
  ctx.fillStyle = colors.curveFillTop
  ctx.fill()
}

function drawDots() {
  if (!ctx) return
  const colors = getThemeColors()

  for (let i = 0; i < frequencies.value.length; i++) {
    const x = freqToX(frequencies.value[i])
    const y = dbToY(bands.value[i])
    const color = BAND_COLORS[i]
    const isHover = hoverBand === i
    const isDrag = draggingBand === i
    const r = isDrag ? 7 : isHover ? 6 : 5

    // 外圈光晕
    ctx.beginPath()
    ctx.arc(x, y, r + 4, 0, Math.PI * 2)
    ctx.fillStyle = color + '30'
    ctx.fill()

    // 实心圆
    ctx.beginPath()
    ctx.arc(x, y, r, 0, Math.PI * 2)
    ctx.fillStyle = color
    ctx.fill()

    // 边框
    ctx.beginPath()
    ctx.arc(x, y, r, 0, Math.PI * 2)
    ctx.strokeStyle = colors.dotBorder
    ctx.lineWidth = 1
    ctx.stroke()
  }
}

// ============ 鼠标交互 ============

function getBandAtPos(mx: number, my: number): number {
  for (let i = 0; i < frequencies.value.length; i++) {
    const x = freqToX(frequencies.value[i])
    const y = dbToY(bands.value[i])
    const dx = mx - x
    const dy = my - y
    if (dx * dx + dy * dy < 200) return i
  }
  return -1
}

function getCanvasPos(e: MouseEvent): { x: number; y: number } {
  if (!canvasRef.value) return { x: 0, y: 0 }
  const rect = canvasRef.value.getBoundingClientRect()
  return { x: e.clientX - rect.left, y: e.clientY - rect.top }
}

const handleMouseDown = (e: MouseEvent) => {
  const { x, y } = getCanvasPos(e)
  const band = getBandAtPos(x, y)
  if (band >= 0) {
    draggingBand = band
    popupVisible.value = false
  }
}

const handleMouseMove = (e: MouseEvent) => {
  const { x, y } = getCanvasPos(e)

  if (draggingBand >= 0) {
    // 垂直拖拽 = 增益
    const newDb = Math.round(yToDb(y) * 2) / 2 // snap to 0.5
    bands.value[draggingBand] = newDb
    // 水平拖拽 = 频率
    const newFreq = xToFreq(x)
    if (newFreq >= 20 && newFreq <= 20000) {
      frequencies.value[draggingBand] = newFreq
    }
    activePreset.value = 'custom'
    syncToBackend()
    // 更新悬停提示框位置
    hoverBandIdx.value = draggingBand
    hoverX.value = e.clientX
    hoverY.value = e.clientY
    hoverVisible.value = true
    draw()
    return
  }

  const band = getBandAtPos(x, y)
  if (band !== hoverBand) {
    hoverBand = band
    if (band >= 0) {
      hoverBandIdx.value = band
      hoverX.value = e.clientX
      hoverY.value = e.clientY
      hoverVisible.value = true
      if (canvasRef.value) {
        canvasRef.value.style.cursor = 'grab'
      }
    } else {
      hoverVisible.value = false
      if (canvasRef.value) {
        canvasRef.value.style.cursor = 'default'
      }
    }
    draw()
  } else if (band >= 0) {
    // 同一个圆点上移动，更新位置
    hoverX.value = e.clientX
    hoverY.value = e.clientY
  }
}

const handleMouseUp = () => {
  if (draggingBand >= 0) {
    if (canvasRef.value) {
      canvasRef.value.style.cursor = 'grab'
    }
    draggingBand = -1
  }
}

const handleMouseLeave = () => {
  hoverVisible.value = false
  hoverBand = -1
  if (draggingBand >= 0) {
    draggingBand = -1
  }
  draw()
}

const handleDoubleClick = (e: MouseEvent) => {
  const { x, y } = getCanvasPos(e)
  const band = getBandAtPos(x, y)
  if (band >= 0) {
    popupBand.value = band
    popupX.value = e.clientX
    popupY.value = e.clientY
    popupVisible.value = true
  }
}

const handlePopupClose = () => {
  popupVisible.value = false
}

const handleGainInput = (val: number) => {
  bands.value[popupBand.value] = Math.round(val * 2) / 2
  syncToBackend()
  draw()
}

const handleFreqInput = (val: number) => {
  const freq = Math.max(20, Math.min(20000, val))
  frequencies.value[popupBand.value] = freq
  syncToBackend()
  draw()
}

// ============ 生命周期 ============

let resizeObserver: ResizeObserver | null = null
let unlisten: (() => void) | null = null

onMounted(async () => {
  // 同步语言设置
  const savedLang = localStorage.getItem('meowmic-lang')
  if (savedLang) setLocale(savedLang)

  // 跨窗口语言同步
  window.addEventListener('storage', handleStorage)

  await Promise.all([loadEqConfig(), loadPresets()])
  await nextTick()

  if (canvasRef.value) {
    ctx = canvasRef.value.getContext('2d')
    draw()
  }

  if (containerRef.value) {
    resizeObserver = new ResizeObserver(() => {
      draw()
    })
    resizeObserver.observe(containerRef.value)
  }

  unlisten = await listen<{ enabled: boolean }>('eq-changed', (event) => {
    enabled.value = event.payload.enabled
    draw()
  })

  // 轮询同步语言变化
  setInterval(() => {
    const lang = localStorage.getItem('meowmic-lang')
    if (lang) setLocale(lang)
  }, 1000)
})

// 主题变化时重绘 Canvas
watch(theme, () => {
  draw()
})

// 跨窗口语言同步
const handleStorage = (e: StorageEvent) => {
  if (e.key === 'meowmic-lang' && e.newValue) {
    setLocale(e.newValue)
  }
}

onUnmounted(() => {
  window.removeEventListener('storage', handleStorage)
  resizeObserver?.disconnect()
  unlisten?.()
})

const formatFreq = (hz: number) => {
  if (hz >= 1000) {
    return `${(hz / 1000).toFixed(1)}kHz`
  }
  return `${hz.toFixed(1)}Hz`
}
</script>

<template>
  <div class="eq-page" @click="handlePopupClose">
    <div class="eq-header">
      <h1>{{ t('eq.title') }}</h1>
      <button class="toggle" :class="{ active: enabled }" @click.stop="handleToggle">
        <span class="toggle-knob"></span>
      </button>
    </div>

    <div class="eq-body">
      <div class="preset-section">
        <span class="preset-label">{{ t('eq.preset') }}</span>
        <div class="preset-group">
          <button
            v-for="key in PRESET_KEYS"
            :key="key"
            class="preset-btn"
            :class="{ active: activePreset === key }"
            @click="applyPreset(key)"
          >
            {{ t(`eq.presets.${key}`) }}
          </button>
        </div>
      </div>

      <div class="canvas-container" ref="containerRef">
        <canvas
          ref="canvasRef"
          class="eq-canvas"
          @mousedown.stop="handleMouseDown"
          @mousemove.stop="handleMouseMove"
          @mouseup.stop="handleMouseUp"
          @mouseleave.stop="handleMouseLeave"
          @dblclick.stop="handleDoubleClick"
        ></canvas>
      </div>

      <!-- 悬停提示框 -->
      <div
        v-if="hoverVisible && !popupVisible"
        class="eq-hover-tooltip"
        :style="{ left: (hoverX + 12) + 'px', top: (hoverY - 40) + 'px' }"
      >
        <span class="hover-dot" :style="{ background: BAND_COLORS[hoverBandIdx] }"></span>
        <span class="hover-db">{{ bands[hoverBandIdx] > 0 ? '+' : '' }}{{ bands[hoverBandIdx].toFixed(1) }} dB</span>
        <span class="hover-freq">{{ formatFreq(frequencies[hoverBandIdx]) }}</span>
      </div>

      <!-- 详情弹窗 -->
      <div
        v-if="popupVisible"
        class="eq-popup"
        :style="{ left: popupX + 'px', top: popupY + 'px' }"
        @click.stop
      >
        <div class="popup-row">
          <span class="popup-dot" :style="{ background: BAND_COLORS[popupBand] }"></span>
          <div class="popup-input-group">
            <input
              type="number"
              class="popup-input"
              :value="bands[popupBand]"
              @change="(e) => handleGainInput(Number((e.target as HTMLInputElement).value))"
              min="-12"
              max="12"
              step="0.5"
            />
            <span class="popup-unit">dB</span>
          </div>
          <div class="popup-input-group">
            <input
              type="number"
              class="popup-input popup-input-freq"
              :value="frequencies[popupBand]"
              @change="(e) => handleFreqInput(Number((e.target as HTMLInputElement).value))"
              min="20"
              max="20000"
              step="1"
            />
            <span class="popup-unit">Hz</span>
          </div>
        </div>
      </div>
    </div>

    <div class="eq-footer">
      <button class="btn-reset" @click="handleReset">{{ t('eq.reset') }}</button>
    </div>
  </div>
</template>

<style>
html, body {
  margin: 0;
  padding: 0;
  overflow: hidden;
  height: 100vh;
  background: var(--bg);
  font-family: 'DM Sans', -apple-system, BlinkMacSystemFont, sans-serif;
}
</style>

<style scoped>
.eq-page {
  height: 100vh;
  background: var(--bg);
  color: var(--text-primary);
  display: flex;
  flex-direction: column;
  user-select: none;
}

.eq-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 14px 24px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.eq-header h1 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
}

.eq-body {
  flex: 1;
  padding: 12px 24px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
}

.preset-section {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}

.preset-label {
  font-size: 13px;
  color: var(--text-muted);
  flex-shrink: 0;
}

.preset-group {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.preset-btn {
  padding: 5px 10px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--text-muted);
  font-size: 11px;
  cursor: pointer;
  transition: all 0.2s;
}

.preset-btn:hover {
  border-color: var(--accent);
  color: var(--text-primary);
}

.preset-btn.active {
  background: var(--accent);
  border-color: var(--accent);
  color: white;
}

.canvas-container {
  flex: 1;
  min-height: 0;
  border-radius: 8px;
  overflow: hidden;
  background: var(--surface);
  border: 1px solid var(--border);
  position: relative;
}

.eq-canvas {
  width: 100%;
  height: 100%;
  display: block;
}

/* 悬停提示框 */
.eq-hover-tooltip {
  position: fixed;
  z-index: 90;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 6px 10px;
  display: flex;
  align-items: center;
  gap: 8px;
  pointer-events: none;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  white-space: nowrap;
}

.hover-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.hover-db {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  font-family: 'DM Mono', monospace;
}

.hover-freq {
  font-size: 12px;
  color: var(--text-muted);
  font-family: 'DM Mono', monospace;
}

/* 弹窗 */
.eq-popup {
  position: fixed;
  z-index: 100;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 12px;
  min-width: 160px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
}

.popup-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border);
}

.popup-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}

.popup-freq {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.popup-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 4px 0;
}

.popup-row label {
  font-size: 12px;
  color: var(--text-muted);
}

.popup-input-group {
  display: flex;
  align-items: center;
  gap: 2px;
}

.popup-input {
  width: 50px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 3px 6px;
  font-size: 12px;
  color: var(--text-primary);
  font-family: 'DM Mono', monospace;
  text-align: right;
  outline: none;
}

.popup-input:focus {
  border-color: var(--accent);
}

.popup-unit {
  font-size: 11px;
  color: var(--text-muted);
}

.popup-value {
  font-size: 12px;
  font-family: 'DM Mono', monospace;
  color: var(--text-primary);
}

/* Toggle */
.toggle {
  width: 44px;
  height: 24px;
  border-radius: 12px;
  border: none;
  background: var(--border);
  cursor: pointer;
  position: relative;
  transition: background 0.3s;
  padding: 0;
  flex-shrink: 0;
}

.toggle.active {
  background: var(--accent);
}

.toggle-knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: white;
  transition: transform 0.3s;
}

.toggle.active .toggle-knob {
  transform: translateX(20px);
}

/* Footer */
.eq-footer {
  display: flex;
  justify-content: flex-end;
  padding: 10px 24px;
  border-top: 1px solid var(--border);
  flex-shrink: 0;
}

.btn-reset {
  padding: 6px 16px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--text-primary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-reset:hover {
  border-color: var(--text-muted);
}
</style>
