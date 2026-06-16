<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, emit } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { useAudioEngine } from './composables/useAudioEngine'
import { useAudioStats } from './composables/useAudioStats'
import { useSettings } from './composables/useSettings'
import { useTheme } from './composables/useTheme'
import { setLocale } from './locales'
import DeviceSelector from './components/DeviceSelector.vue'
import DenoiseControl from './components/DenoiseControl.vue'
import AudioMeter from './components/AudioMeter.vue'
import SpectrumVisualizer from './components/SpectrumVisualizer.vue'
import StatusBadge from './components/StatusBadge.vue'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import BgmMixer from './components/BgmMixer.vue'
import ExplodeButton from './components/ExplodeButton.vue'
import EqControl from './components/EqControl.vue'
import { check } from '@tauri-apps/plugin-updater'

const { t } = useI18n()

const { startDenoising, stopDenoising, updateConfig, listInputDevices, listOutputDevices, listDenoiseModels, setMonitorMode, setMonitorPoint } = useAudioEngine()
const { stats, startPolling, stopPolling } = useAudioStats()
const { settings, loadSettings } = useSettings()
const { theme } = useTheme()

const inputDevices = ref<string[]>([])
const outputDevices = ref<string[]>([])
const selectedInput = ref('')
const selectedOutput = ref('')
const isRunning = ref(false)
const denoiseEnabled = ref(false)
const denoiseStrength = ref(0.5)
const micGain = ref(1.0)
const isLoading = ref(false)

const openTutorial = async () => {
  const existing = await WebviewWindow.getByLabel('tutorial')
  if (existing) {
    try {
      await existing.show()
      await existing.setFocus()
      return
    } catch {
      // 窗口已销毁，重新创建
    }
  }
  const isDev = location.hostname === 'localhost'
  const baseUrl = isDev ? 'http://localhost:1420' : 'tauri://localhost'
  new WebviewWindow('tutorial', {
    url: `${baseUrl}/tutorial.html`,
    title: '使用教程 - MeowMic',
    width: 520,
    height: 640,
    center: true,
    resizable: true,
  })
}

const openSettings = async () => {
  const existing = await WebviewWindow.getByLabel('settings')
  if (existing) {
    try {
      await existing.show()
      await existing.setFocus()
      // 发送最新配置
      await emit('settings-init', {
        selectedInput: selectedInput.value,
        selectedOutput: selectedOutput.value,
        selectedModel: selectedModel.value,
        monitorEnabled: monitorEnabled.value,
        inputDevices: inputDevices.value,
        outputDevices: outputDevices.value,
        availableModels: availableModels.value,
        hotkey: settings.value.hotkey,
        hotkeyEnabled: settings.value.hotkeyEnabled,
        autostart: settings.value.autostart,
        language: localStorage.getItem('meowmic-lang') || 'zh-CN',
        theme: theme.value,
      })
      return
    } catch {
      // 窗口引用已失效，重新创建
    }
  }
  const isDev = location.hostname === 'localhost'
  const baseUrl = isDev ? 'http://localhost:1420' : 'tauri://localhost'
  new WebviewWindow('settings', {
    url: `${baseUrl}/settings.html`,
    title: '设置 - MeowMic',
    width: 480,
    height: 580,
    center: true,
    resizable: true,
  })
  setTimeout(async () => {
    try {
      await emit('settings-init', {
        selectedInput: selectedInput.value,
        selectedOutput: selectedOutput.value,
        selectedModel: selectedModel.value,
        monitorEnabled: monitorEnabled.value,
        inputDevices: inputDevices.value,
        outputDevices: outputDevices.value,
        availableModels: availableModels.value,
        hotkey: settings.value.hotkey,
        hotkeyEnabled: settings.value.hotkeyEnabled,
        autostart: settings.value.autostart,
        language: localStorage.getItem('meowmic-lang') || 'zh-CN',
        theme: theme.value,
      })
    } catch {}
  }, 300)
}

const selectedModel = ref('RNNoise')
const availableModels = ref<string[]>([])
const selectedPreset = ref('standard')
const monitorEnabled = ref(false)
const monitorPoint = ref(5) // 默认监听最终输出
const explodeIntensity = ref(0.75)
const explodeEnabled = ref(false)
const eqEnabled = ref(false)
const eqActivePreset = ref('custom')
const hasUpdate = ref(false)

