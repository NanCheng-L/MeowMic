<template>
  <div class="explode-control">
    <div class="control-header">
      <span class="label">💥 {{ t('explode.start') }}</span>
      <button
        class="toggle"
        :class="{ active: enabled }"
        @click="toggleEnabled"
      >
        <span class="toggle-knob"></span>
      </button>
    </div>

    <!-- 效果选择 -->
    <div class="effect-chips">
      <button
        v-for="effect in effects"
        :key="effect.value"
        class="effect-chip"
        :class="{ active: selectedEffect === effect.value }"
        @click="selectEffect(effect.value)"
      >
        {{ effect.icon }} {{ t(`explode.effects.${effect.key}`) }}
      </button>
    </div>

    <!-- 强度滑块 -->
    <div class="intensity-controls">
      <div class="intensity-row">
        <span class="intensity-label">{{ t('explode.intensity') }}</span>
        <div class="slider-wrapper">
          <input
            type="range"
            min="1"
            max="100"
            :value="intensity"
            @input="onIntensityInput"
            class="slider"
          />
        </div>
        <span class="intensity-value">{{ intensity }}%</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  intensity: number
  enabled: boolean
}>()

const emit = defineEmits<{
  'update:intensity': [value: number]
  'update:enabled': [value: boolean]
}>()

const selectedEffect = ref(0)

const effects = [
  { value: 0, icon: '🔊', key: 'classic' },
  { value: 1, icon: '⚡', key: 'electric' },
  { value: 2, icon: '💥', key: 'clipping' },
  { value: 3, icon: '📻', key: 'whiteNoise' },
  { value: 4, icon: '🤖', key: 'robot' },
  { value: 5, icon: '👿', key: 'demon' },
  { value: 6, icon: '🌀', key: 'echo' },
]

const EFFECT_KEY = 'meowmic-explode-effect'

async function toggleEnabled() {
  const newEnabled = !props.enabled
  emit('update:enabled', newEnabled)
  try {
    await invoke('set_explode_mode', {
      enabled: newEnabled,
      intensity: props.intensity
    })
    // 同步当前选择的效果类型
    if (newEnabled) {
      await invoke('set_explode_effect', { effect: selectedEffect.value })
    }
  } catch (e) {
    console.error('Failed to toggle explode mode:', e)
  }
}

function onIntensityInput(e: Event) {
  const target = e.target as HTMLInputElement
  const value = parseInt(target.value, 10)
  emit('update:intensity', value)
  invoke('set_explode_mode', {
    enabled: props.enabled,
    intensity: value
  }).catch(e => console.error('Failed to update explode intensity:', e))
}

function selectEffect(effect: number) {
  selectedEffect.value = effect
  localStorage.setItem(EFFECT_KEY, effect.toString())
  invoke('set_explode_effect', { effect }).catch(e => console.error('Failed to set explode effect:', e))
}

onMounted(() => {
  const savedEffect = localStorage.getItem(EFFECT_KEY)
  if (savedEffect !== null) {
    selectedEffect.value = parseInt(savedEffect, 10) || 0
  }
})
</script>

<style scoped>
.explode-control {
  display: flex;
  flex-direction: column;
  gap: 12px;
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

.setting-desc {
  font-size: 12px;
  color: var(--text-muted);
  margin: 0;
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

.effect-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.effect-chip {
  padding: 5px 12px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg);
  color: var(--text-muted);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.2s;
  white-space: nowrap;
}

.effect-chip:hover {
  border-color: var(--accent);
  color: var(--text-primary);
}

.effect-chip.active {
  background: var(--accent);
  border-color: var(--accent);
  color: white;
}

.intensity-controls {
  margin-top: 4px;
}

.intensity-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.intensity-label {
  font-size: 12px;
  color: var(--text-muted);
  min-width: 50px;
  flex-shrink: 0;
}

.slider-wrapper {
  flex: 1;
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
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--accent);
  cursor: pointer;
  box-shadow: 0 0 6px var(--accent);
  transition: transform 0.2s;
}

.slider::-webkit-slider-thumb:hover {
  transform: scale(1.15);
}

.intensity-value {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  min-width: 35px;
  text-align: right;
  font-family: 'DM Mono', monospace;
}
</style>
