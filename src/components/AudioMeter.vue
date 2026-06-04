<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  label: string
  level: number
  color?: string
}>()

const segments = 32

const activeSegments = computed(() => {
  if (props.level <= -100) return 0
  const normalized = Math.max(0, Math.min(1, (props.level + 60) / 60))
  return Math.round(normalized * segments)
})

const segmentColor = (index: number) => {
  if (index < 21) return 'var(--accent)'
  if (index < 27) return 'var(--warning)'
  return 'var(--danger)'
}

const displayLevel = computed(() => {
  if (props.level <= -100) return '-∞'
  return `${props.level.toFixed(1)} dB`
})
</script>

<template>
  <div class="audio-meter">
    <div class="meter-header">
      <span class="label">{{ label }}</span>
      <span class="level-value">{{ displayLevel }}</span>
    </div>
    <div class="meter-bar">
      <div
        v-for="i in segments"
        :key="i"
        class="segment"
        :class="{ active: i <= activeSegments }"
        :style="{
          '--segment-color': segmentColor(i - 1),
          animationDelay: `${i * 10}ms`
        }"
      ></div>
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
  display: flex;
  gap: 2px;
  height: 8px;
}

.segment {
  flex: 1;
  background: var(--border);
  border-radius: 1px;
  transition: all 50ms ease-out;
}

.segment.active {
  background: var(--segment-color);
  box-shadow: 0 0 6px var(--segment-color);
}
</style>