let unlistenToggle: (() => void) | null = null
let unlistenDevices: (() => void) | null = null
let unlistenEq: (() => void) | null = null
let initialized = false
let restartTimer: ReturnType<typeof setTimeout> | null = null
let restarting = false
let lastUserInput = '' // 记住用户最后选择的输入设备

const loadConfig = () => {
  try {
    const saved = localStorage.getItem('meowmic-config')
    if (saved) {
      const config = JSON.parse(saved)
      if (config.selectedInput) {
        selectedInput.value = config.selectedInput
        lastUserInput = config.selectedInput
      }
      if (config.selectedOutput) selectedOutput.value = config.selectedOutput
      if (config.denoiseEnabled !== undefined) denoiseEnabled.value = config.denoiseEnabled
      if (config.denoiseStrength !== undefined) denoiseStrength.value = config.denoiseStrength
      if (config.micGain !== undefined) micGain.value = config.micGain
      if (config.selectedModel) selectedModel.value = config.selectedModel
      if (config.selectedPreset) selectedPreset.value = config.selectedPreset
      if (config.monitorEnabled !== undefined) monitorEnabled.value = config.monitorEnabled
      if (config.monitorPoint !== undefined) monitorPoint.value = config.monitorPoint
      if (config.explodeIntensity !== undefined) explodeIntensity.value = config.explodeIntensity
      if (config.eqEnabled !== undefined) eqEnabled.value = config.eqEnabled
    }
    // EQ 预设单独存储
    const savedPreset = localStorage.getItem('meowmic-eq-preset')
    if (savedPreset) eqActivePreset.value = savedPreset
  } catch (e) {
    console.error('Failed to load config:', e)
  }
}

