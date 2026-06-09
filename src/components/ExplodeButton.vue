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
    <div class="explode-header">
      <span class="explode-label">{{ t('explode.label') }}</span>
      <span class="explode-hint">{{ t('explode.hint') }}</span>
    </div>

    <div class="explode-row">
      <button
        class="explode-btn"
        :class="{ active: enabled }"
        @click="toggle"
      >
        <span class="explode-icon">💥</span>
        <span class="explode-text">{{ enabled ? t('explode.stop') : t('explode.start') }}</span>
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

.explode-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.explode-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.explode-hint {
  font-size: 11px;
  color: var(--text-muted);
}

.explode-row {
  display: flex;
  align-items: center;
}

.explode-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: 100%;
  padding: 14px 16px;
  border: 2px solid #ef4444;
  border-radius: 10px;
  background: linear-gradient(135deg, #1a1a2e, #16213e);
  color: #ef4444;
  font-size: 15px;
  font-weight: 700;
  cursor: pointer;
  transition: all 0.2s;
  position: relative;
  overflow: hidden;
}

.explode-btn:hover {
  background: linear-gradient(135deg, #1a1a2e, #1e1024);
  border-color: #f87171;
  box-shadow: 0 0 20px rgba(239, 68, 68, 0.3);
}

.explode-btn.active {
  background: linear-gradient(135deg, #7f1d1d, #991b1b);
  border-color: #fca5a5;
  color: #fca5a5;
  animation: shake 0.1s infinite;
  box-shadow: 0 0 30px rgba(239, 68, 68, 0.5);
}

.explode-icon {
  font-size: 20px;
}

.explode-text {
  letter-spacing: 1px;
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

@keyframes shake {
  0%, 100% { transform: translateX(0); }
  25% { transform: translateX(-3px) rotate(-1deg); }
  50% { transform: translateX(3px) rotate(1deg); }
  75% { transform: translateX(-2px) rotate(-0.5deg); }
}
</style>
