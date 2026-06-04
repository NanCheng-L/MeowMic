<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  enabled: boolean
  strength: number
}>()

const emit = defineEmits<{
  'update:enabled': [value: boolean]
  'update:strength': [value: number]
}>()

const sliderColor = computed(() => {
  const s = props.strength
  if (s < 0.33) return '#10b981'
  if (s < 0.66) return '#06b6d4'
  return '#3b82f6'
})

const strengthLabel = computed(() => {
  const s = props.strength
  if (s < 0.33) return '轻度'
  if (s < 0.66) return '中度'
  return '强力'
})

const strengthPercent = computed(() => Math.round(props.strength * 100))
</script>

<template>
  <div class="denoise-control">
    <div class="control-header">
      <span class="label">降噪</span>
      <button
        class="toggle"
        :class="{ active: enabled }"
        @click="emit('update:enabled', !enabled)"
      >
        <span class="toggle-knob"></span>
      </button>
    </div>

    <div class="slider-section">
      <div class="slider-header">
        <span class="strength-label">{{ strengthLabel }}</span>
        <span class="strength-value">{{ strengthPercent }}%</span>
      </div>
      <div class="slider-track">
        <input
          type="range"
          min="0"
          max="100"
          :value="strengthPercent"
          @input="emit('update:strength', Number(($event.target as HTMLInputElement).value) / 100)"
          :disabled="!enabled"
          class="slider"
          :style="{ '--slider-color': sliderColor }"
        />
      </div>
      <div class="slider-labels">
        <span>轻度</span>
        <span>中度</span>
        <span>强力</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.denoise-control {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.control-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.label {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.toggle {
  width: 48px;
  height: 26px;
  border-radius: 13px;
  border: none;
  background: var(--border);
  cursor: pointer;
  position: relative;
  transition: background 0.3s;
  padding: 0;
}

.toggle.active {
  background: var(--accent);
}

.toggle-knob {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: white;
  transition: transform 0.3s;
}

.toggle.active .toggle-knob {
  transform: translateX(22px);
}

.slider-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.slider-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.strength-label {
  font-size: 12px;
  color: var(--text-muted);
}

.strength-value {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.slider-track {
  position: relative;
}

.slider {
  width: 100%;
  height: 6px;
  border-radius: 3px;
  background: var(--border);
  outline: none;
  appearance: none;
  cursor: pointer;
}

.slider::-webkit-slider-thumb {
  appearance: none;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--slider-color);
  cursor: pointer;
  box-shadow: 0 0 8px var(--slider-color);
  transition: transform 0.2s;
}

.slider::-webkit-slider-thumb:hover {
  transform: scale(1.1);
}

.slider:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.slider-labels {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text-muted);
}
</style>
