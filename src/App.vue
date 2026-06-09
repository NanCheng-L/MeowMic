<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen } from '@tauri-apps/api/event'
import { useAudioEngine } from './composables/useAudioEngine'
import { useAudioStats } from './composables/useAudioStats'
import { useSettings } from './composables/useSettings'
import DeviceSelector from './components/DeviceSelector.vue'
import DenoiseControl from './components/DenoiseControl.vue'
import AudioMeter from './components/AudioMeter.vue'
import SpectrumVisualizer from './components/SpectrumVisualizer.vue'
import StatusBadge from './components/StatusBadge.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import BgmMixer from './components/BgmMixer.vue'
import ExplodeButton from './components/ExplodeButton.vue'

const { t } = useI18n()

const { startDenoising, stopDenoising, updateConfig, listInputDevices, listOutputDevices, listDenoiseModels, setMonitorMode, setMonitorDevice } = useAudioEngine()
const { stats, startPolling, stopPolling } = useAudioStats()
const { loadSettings } = useSettings()

const inputDevices = ref<string[]>([])
const outputDevices = ref<string[]>([])
const selectedInput = ref('')
const selectedOutput = ref('')
const isRunning = ref(false)
const denoiseEnabled = ref(true)
const denoiseStrength = ref(0.8)
const isLoading = ref(false)
const showSettings = ref(false)
const selectedModel = ref('RNNoise')
const availableModels = ref<string[]>([])
const selectedPreset = ref('standard')
const monitorEnabled = ref(false)
const monitorDevice = ref('')

let unlistenToggle: (() => void) | null = null
let unlistenDevices: (() => void) | null = null
let initialized = false
let restartTimer: ReturnType<typeof setTimeout> | null = null
let restarting = false

const loadConfig = () => {
  try {
    const saved = localStorage.getItem('pico-denoise-config')
    if (saved) {
      const config = JSON.parse(saved)
      if (config.selectedInput) selectedInput.value = config.selectedInput
      if (config.selectedOutput) selectedOutput.value = config.selectedOutput
      if (config.denoiseEnabled !== undefined) denoiseEnabled.value = config.denoiseEnabled
      if (config.denoiseStrength !== undefined) denoiseStrength.value = config.denoiseStrength
      if (config.selectedModel) selectedModel.value = config.selectedModel
      if (config.selectedPreset) selectedPreset.value = config.selectedPreset
      if (config.monitorEnabled !== undefined) monitorEnabled.value = config.monitorEnabled
      if (config.monitorDevice) monitorDevice.value = config.monitorDevice
    }
  } catch (e) {
    console.error('Failed to load config:', e)
  }
}

const saveConfig = () => {
  try {
    localStorage.setItem('pico-denoise-config', JSON.stringify({
      selectedInput: selectedInput.value,
      selectedOutput: selectedOutput.value,
      denoiseEnabled: denoiseEnabled.value,
      denoiseStrength: denoiseStrength.value,
      selectedModel: selectedModel.value,
      selectedPreset: selectedPreset.value,
      monitorEnabled: monitorEnabled.value,
      monitorDevice: monitorDevice.value,
    }))
  } catch (e) {
    console.error('Failed to save config:', e)
  }
}

const postInstall = ref(false)

const loadDevices = async () => {
  try {
    inputDevices.value = await listInputDevices()
    outputDevices.value = await listOutputDevices()
    // 自动选择：没有选中 或 选中的设备已不在列表中
    if (inputDevices.value.length > 0 && (!selectedInput.value || !inputDevices.value.includes(selectedInput.value))) {
      selectedInput.value = inputDevices.value[0]
    }
    if (outputDevices.value.length > 0) {
      if (postInstall.value) {
        // 安装完成后优先选中 VB-Cable
        const vbCable = outputDevices.value.find(d => d.toLowerCase().includes('cable') || d.toLowerCase().includes('vb-audio'))
        if (vbCable) {
          selectedOutput.value = vbCable
          postInstall.value = false
        } else if (!selectedOutput.value || !outputDevices.value.includes(selectedOutput.value)) {
          selectedOutput.value = outputDevices.value[0]
        }
      } else if (!selectedOutput.value || !outputDevices.value.includes(selectedOutput.value)) {
        selectedOutput.value = outputDevices.value[0]
      }
    }
  } catch (e) {
    console.error('Failed to load devices:', e)
  }
}

const handleRefresh = () => {
  console.log('[DeviceRefresh] handleRefresh called')
  loadDevices()
}

