<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'

const props = defineProps<{
  data?: number[]
}>()

const canvasRef = ref<HTMLCanvasElement | null>(null)
let animationId: number | null = null
let currentData: number[] = []

// 采样率 48kHz，480 样本 → 32 bands，每 band ≈ 750Hz
const SAMPLE_RATE = 48000
const BANDS = 32
const freqPerBand = SAMPLE_RATE / 2 / BANDS // ≈ 750Hz

// 频率标签位置（Hz）
const freqLabels = [0, 1000, 2000, 5000, 10000, 20000]

const draw = () => {
  const canvas = canvasRef.value
  if (!canvas) return

  const ctx = canvas.getContext('2d')
  if (!ctx) return

  const width = canvas.width
  const height = canvas.height
  const labelHeight = 20 // 底部坐标区高度
  const specHeight = height - labelHeight

  ctx.clearRect(0, 0, width, height)

  if (currentData.length === 0) {
    // 空状态也画坐标
    drawLabels(ctx, width, labelHeight, specHeight)
    animationId = requestAnimationFrame(draw)
    return
  }

  const data = currentData
  const barCount = data.length
  const gap = 2
  const barWidth = (width - (barCount - 1) * gap) / barCount

  data.forEach((value, i) => {
    const barHeight = value * specHeight
    const x = i * (barWidth + gap)
    const y = specHeight - barHeight

    const gradient = ctx.createLinearGradient(x, specHeight, x, y)
    gradient.addColorStop(0, '#10b981')
    gradient.addColorStop(0.6, '#f59e0b')
    gradient.addColorStop(1, '#ef4444')

    ctx.fillStyle = gradient
    ctx.beginPath()
    ctx.roundRect(x, y, barWidth, barHeight, [2, 2, 0, 0])
    ctx.fill()
  })

  drawLabels(ctx, width, labelHeight, specHeight)
  animationId = requestAnimationFrame(draw)
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
    // 0Hz 左对齐，20k 右对齐，其余居中
    if (freq === 0) ctx.textAlign = 'left'
    else if (freq === 20000) ctx.textAlign = 'right'
    else ctx.textAlign = 'center'
    ctx.fillText(label, x, specHeight + 14)

    // 画小刻度线
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
  }
}

watch(() => props.data, (newData) => {
  if (newData && newData.length > 0) {
    currentData = newData
  }
}, { deep: true })

onMounted(() => {
  resize()
  window.addEventListener('resize', resize)
  animationId = requestAnimationFrame(draw)
})

onUnmounted(() => {
  window.removeEventListener('resize', resize)
  if (animationId) {
    cancelAnimationFrame(animationId)
  }
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
