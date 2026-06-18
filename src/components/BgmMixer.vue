<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()

interface AudioProcess {
  friendly: string
  name: string
  pid: number
}

const enabled = ref(false)
const processes = ref<AudioProcess[]>([])
const selectedPids = ref<Set<number>>(new Set())
const bgmGain = ref(100)
const loading = ref(false)
const switching = ref(false)

const bgmGainColor = computed(() => {
  const v = bgmGain.value
  if (v < 100) return '#10b981'
  if (v < 200) return '#06b6d4'
  return '#3b82f6'
})

const loadProcesses = async () => {
  try {
    const result = await invoke<[string, string, number][]>('list_audio_processes')
    processes.value = result.map(([friendly, name, pid]) => ({ friendly, name, pid }))
  } catch (e) {
    console.error('Failed to load audio processes:', e)
  }
}

// 监听全局快捷键触发的 BGM 开关
const handleHotkeyToggle = () => {
  handleToggle(!enabled.value)
}

onMounted(() => {
  window.addEventListener('hotkey-toggle-bgm', handleHotkeyToggle)
})

onUnmounted(() => {
  window.removeEventListener('hotkey-toggle-bgm', handleHotkeyToggle)
})

const handleToggle = async (val: boolean) => {
  loading.value = true
  try {
    if (val) {
      // 没有选中进程时先开开关
      if (selectedPids.value.size === 0) {
        enabled.value = true
      } else {
        await invoke('start_bgm', { pids: Array.from(selectedPids.value) })
        enabled.value = true
      }
    } else {
      await invoke('stop_bgm')
      enabled.value = false
    }
  } catch (e) {
    console.error('Failed to toggle BGM:', e)
  } finally {
    loading.value = false
  }
}

const toggleProcess = async (pid: number) => {
  if (switching.value) return
  switching.value = true
  try {
    const newSet = new Set(selectedPids.value)
    if (newSet.has(pid)) {
      newSet.delete(pid)
    } else {
      newSet.add(pid)
    }
    selectedPids.value = newSet

    // 如果已开启，立即更新混音
    if (enabled.value) {
      try {
        await invoke('stop_bgm')
      } catch {}
      if (newSet.size > 0) {
        await invoke('start_bgm', { pids: Array.from(newSet) })
      }
    }
  } finally {
    switching.value = false
  }
}

const handleBgmGainChange = async (val: number) => {
  bgmGain.value = val
  if (enabled.value) {
    await invoke('update_bgm_config', { bgmGain: val / 100 })
  }
}

const handleBgmGainDirect = async (val: number) => {
  if (!isNaN(val)) {
    const clamped = Math.max(0, Math.min(1000, val))
    bgmGain.value = clamped
    if (enabled.value) {
      await invoke('update_bgm_config', { bgmGain: clamped / 100 })
    }
  }
}

const spinBgmGain = async (delta: number) => {
  const newVal = Math.min(1000, Math.max(0, bgmGain.value + delta))
  bgmGain.value = newVal
  if (enabled.value) {
    await invoke('update_bgm_config', { bgmGain: newVal / 100 })
  }
}

const blockNegative = (e: KeyboardEvent) => {
  if (e.key === '-' || e.key === 'e' || e.key === 'E' || e.key === '.') {
    e.preventDefault()
  }
}

const refreshProcesses = async () => {
  await loadProcesses()
}

onMounted(() => {
  loadProcesses()
})

onUnmounted(() => {
})
</script>

<template>
  <div class="bgm-mixer">
    <div class="control-header">
      <span class="label">{{ t('bgm.label') }}</span>
      <div class="header-right">
        <button class="refresh-btn" @click="refreshProcesses" title="刷新进程列表">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="23 4 23 10 17 10"/>
            <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
          </svg>
        </button>
        <button
          class="toggle"
          :class="{ active: enabled }"
          @click="handleToggle(!enabled)"
          :disabled="loading"
        >
          <span class="toggle-knob"></span>
        </button>
      </div>
    </div>
    <p class="setting-desc">{{ t('bgm.desc') }}</p>

    <div class="process-chips">
      <button
        v-for="proc in processes"
        :key="proc.pid"
        class="process-chip"
        :class="{ active: selectedPids.has(proc.pid) }"
        @click="toggleProcess(proc.pid)"
      >
        {{ proc.friendly }}
      </button>
      <p v-if="processes.length === 0" class="no-process">{{ t('bgm.noProcess') }}</p>
    </div>

    <div class="volume-controls">
      <div class="volume-row">
        <span class="volume-label">BGM</span>
        <div class="slider-wrapper">
          <input
            type="range"
            min="0"
            max="300"
            :value="Math.min(bgmGain, 300)"
            @input="handleBgmGainChange(Number(($event.target as HTMLInputElement).value))"
            class="slider"
            :style="{ '--slider-color': bgmGainColor }"
          />
        </div>
        <div class="gain-input-wrapper">
          <input
            type="number"
            class="gain-input"
            :value="bgmGain"
            @change="handleBgmGainDirect(Number(($event.target as HTMLInputElement).value))"
            @focus="($event.target as HTMLInputElement).select()"
            @keydown="blockNegative"
            min="0"
            max="1000"
            step="10"
          />
          <span class="gain-percent">%</span>
          <div class="gain-spinner">
            <button class="spin-up" @click="spinBgmGain(10)">▲</button>
            <button class="spin-down" @click="spinBgmGain(-10)">▼</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.bgm-mixer {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.control-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 6px;
}

.label {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.refresh-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.2s;
}

.refresh-btn:hover {
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

.toggle:disabled {
  opacity: 0.5;
  cursor: not-allowed;
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

.process-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.process-chip {
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

.process-chip:hover {
  border-color: var(--accent);
  color: var(--text-primary);
}

.process-chip.active {
  background: var(--accent);
  border-color: var(--accent);
  color: white;
}

.no-process {
  font-size: 11px;
  color: var(--warning);
  margin: 4px 0 0;
}

.volume-controls {
  margin-top: 4px;
}

.volume-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.volume-label {
  font-size: 12px;
  color: var(--text-muted);
  min-width: 32px;
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
  background: var(--slider-color);
  cursor: pointer;
  box-shadow: 0 0 6px var(--slider-color);
  transition: transform 0.2s;
}

.slider::-webkit-slider-thumb:hover {
  transform: scale(1.15);
}

.slider:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.volume-value {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  min-width: 32px;
  text-align: right;
  font-family: 'DM Mono', monospace;
}

.gain-input-wrapper {
  display: flex;
  align-items: center;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  overflow: hidden;
}

.gain-input {
  width: 40px;
  background: transparent;
  border: none;
  color: var(--text-primary);
  font-size: 12px;
  font-family: 'DM Mono', monospace;
  text-align: center;
  outline: none;
  padding: 3px 0;
  -moz-appearance: textfield;
}

.gain-input::-webkit-outer-spin-button,
.gain-input::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}

.gain-input:focus {
  outline: none;
}

.gain-percent {
  font-size: 11px;
  color: var(--text-muted);
  padding-right: 6px;
}

.gain-spinner {
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--border);
}

.gain-spinner button {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 0 4px;
  font-size: 9px;
  line-height: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.15s;
}

.gain-spinner button:hover {
  color: var(--text-primary);
}

.gain-spinner button:active {
  color: var(--accent);
}

.gain-spinner .spin-up {
  padding-bottom: 1px;
  border-bottom: 1px solid var(--border);
}

.gain-spinner .spin-down {
  padding-top: 1px;
}
</style>
