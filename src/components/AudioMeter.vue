<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'

const props = defineProps<{
  label: string
  level: number
  color?: string
}>()

const canvasRef = ref<HTMLCanvasElement | null>(null)
const segments = 32

const activeSegments = computed(() => {
  if (props.level <= -100) return 0
  const normalized = Math.max(0, Math.min(1, (props.level + 60) / 60))
  return Math.round(normalized * segments)
})

const displayLevel = computed(() => {
  if (props.level <= -100) return '-∞'
  return `${props.level.toFixed(1)} dB`
})

function getColors() {
  const style = getComputedStyle(document.documentElement)
  return {
    border: style.getPropertyValue('--border').trim() || '#1e1e1e',
    accent: style.getPropertyValue('--accent').trim() || '#10b981',
    warning: style.getPropertyValue('--warning').trim() || '#f59e0b',
    danger: style.getPropertyValue('--danger').trim() || '#ef4444',
  }
}

function drawMeter() {
  const canvas = canvasRef.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return

  const dpr = window.devicePixelRatio || 1
  const w = canvas.clientWidth
  const h = canvas.clientHeight || 8
  canvas.width = w * dpr
  canvas.height = h * dpr

  ctx.scale(dpr, dpr)

  const colors = getColors()
  const gap = 2
  const segW = (w - (segments - 1) * gap) / segments
  const segH = h
  const active = activeSegments.value

  for (let i = 0; i < segments; i++) {
    const x = i * (segW + gap)
    if (i < active) {
      if (i < 21) ctx.fillStyle = colors.accent
      else if (i < 27) ctx.fillStyle = colors.warning
      else ctx.fillStyle = colors.danger
    } else {
      ctx.fillStyle = colors.border
    }
    ctx.beginPath()
    ctx.roundRect(x, 0, segW, segH, 1)
    ctx.fill()
  }
}

let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  const canvas = canvasRef.value
  if (canvas) {
    resizeObserver = new ResizeObserver(drawMeter)
    resizeObserver.observe(canvas)
  }
  drawMeter()
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
})

watch(activeSegments, drawMeter)
</script>

<template>
  <div class="audio-meter">
    <div class="meter-header">
      <span class="label">{{ label }}</span>
      <span class="level-value">{{ displayLevel }}</span>
    </div>
    <div class="meter-bar">
      <canvas ref="canvasRef"></canvas>
    </div>
  </div>
</template>

<style scoped>
.audio-meter {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1;
}

.meter-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.label {
  font-size: 11px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.level-value {
  font-size: 12px;
  font-family: 'DM Mono', monospace;
  color: var(--text-primary);
}

.meter-bar {
  height: 8px;
  width: 100%;
}

canvas {
  display: block;
  width: 100%;
  height: 100%;
}
</style>
