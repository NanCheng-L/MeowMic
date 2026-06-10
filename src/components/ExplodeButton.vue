<script setup lang="ts">
import { computed, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  intensity: number
  enabled: boolean
}>()

const emit = defineEmits<{
  'update:intensity': [value: number]
  'update:enabled': [value: boolean]
}>()

const { t } = useI18n()

const intensityPercent = computed(() => Math.round(props.intensity * 100))

const sliderColor = computed(() => {
  const pct = intensityPercent.value
  if (pct < 33) return '#f97316'
  if (pct < 66) return '#ef4444'
  return '#dc2626'
})

const toggle = async () => {
  const next = !props.enabled
  emit('update:enabled', next)
  await invoke('set_explode_mode', { enabled: next, intensity: Math.round(props.intensity * 100) })
}

const handleSliderInput = async (e: Event) => {
  const val = Number((e.target as HTMLInputElement).value)
  emit('update:intensity', val / 100)
  // 如果正在炸麦，实时更新强度
  if (props.enabled) {
    await invoke('set_explode_mode', { enabled: true, intensity: val })
  }
}

onUnmounted(() => {
  if (props.enabled) {
    invoke('set_explode_mode', { enabled: false }).catch(() => {})
  }
})
</script>

<template>
  <div class="explode-section">
    <div class="setting-row">
      <div class="setting-info">
        <span class="setting-label">
          <span class="explode-icon">💥</span>
          {{ t('explode.start') }}
        </span>
      </div>
      <button
        class="toggle"
        :class="{ active: enabled }"
        @click="toggle"
      >
        <span class="toggle-knob"></span>
      </button>
    </div>

    <div class="slider-section">
      <div class="slider-header">
        <span class="slider-label">{{ t('explode.intensity') }}</span>
        <span class="slider-value" :style="{ color: sliderColor }">{{ intensityPercent }}%</span>
      </div>
      <div class="slider-track">
        <input
          type="range"
          min="1"
          max="100"
          :value="intensityPercent"
          @input="handleSliderInput"
          class="slider"
          :style="{ '--slider-color': sliderColor }"
        />
      </div>
      <div class="slider-labels">
        <span>🔊</span>
        <span>📢</span>
        <span>💥</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.explode-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.setting-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.setting-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.setting-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  display: flex;
  align-items: center;
  gap: 6px;
}

.explode-icon {
  font-size: 16px;
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
  background: var(--danger);
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

.slider-label {
  font-size: 12px;
  color: var(--text-secondary);
}

.slider-value {
  font-size: 12px;
  font-weight: 600;
  font-family: 'DM Mono', monospace;
}

.slider-track {
  width: 100%;
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

.slider::-moz-range-thumb {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--slider-color);
  cursor: pointer;
  border: none;
  box-shadow: 0 0 8px var(--slider-color);
}

.slider-labels {
  display: flex;
  justify-content: space-between;
  font-size: 10px;
  color: var(--text-muted);
}
</style>