const handleStart = async () => {
  if (isRunning.value || isLoading.value) return
  if (!selectedInput.value || !selectedOutput.value) {
    console.warn('No device selected, skipping start')
    return
  }
  isLoading.value = true
  try {
    await startDenoising(selectedInput.value, selectedOutput.value, selectedModel.value)
    isRunning.value = true
    startPolling()
    // 同步监听模式到后端
    if (monitorEnabled.value) {
      await setMonitorMode(true)
      if (monitorDevice.value) {
        await setMonitorDevice(monitorDevice.value)
      }
    }
  } catch (e) {
    const msg = String(e)
    // HMR 热重载后前端状态重置但 Rust 引擎仍在运行，同步状态
    if (msg.includes('already running')) {
      console.warn('Engine already running, syncing state')
      isRunning.value = true
      startPolling()
    } else {
      console.error('Failed to start denoising:', e)
    }
  } finally {
    isLoading.value = false
  }
}

const handleStop = async () => {
  if (!isRunning.value || isLoading.value) return
  isLoading.value = true
  try {
    await stopDenoising()
    isRunning.value = false
    stopPolling()
  } catch (e) {
    console.error('Failed to stop denoising:', e)
  } finally {
    isLoading.value = false
  }
}

// 统一的 debounce restart — 防止 watcher 和 devices-changed 同时触发导致竞争
const scheduleRestart = (delay = 500) => {
  if (restartTimer) clearTimeout(restartTimer)
  restartTimer = setTimeout(async () => {
    restartTimer = null
    if (restarting) return
    if (!isRunning.value) return
    restarting = true
    try {
      await handleStop()
      if (selectedInput.value && selectedOutput.value) {
        await handleStart()
      }
    } finally {
      restarting = false
    }
  }, delay)
}

const handleEnabledChange = async (enabled: boolean) => {
  denoiseEnabled.value = enabled
  await updateConfig({ enabled })
  saveConfig()
}

const handleStrengthChange = async (strength: number) => {
  denoiseStrength.value = strength
  await updateConfig({ strength })
  saveConfig()
}

const handlePresetChange = (preset: string) => {
  selectedPreset.value = preset
  saveConfig()
}

const handleModelChange = async (model: string) => {
  selectedModel.value = model
  saveConfig()
  // 切换模型需要重启引擎
  if (isRunning.value) {
    scheduleRestart(100)
  }
}

const handleMonitorChange = async (enabled: boolean) => {
  monitorEnabled.value = enabled
  saveConfig()
  try {
    await setMonitorMode(enabled)
    // 同步监听设备到后端
    if (enabled && monitorDevice.value) {
      await setMonitorDevice(monitorDevice.value)
    }
  } catch (e) {
    console.error('Failed to set monitor mode:', e)
  }
}

const handleMonitorDeviceChange = async (device: string) => {
  monitorDevice.value = device
  saveConfig()
  try {
    await setMonitorDevice(device)
  } catch (e) {
    console.error('Failed to set monitor device:', e)
  }
}

// 全局快捷键触发的降噪切换
const toggleDenoise = async () => {
  await handleEnabledChange(!denoiseEnabled.value)
}

watch([selectedInput, selectedOutput], () => {
  saveConfig()
  if (initialized && isRunning.value) {
    scheduleRestart(100)
  }
})

watch([denoiseEnabled, denoiseStrength], () => {
  updateConfig({ enabled: denoiseEnabled.value, strength: denoiseStrength.value })
  saveConfig()
})

onMounted(async () => {
  loadConfig()
  await loadDevices()
  await loadSettings()
  try {
    availableModels.value = await listDenoiseModels()
  } catch (e) {
    console.error('Failed to load denoise models:', e)
  }
  // 先尝试停掉残留引擎（HMR 热重载时旧实例未清理）
  try { await stopDenoising() } catch {}
  await handleStart()
  initialized = true

  // 监听 Rust 端全局快捷键事件
  unlistenToggle = await listen('toggle-denoise', () => {
    toggleDenoise()
  })

  // 监听设备热拔插事件（防抖：快速插拔只触发一次重启）
  unlistenDevices = await listen<{
    input_devices: string[]
    output_devices: string[]
  }>('devices-changed', async (event) => {
    // 初始化期间跳过，避免干扰启动流程
    if (!initialized) return

    // 更新设备列表
    const prevInput = selectedInput.value
    const prevOutput = selectedOutput.value

    inputDevices.value = event.payload.input_devices
    outputDevices.value = event.payload.output_devices

    // 如果之前选中的设备还在列表中，保持选中；否则选第一个
    if (inputDevices.value.length > 0) {
      if (!inputDevices.value.includes(prevInput)) {
        selectedInput.value = inputDevices.value[0]
      }
    } else {
      selectedInput.value = ''
    }

    if (outputDevices.value.length > 0) {
      if (!outputDevices.value.includes(prevOutput)) {
        selectedOutput.value = outputDevices.value[0]
      }
    } else {
      selectedOutput.value = ''
    }

    // debounce restart — 快速插拔只触发一次
    scheduleRestart(500)
  })
})

