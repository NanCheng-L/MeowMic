<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'

const props = defineProps<{
  data?: number[]
}>()

const canvasRef = ref<HTMLCanvasElement | null>(null)
let animationId: number | null = null
let currentData: number[] = []
let hasData = false

// 采样率 48kHz，480 样本 → 32 bands，每 band ≈ 750Hz
const SAMPLE_RATE = 48000
const BANDS = 32
const freqPerBand = SAMPLE_RATE / 2 / BANDS // ≈ 750Hz

const freqLabels = [0, 5000, 10000, 15000, 20000]

// 复用 gradient 对象，避免每帧每 bar 创建新的
const gradients: CanvasGradient[] = []

function ensureGradients(ctx: CanvasRenderingContext2D, barCount: number, height: number) {
  if (gradients.length === barCount) return
  gradients.length = 0
  for (let i = 0; i < barCount; i++) {
    const g = ctx.createLinearGradient(0, height, 0, 0)
    g.addColorStop(0, '#10b981')
    g.addColorStop(0.6, '#f59e0b')
    g.addColorStop(1, '#ef4444')
    gradients.push(g)
  }
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

  if (currentData.length > 0) {
    const data = currentData
    const barCount = data.length
    const gap = 2
    const barWidth = (width - (barCount - 1) * gap) / barCount

    ensureGradients(ctx, barCount, specHeight)

    for (let i = 0; i < barCount; i++) {
      const value = data[i]
      const barHeight = value * specHeight
      const x = i * (barWidth + gap)
      const y = specHeight - barHeight

      if (barHeight <= 0) continue

      ctx.fillStyle = gradients[i]
      ctx.beginPath()
      ctx.roundRect(x, y, barWidth, barHeight, [2, 2, 0, 0])
      ctx.fill()
    }
  }

  drawLabels(ctx, width, labelHeight, specHeight)
}

const draw = () => {
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
  ctx.textAlign = 'center'

  for (const freq of freqLabels) {
    const band = freq / freqPerBand
    const x = (band / BANDS) * width
    if (x < 0 || x > width) continue

    const label = freq >= 1000 ? `${freq / 1000}k` : `${freq}`
    if (freq === 0) ctx.textAlign = 'left'
    else if (freq === 20000) ctx.textAlign = 'right'
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
    gradients.length = 0
  }
}

watch(() => props.data, (newData) => {
  if (newData && newData.length > 0) {
    currentData = newData
    hasData = true
    scheduleDraw()
  } else {
    hasData = false
    currentData = []
    stopDraw()
  }
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
