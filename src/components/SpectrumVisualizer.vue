<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'

const props = defineProps<{
  data?: number[]
  dataOut?: number[]
}>()

const canvasRef = ref<HTMLCanvasElement | null>(null)
let animationId: number | null = null
let hasData = false

const SAMPLE_RATE = 48000
const BANDS = 64
const NYQUIST = SAMPLE_RATE / 2 // 24000
const FREQ_MIN = 20
const FREQ_MAX = NYQUIST

// 对数频率映射：32个band的中心频率
const logBandFreqs: number[] = []
for (let i = 0; i < BANDS; i++) {
  const t = i / (BANDS - 1)
  logBandFreqs.push(FREQ_MIN * Math.pow(FREQ_MAX / FREQ_MIN, t))
}

// 标签频率
const freqLabels = [50, 200, 500, 1000, 2000, 5000, 10000, 20000]

let displayIn: Float32Array = new Float32Array(BANDS)
let displayOut: Float32Array = new Float32Array(BANDS)
let targetIn: Float32Array = new Float32Array(BANDS)
let targetOut: Float32Array = new Float32Array(BANDS)

const RISE_SPEED = 0.22
const FALL_SPEED = 0.10

let cachedGradientH = 0
let greenGradient: CanvasGradient | null = null

function ensureGradient(ctx: CanvasRenderingContext2D, height: number) {
  if (greenGradient && cachedGradientH === height) return
  cachedGradientH = height
  greenGradient = ctx.createLinearGradient(0, height, 0, 0)
  greenGradient.addColorStop(0, '#10b981')
  greenGradient.addColorStop(0.6, '#f59e0b')
  greenGradient.addColorStop(1, '#ef4444')
}

// 对数频率 → x 坐标
function freqToX(freq: number, width: number): number {
  if (freq <= FREQ_MIN) return 0
  if (freq >= FREQ_MAX) return width
  const t = Math.log(freq / FREQ_MIN) / Math.log(FREQ_MAX / FREQ_MIN)
  return t * width
}

const drawFrame = () => {
  const canvas = canvasRef.value
  if (!canvas) return

  const ctx = canvas.getContext('2d')
  if (!ctx) return

  const width = canvas.width
  const height = canvas.height
  const labelHeight = 20
  const specHeight = height - labelHeight

  ctx.clearRect(0, 0, width, height)
  ensureGradient(ctx, specHeight)

  // 对数分布：每个band的宽度由频率范围决定
  for (let i = 0; i < BANDS; i++) {
    const freqLo = i === 0 ? FREQ_MIN : logBandFreqs[i - 1]
    const freqHi = i === BANDS - 1 ? FREQ_MAX : logBandFreqs[i]
    const xLo = freqToX(freqLo, width)
    const xHi = freqToX(freqHi, width)
    const x = xLo
    const barWidth = Math.max(xHi - xLo - 1, 1)

    const val = displayIn[i]
    if (val >= 0.005) {
      const barHeight = val * specHeight
      const y = specHeight - barHeight
      ctx.fillStyle = 'rgba(148, 163, 184, 0.55)'
      ctx.beginPath()
      ctx.roundRect(x, y, barWidth, barHeight, [1, 1, 0, 0])
      ctx.fill()
    }

    const valOut = displayOut[i]
    if (valOut >= 0.005) {
      const barHeight = valOut * specHeight
      const y = specHeight - barHeight
      ctx.fillStyle = greenGradient || '#10b981'
      ctx.beginPath()
      ctx.roundRect(x, y, barWidth, barHeight, [1, 1, 0, 0])
      ctx.fill()
    }
  }

  drawLabels(ctx, width, labelHeight, specHeight)
}

const draw = () => {
  for (let i = 0; i < BANDS; i++) {
    const speedIn = targetIn[i] > displayIn[i] ? RISE_SPEED : FALL_SPEED
    const diffIn = targetIn[i] - displayIn[i]
    if (Math.abs(diffIn) < 0.002) {
      displayIn[i] = targetIn[i]
    } else {
      displayIn[i] += diffIn * speedIn
    }

    const speedOut = targetOut[i] > displayOut[i] ? RISE_SPEED : FALL_SPEED
    const diffOut = targetOut[i] - displayOut[i]
    if (Math.abs(diffOut) < 0.002) {
      displayOut[i] = targetOut[i]
    } else {
      displayOut[i] += diffOut * speedOut
    }
  }

  drawFrame()

  if (hasData) {
    animationId = requestAnimationFrame(draw)
  } else {
    animationId = null
  }
}

const scheduleDraw = () => {
  if (!animationId) {
    animationId = requestAnimationFrame(draw)
  }
}

const stopDraw = () => {
  if (animationId) {
    cancelAnimationFrame(animationId)
    animationId = null
  }
}

function drawLabels(ctx: CanvasRenderingContext2D, width: number, _labelHeight: number, specHeight: number) {
  ctx.fillStyle = '#64748b'
  ctx.font = '10px "DM Mono", monospace'

  for (const freq of freqLabels) {
    const x = freqToX(freq, width)
    if (x < 0 || x > width) continue

    const label = freq >= 1000 ? `${freq / 1000}k` : `${freq}`
    if (freq <= FREQ_MIN + 10) ctx.textAlign = 'left'
    else if (freq >= FREQ_MAX - 100) ctx.textAlign = 'right'
    else ctx.textAlign = 'center'
    ctx.fillText(label, x, specHeight + 14)

    ctx.strokeStyle = '#334155'
    ctx.lineWidth = 1
    ctx.beginPath()
    ctx.moveTo(x, specHeight)
    ctx.lineTo(x, specHeight + 4)
    ctx.stroke()
  }
}

const resize = () => {
  const canvas = canvasRef.value
  if (!canvas) return
  const parent = canvas.parentElement
  if (parent) {
    canvas.width = parent.clientWidth
    canvas.height = parent.clientHeight
    cachedGradientH = 0
    greenGradient = null
  }
}

watch([() => props.data, () => props.dataOut], ([newData, newDataOut]) => {
  if (newData && newData.length > 0) {
    for (let i = 0; i < BANDS; i++) {
      targetIn[i] = i < newData.length ? newData[i] : 0
    }
  } else {
    targetIn.fill(0)
  }

  if (newDataOut && newDataOut.length > 0) {
    for (let i = 0; i < BANDS; i++) {
      targetOut[i] = i < newDataOut.length ? newDataOut[i] : 0
    }
  } else {
    targetOut.fill(0)
  }

  hasData = true
  scheduleDraw()
})

onMounted(() => {
  resize()
  window.addEventListener('resize', resize)
  drawFrame()
})

onUnmounted(() => {
  window.removeEventListener('resize', resize)
  stopDraw()
})
</script>

<template>
  <div class="spectrum-container">
    <canvas ref="canvasRef"></canvas>
  </div>
</template>

<style scoped>
.spectrum-container {
  width: 100%;
  height: 140px;
  background: var(--surface);
  border-radius: 8px;
  overflow: hidden;
}

canvas {
  width: 100%;
  height: 100%;
}
</style>
