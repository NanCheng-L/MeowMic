<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

export interface Preset {
  key: string
  strength: number
}

const PRESETS: Preset[] = [
  { key: 'quiet', strength: 0.3 },
  { key: 'standard', strength: 0.5 },
  { key: 'noisy', strength: 0.85 },
  { key: 'streaming', strength: 0.7 },
]

const props = defineProps<{
  enabled: boolean
  strength: number
  model: string
  availableModels: string[]
  preset: string
  loading: boolean
  monitorEnabled: boolean
  monitorDevices: string[]
  monitorDevice: string
}>()

const emit = defineEmits<{
  'update:enabled': [value: boolean]
  'update:strength': [value: number]
  'update:model': [value: string]
  'update:preset': [value: string]
  'update:monitorEnabled': [value: boolean]
  'update:monitorDevice': [value: string]
}>()

// 当前预设是否匹配某个内置预设
const matchedPreset = computed(() => {
  const match = PRESETS.find(p => Math.abs(p.strength - props.strength) < 0.01)
  return match?.key ?? 'custom'
})

const activePreset = computed(() => {
  if (props.preset !== 'custom') return props.preset
  return matchedPreset.value
})

const handlePresetSelect = (key: string) => {
  const p = PRESETS.find(p => p.key === key)
  if (!p) return
  emit('update:preset', key)
  emit('update:strength', p.strength)
}

const sliderColor = computed(() => {
  const s = props.strength
  if (s < 0.33) return '#10b981'
  if (s < 0.66) return '#06b6d4'
  return '#3b82f6'
})

const strengthLabel = computed(() => {
  const s = props.strength
  if (s < 0.33) return t('denoise.light')
  if (s < 0.66) return t('denoise.medium')
  return t('denoise.strong')
})

const strengthPercent = computed(() => Math.round(props.strength * 100))

// 拖滑块时自动切到自定义
const handleSliderInput = (e: Event) => {
  const val = Number((e.target as HTMLInputElement).value) / 100
  emit('update:strength', val)
  // 如果当前值和某个预设不匹配，切到自定义
  const match = PRESETS.find(p => Math.abs(p.strength - val) < 0.01)
  if (!match) {
    emit('update:preset', 'custom')
  } else {
    emit('update:preset', match.key)
  }
}
</script>

<template>
  <div class="denoise-control">
    <div class="control-header">
      <span class="label">{{ t('denoise.label') }}</span>
      <div class="header-right">
        <select
          class="model-select"
          :value="model"
          @change="emit('update:model', ($event.target as HTMLSelectElement).value)"
          :disabled="loading"
        >
          <option v-for="m in availableModels" :key="m" :value="m">{{ m }}</option>
        </select>
        <span v-if="loading" class="loading-hint">{{ t('denoise.loading') }}</span>
        <button
          class="toggle"
          :class="{ active: enabled }"
          @click="emit('update:enabled', !enabled)"
        >
          <span class="toggle-knob"></span>
        </button>
      </div>
    </div>

    <div class="monitor-row">
      <div class="monitor-left">
        <svg class="monitor-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M3 18v-6a9 9 0 0 1 18 0v6"/>
          <path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/>
        </svg>
        <span class="monitor-label">{{ t('denoise.monitor') }}</span>
      </div>
      <button
        class="toggle"
        :class="{ active: monitorEnabled }"
        @click="emit('update:monitorEnabled', !monitorEnabled)"
      >
        <span class="toggle-knob"></span>
      </button>
    </div>
    <p class="monitor-desc">{{ t('denoise.monitorDesc') }}</p>

    <div v-if="monitorEnabled" class="monitor-device-section">
      <label class="monitor-device-label">{{ t('device.selectMonitor') }}</label>
      <div class="select-wrapper">
        <select
          :value="monitorDevice"
          @change="emit('update:monitorDevice', ($event.target as HTMLSelectElement).value)"
        >
          <option value="" disabled>{{ t('device.selectMonitor') }}</option>
          <option v-for="device in monitorDevices" :key="device" :value="device">
            {{ device }}
          </option>
        </select>
        <span class="select-arrow"></span>
      </div>
    </div>

    <p v-if="enabled && model" class="model-desc">{{ t(`denoise.desc.${model}`) }}</p>

    <div class="preset-section" v-if="enabled">
      <span class="preset-label">{{ t('denoise.preset') }}</span>
      <div class="preset-group">
        <button
          v-for="p in PRESETS"
          :key="p.key"
          class="preset-btn"
          :class="{ active: activePreset === p.key }"
          @click="handlePresetSelect(p.key)"
        >
          {{ t(`denoise.presets.${p.key}`) }}
        </button>
      </div>
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
          @input="handleSliderInput"
          :disabled="!enabled"
          class="slider"
          :style="{ '--slider-color': sliderColor }"
        />
      </div>
      <div class="slider-labels">
        <span>{{ t('denoise.light') }}</span>
        <span>{{ t('denoise.medium') }}</span>
        <span>{{ t('denoise.strong') }}</span>
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

.header-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.model-select {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 4px 8px;
  font-size: 12px;
  color: var(--text-primary);
  cursor: pointer;
  outline: none;
}

.model-select:focus {
  border-color: var(--accent);
}

.model-select:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.loading-hint {
  font-size: 11px;
  color: var(--accent);
  animation: pulse 1.5s ease-in-out infinite;
}

.model-desc {
  margin: 0;
  font-size: 11px;
  color: var(--text-muted);
  line-height: 1.4;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
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

.preset-section {
  display: flex;
  align-items: center;
  gap: 8px;
}

.preset-label {
  font-size: 12px;
  color: var(--text-muted);
  flex-shrink: 0;
}

.preset-group {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.preset-btn {
  padding: 4px 10px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg);
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

.monitor-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.monitor-left {
  display: flex;
  align-items: center;
  gap: 6px;
}

.monitor-icon {
  color: var(--text-muted);
}

.monitor-label {
  font-size: 13px;
  color: var(--text-primary);
}

.monitor-desc {
  margin: 0;
  font-size: 11px;
  color: var(--text-muted);
  line-height: 1.4;
}

.monitor-device-section {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.monitor-device-label {
  font-size: 12px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.select-wrapper {
  position: relative;
}

.select-wrapper select {
  width: 100%;
  padding: 10px 32px 10px 12px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text-primary);
  font-size: 13px;
  appearance: none;
  cursor: pointer;
  transition: border-color 0.2s;
}

.select-wrapper select:hover {
  border-color: var(--text-muted);
}

.select-wrapper select:focus {
  outline: none;
  border-color: var(--accent);
}

.select-arrow {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%);
  width: 0;
  height: 0;
  border-left: 5px solid transparent;
  border-right: 5px solid transparent;
  border-top: 5px solid var(--text-muted);
  pointer-events: none;
}
</style>