onUnmounted(() => {
  unlistenToggle?.()
  unlistenDevices?.()
  if (restartTimer) clearTimeout(restartTimer)
  if (isRunning.value) {
    stopDenoising().catch(console.error)
  }
  stopPolling()
})
</script>

<template>
  <div class="app">
    <header class="header">
      <div class="header-left">
        <svg class="logo" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/>
          <path d="M19 10v2a7 7 0 0 1-14 0v-2"/>
          <line x1="12" y1="19" x2="12" y2="23"/>
          <line x1="8" y1="23" x2="16" y2="23"/>
        </svg>
        <h1>pico-denoise</h1>
      </div>
      <div class="header-right">
        <button class="settings-btn" @click="showSettings = true" :title="t('settings.title')">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="3"/>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
        </button>
        <StatusBadge :running="isRunning" :latency="stats.latency_ms" />
      </div>
    </header>

    <main class="main">
      <section class="section">
        <DeviceSelector
          :input-devices="inputDevices"
          :output-devices="outputDevices"
          v-model:input-value="selectedInput"
          v-model:output-value="selectedOutput"
          @refresh="handleRefresh"
          @install-success="postInstall = true"
        />
      </section>

      <section class="section">
        <DenoiseControl
          :enabled="denoiseEnabled"
          :strength="denoiseStrength"
          :model="selectedModel"
          :available-models="availableModels"
          :preset="selectedPreset"
          :loading="isLoading"
          :monitor-enabled="monitorEnabled"
          :monitor-devices="outputDevices"
          :monitor-device="monitorDevice"
          @update:enabled="handleEnabledChange"
          @update:strength="handleStrengthChange"
          @update:model="handleModelChange"
          @update:preset="handlePresetChange"
          @update:monitor-enabled="handleMonitorChange"
          @update:monitor-device="handleMonitorDeviceChange"
        />
      </section>

      <section class="section">
        <BgmMixer />
      </section>

      <section class="section">
        <ExplodeButton />
      </section>

      <section class="section visualization">
        <div class="meters">
          <AudioMeter :label="t('stats.input')" :level="stats.input_level" />
          <AudioMeter :label="t('stats.output')" :level="stats.output_level" />
        </div>
        <SpectrumVisualizer :data="stats.spectrum" />
      </section>

      <section class="section stats-bar">
        <div class="stat">
          <span class="stat-label">{{ t('stats.reduction') }}</span>
          <span class="stat-value">{{ stats.noise_reduction_db.toFixed(1) }} dB</span>
        </div>
        <div class="stat">
          <span class="stat-label">{{ t('stats.latency') }}</span>
          <span class="stat-value">{{ stats.latency_ms.toFixed(1) }} ms</span>
        </div>
        <div class="stat">
          <span class="stat-label">{{ t('stats.frames') }}</span>
          <span class="stat-value">{{ stats.frames_processed.toLocaleString() }}</span>
        </div>
      </section>
    </main>

    <SettingsDialog :open="showSettings" @close="showSettings = false" />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border);
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.logo {
  color: var(--accent);
}

.header h1 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
}

.settings-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 6px;
  border-radius: 6px;
  transition: color 0.2s, background 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
}

.settings-btn:hover {
  color: var(--text-primary);
  background: var(--border);
}

.main {
  flex: 1;
  padding: 20px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.section {
  background: var(--surface);
  border-radius: 12px;
  padding: 16px;
}

.visualization {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.meters {
  display: flex;
  gap: 16px;
}

.stats-bar {
  display: flex;
  justify-content: space-around;
  padding: 12px 16px;
}

.stat {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}

.stat-label {
  font-size: 11px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.stat-value {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  font-family: 'DM Mono', monospace;
}
</style>