const saveConfig = () => {
  try {
    localStorage.setItem('meowmic-config', JSON.stringify({
      selectedInput: selectedInput.value,
      selectedOutput: selectedOutput.value,
      denoiseEnabled: denoiseEnabled.value,
      denoiseStrength: denoiseStrength.value,
      micGain: micGain.value,
      selectedModel: selectedModel.value,
      selectedPreset: selectedPreset.value,
      monitorEnabled: monitorEnabled.value,
      monitorPoint: monitorPoint.value,
      explodeIntensity: explodeIntensity.value,
      eqEnabled: eqEnabled.value,
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
    await startDenoising(
      selectedInput.value,
      selectedOutput.value,
      selectedModel.value,
      monitorEnabled.value
    )
    isRunning.value = true
    // 启动后同步降噪开关状态到后端
    await updateConfig({ enabled: denoiseEnabled.value, strength: denoiseStrength.value, micGain: micGain.value })
    // 同步监听点到后端
    if (monitorPoint.value > 0) {
      await setMonitorPoint(monitorPoint.value)
    }
    startPolling()
  } catch (e) {
    const msg = String(e)
    // HMR 热重载后前端状态重置但 Rust 引擎仍在运行，同步状态
    if (msg.includes('already running')) {
      console.warn('Engine already running, syncing state')
      isRunning.value = true
      await updateConfig({ enabled: denoiseEnabled.value, strength: denoiseStrength.value, micGain: micGain.value })
      if (monitorPoint.value > 0) {
        await setMonitorPoint(monitorPoint.value)
      }
      startPolling()
    } else if (msg.includes('AUDIO_DEVICE_CONFLICT')) {
      // 输入输出是同一设备导致的死锁
      console.error('Device conflict: input and output are the same device')
      isRunning.value = false
      // 自动切换到系统默认输出
      if (outputDevices.value.length > 0) {
        const altOutput = outputDevices.value.find(d => d !== selectedInput.value) || outputDevices.value[0]
        selectedOutput.value = altOutput
        saveConfig()
        // 重新尝试启动
        setTimeout(() => handleStart(), 500)
      }
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
  // 只更新降噪开关状态，引擎继续运行（用户需要监听麦克风）
  if (isRunning.value) {
    await updateConfig({ enabled })
  } else if (enabled) {
    // 引擎未运行时开启降噪，启动引擎
    await handleStart()
  }
  saveConfig()
}

const handleStrengthChange = async (strength: number) => {
  denoiseStrength.value = strength
  await updateConfig({ strength })
  saveConfig()
}

const handleMicGainChange = async (gain: number) => {
  micGain.value = gain
  await updateConfig({ micGain: gain })
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
  } catch (e) {
    console.error('Failed to set monitor mode:', e)
  }
}

const handleMonitorPointChange = async (point: number) => {
  monitorPoint.value = point
  saveConfig()
  try {
    await setMonitorPoint(point)
  } catch (e) {
    console.error('Failed to set monitor point:', e)
  }
}

const handleEqEnabledChange = async (enabled: boolean) => {
  eqEnabled.value = enabled
  saveConfig()
  try {
    await invoke('update_eq_config', { enabled })
    await emit('eq-changed', { enabled })
  } catch (e) {
    console.error('Failed to update EQ config:', e)
  }
}

const handleEqPresetChange = async (preset: string) => {
  eqActivePreset.value = preset
  localStorage.setItem('meowmic-eq-preset', preset)

  // 自定义：从 localStorage 加载 bands（不覆盖）
  if (preset === 'custom') {
    const savedBands = localStorage.getItem('meowmic-eq-bands')
    const bands = savedBands ? JSON.parse(savedBands) : new Array(10).fill(0)
    await invoke('update_eq_config', { bands })
    await emit('eq-changed', { enabled: eqEnabled.value })
    return
  }

  // 其他预设：从后端获取 bands 值
  try {
    const presets = await invoke<[string, number[]][]>('get_eq_presets')
    const found = presets.find(([k]) => k === preset)
    if (found) {
      await invoke('update_eq_config', { bands: found[1] })
      await emit('eq-changed', { enabled: eqEnabled.value })
    }
  } catch (e) {
    console.error('Failed to apply EQ preset:', e)
  }
}

const openEq = async () => {
  const existing = await WebviewWindow.getByLabel('eq')
  if (existing) {
    try {
      await existing.show()
      await existing.setFocus()
      return
    } catch {
      // 窗口已销毁，重新创建
    }
  }
  const isDev = location.hostname === 'localhost'
  const baseUrl = isDev ? 'http://localhost:1420' : 'tauri://localhost'
  new WebviewWindow('eq', {
    url: `${baseUrl}/eq.html`,
    title: '均衡器 - MeowMic',
    width: 640,
    height: 540,
    center: true,
    resizable: true,
  })
}

// 全局快捷键触发的降噪切换
const toggleDenoise = async () => {
  await handleEnabledChange(!denoiseEnabled.value)
}

// 应用设置窗口发来的变更
const handleApplySettings = async (payload: {
  selectedInput?: string
  selectedOutput?: string
  selectedModel?: string
  monitorEnabled?: boolean
}) => {
  let needRestart = false

  if (payload.selectedInput !== undefined && payload.selectedInput !== selectedInput.value) {
    selectedInput.value = payload.selectedInput
    needRestart = true
  }
  if (payload.selectedOutput !== undefined && payload.selectedOutput !== selectedOutput.value) {
    selectedOutput.value = payload.selectedOutput
    needRestart = true
  }
  if (payload.selectedModel !== undefined && payload.selectedModel !== selectedModel.value) {
    selectedModel.value = payload.selectedModel
    needRestart = true
  }
  if (payload.monitorEnabled !== undefined) {
    monitorEnabled.value = payload.monitorEnabled
    try { await setMonitorMode(payload.monitorEnabled) } catch {}
  }

  saveConfig()

  if (needRestart && isRunning.value) {
    scheduleRestart(100)
  }
}

watch([denoiseEnabled, denoiseStrength], () => {
  updateConfig({ enabled: denoiseEnabled.value, strength: denoiseStrength.value })
  saveConfig()
})

// 用户通过下拉列表切换设备时，重启引擎
watch([selectedInput, selectedOutput], () => {
  if (initialized && isRunning.value) {
    scheduleRestart(100)
  }
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
  // 引擎始终启动，降噪开关只控制降噪处理（用户需要监听麦克风）
  await handleStart()
  initialized = true

  // 启动 3 秒后静默检查更新
  setTimeout(async () => {
    try {
      const update = await check()
      if (update) hasUpdate.value = true
    } catch {}
  }, 3000)

  // 监听 Rust 端全局快捷键事件
  unlistenToggle = await listen('toggle-denoise', () => {
    toggleDenoise()
  })

  // 监听 EQ 窗口状态变更
  unlistenEq = await listen<{ enabled: boolean; preset?: string }>('eq-changed', (event) => {
    eqEnabled.value = event.payload.enabled
    if (event.payload.preset) {
      eqActivePreset.value = event.payload.preset
      localStorage.setItem('meowmic-eq-preset', event.payload.preset)
    }
    saveConfig()
  })

  // 监听设备热拔插事件（防抖：快速插拔只触发一次重启）
  unlistenDevices = await listen<{
    input_devices: string[]
    output_devices: string[]
  }>('devices-changed', async (event) => {
    // 初始化期间跳过，避免干扰启动流程
    if (!initialized) return

    const prevInput = selectedInput.value
    const prevOutput = selectedOutput.value

    inputDevices.value = event.payload.input_devices
    outputDevices.value = event.payload.output_devices

    // 优先恢复用户之前选的设备，其次保持当前设备
    if (inputDevices.value.length > 0) {
      if (lastUserInput && inputDevices.value.includes(lastUserInput)) {
        selectedInput.value = lastUserInput
      } else if (inputDevices.value.includes(prevInput)) {
      } else {
        selectedInput.value = inputDevices.value[0]
      }
    } else {
      selectedInput.value = ''
    }

    if (outputDevices.value.length > 0) {
      if (!outputDevices.value.includes(selectedOutput.value)) {
        selectedOutput.value = outputDevices.value[0]
      }
    } else {
      selectedOutput.value = ''
    }

    // 只在选中的设备实际发生变化时才重启，避免无关设备变化触发无意义重启
    const inputChanged = selectedInput.value !== prevInput
    const outputChanged = selectedOutput.value !== prevOutput
    const inputLost = prevInput && !inputDevices.value.includes(prevInput)
    const outputLost = prevOutput && !outputDevices.value.includes(prevOutput)
    if (inputChanged || outputChanged || inputLost || outputLost) {
      scheduleRestart(100)
    }
  })

  // 轮询检测设置窗口的变更（localStorage 通信，避免跨窗口 emit 问题）
  setInterval(() => {
    try {
      // 设备/模型等设置变更
      const raw = localStorage.getItem('meowmic-pending')
      if (raw) {
        const pending = JSON.parse(raw)
        localStorage.removeItem('meowmic-pending')
        handleApplySettings(pending)
      }
      // 语言切换（独立检查，不受 pending 限制）
      const lang = localStorage.getItem('meowmic-lang')
      if (lang) setLocale(lang)
    } catch {}
  }, 1000)
})

onUnmounted(() => {
  unlistenToggle?.()
  unlistenDevices?.()
  unlistenEq?.()
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
        <h1>MeowMic</h1>
      </div>
      <div class="header-right">
        <button class="settings-btn" @click="openTutorial" :title="t('tutorial.title')">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"/>
            <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/>
            <line x1="12" y1="17" x2="12.01" y2="17"/>
          </svg>
        </button>
        <button class="settings-btn" @click="openSettings" :title="t('settings.title')">
          <span v-if="hasUpdate" class="update-badge"></span>
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
          @update:input-value="(v: string) => { lastUserInput = v }"
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
          :monitor-point="monitorPoint"
          :mic-gain="micGain"
          @update:enabled="handleEnabledChange"
          @update:strength="handleStrengthChange"
          @update:model="handleModelChange"
          @update:preset="handlePresetChange"
          @update:monitor-enabled="handleMonitorChange"
          @update:monitor-point="handleMonitorPointChange"
          @update:mic-gain="handleMicGainChange"
        />
      </section>

      <section class="section">
        <EqControl
          :enabled="eqEnabled"
          :active-preset="eqActivePreset"
          @update:enabled="handleEqEnabledChange"
          @update:preset="handleEqPresetChange"
          @open-eq="openEq"
        />
      </section>

      <section class="section">
        <BgmMixer />
      </section>

      <section class="section">
        <ExplodeButton
          :intensity="explodeIntensity"
          :enabled="explodeEnabled"
          @update:intensity="(v: number) => { explodeIntensity = v; saveConfig() }"
          @update:enabled="(v: boolean) => { explodeEnabled = v }"
        />
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
  overflow: visible;
  position: relative;
}

.update-badge {
  position: absolute;
  top: 2px;
  right: 2px;
  width: 8px;
  height: 8px;
  background: #ef4444;
  border-radius: 50%;
  border: 1.5px solid var(--bg);
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
