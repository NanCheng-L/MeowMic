<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

interface AudioProcess {
  friendly: string
  name: string
  pid: number
}

const enabled = ref(false)
const processes = ref<AudioProcess[]>([])
const selectedPid = ref<number>(0)
const selectedName = ref('')
const bgmVolume = ref(30)
const loading = ref(false)

const bgmVolumeColor = computed(() => {
  const v = bgmVolume.value
  if (v < 33) return '#10b981'
  if (v < 66) return '#06b6d4'
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

const handleToggle = async (val: boolean) => {
  loading.value = true
  try {
    if (val) {
      if (!selectedPid.value) {
        // 默认选第一个
        if (processes.value.length > 0) {
          selectedPid.value = processes.value[0].pid
          selectedName.value = processes.value[0].name
        } else {
          loading.value = false
          return
        }
      }
      await invoke('start_bgm', {
        processName: selectedName.value,
        pid: selectedPid.value,
      })
      enabled.value = true
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

const handleProcessChange = async () => {
  const proc = processes.value.find(p => p.pid === selectedPid.value)
  if (proc) {
    selectedName.value = proc.name
  }
  if (enabled.value) {
    await invoke('stop_bgm')
    if (selectedPid.value) {
      await invoke('start_bgm', {
        processName: selectedName.value,
        pid: selectedPid.value,
      })
    }
  }
}

const handleBgmVolumeChange = async (val: number) => {
  bgmVolume.value = val
  if (enabled.value) {
    await invoke('update_bgm_config', { volume: val / 100 })
  }
}

const refreshProcesses = async () => {
  await loadProcesses()
}

onMounted(() => {
  loadProcesses()
})
</script>

<template>
  <div class="bgm-mixer">
    <div class="control-header">
      <span class="label">BGM 混音</span>
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
    <p class="setting-desc">捕获指定进程的音频混合到麦克风输出</p>

    <div class="device-select">
      <select
        v-model="selectedPid"
        @change="handleProcessChange"
        :disabled="!enabled"
        class="device-dropdown"
      >
        <option :value="0" disabled>选择播放进程...</option>
        <option v-for="proc in processes" :key="proc.pid" :value="proc.pid">
          {{ proc.friendly }}
        </option>
      </select>
      <p v-if="processes.length === 0" class="no-process">未检测到音乐播放器进程，请先打开音乐软件</p>
    </div>

    <div class="volume-controls">
      <div class="volume-row">
        <span class="volume-label">BGM</span>
        <div class="slider-wrapper">
          <input
            type="range"
            min="0"
            max="100"
            :value="bgmVolume"
            @input="handleBgmVolumeChange(Number(($event.target as HTMLInputElement).value))"
            :disabled="!enabled"
            class="slider"
            :style="{ '--slider-color': bgmVolumeColor }"
          />
        </div>
        <span class="volume-value">{{ bgmVolume }}%</span>
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

.device-select {
  margin-top: 4px;
}

.device-dropdown {
  width: 100%;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 12px;
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
  cursor: pointer;
  appearance: none;
}

.device-dropdown:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.device-dropdown:focus {
  border-color: var(--accent);
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
</style>
