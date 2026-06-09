<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettings } from '../composables/useSettings'
import { availableLocales, setLocale } from '../locales'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'

const { t } = useI18n()
const { saveSettings, registerHotkey, unregisterHotkey } = useSettings()

// 暂存的配置
const localConfig = ref({
  selectedInput: '',
  selectedOutput: '',
  selectedModel: 'RNNoise',
  monitorEnabled: false,
  monitorDevice: '',
  hotkey: 'Ctrl+Shift+D',
  hotkeyEnabled: true,
  autostart: false,
  language: 'zh-CN',
})

// 选项数据
const inputDevices = ref<string[]>([])
const outputDevices = ref<string[]>([])
const availableModels = ref<string[]>([])

const recording = ref(false)
const hotkeyError = ref('')
const saving = ref(false)

let recordedKeys: string[] = []
let keydownHandler: ((e: KeyboardEvent) => void) | null = null
let keyupHandler: ((e: KeyboardEvent) => void) | null = null

const formatKey = (e: KeyboardEvent): string => {
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.shiftKey) parts.push('Shift')
  if (e.altKey) parts.push('Alt')
  if (e.metaKey) parts.push('Super')
  const key = e.key
  if (!['Control', 'Shift', 'Alt', 'Meta'].includes(key)) {
    if (key === ' ') parts.push('Space')
    else if (key.length === 1) parts.push(key.toUpperCase())
    else parts.push(key)
  }
  return parts.join('+')
}

const startRecording = () => {
  recording.value = true
  hotkeyError.value = ''
  recordedKeys = []
  keydownHandler = (e: KeyboardEvent) => {
    e.preventDefault()
    e.stopPropagation()
    recordedKeys = [formatKey(e)]
  }
  keyupHandler = (e: KeyboardEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (recordedKeys.length > 0) {
      const hasModifier = recordedKeys[0].includes('Ctrl') ||
        recordedKeys[0].includes('Shift') ||
        recordedKeys[0].includes('Alt') ||
        recordedKeys[0].includes('Super')
      if (hasModifier) {
        localConfig.value.hotkey = recordedKeys[0]
        recording.value = false
        stopRecording()
      } else {
        hotkeyError.value = t('settings.modifierError')
        recording.value = false
        stopRecording()
      }
    }
  }
  window.addEventListener('keydown', keydownHandler, true)
  window.addEventListener('keyup', keyupHandler, true)
}

const stopRecording = () => {
  if (keydownHandler) {
    window.removeEventListener('keydown', keydownHandler, true)
    keydownHandler = null
  }
  if (keyupHandler) {
    window.removeEventListener('keyup', keyupHandler, true)
    keyupHandler = null
  }
  recording.value = false
}

// 接收主窗口发来的当前配置
let unlistenInit: (() => void) | null = null

onMounted(async () => {
  // 从 localStorage 同步语言到 i18n（窗口复用时也能更新）
  const savedLang = localStorage.getItem('pico-denoise-lang')
  if (savedLang) {
    setLocale(savedLang)
  }

  unlistenInit = await listen<any>('settings-init', (event) => {
    const p = event.payload
    localConfig.value.selectedInput = p.selectedInput || ''
    localConfig.value.selectedOutput = p.selectedOutput || ''
    localConfig.value.selectedModel = p.selectedModel || 'RNNoise'
    localConfig.value.monitorEnabled = p.monitorEnabled || false
    localConfig.value.monitorDevice = p.monitorDevice || ''
    localConfig.value.hotkey = p.hotkey || 'Ctrl+Shift+D'
    localConfig.value.hotkeyEnabled = p.hotkeyEnabled ?? true
    localConfig.value.autostart = p.autostart || false
    localConfig.value.language = p.language || 'zh-CN'
    inputDevices.value = p.inputDevices || []
    outputDevices.value = p.outputDevices || []
    availableModels.value = p.availableModels || []
    // 同步语言到 i18n（窗口复用时更新界面语言）
    if (p.language) {
      setLocale(p.language)
    }
  })
})

onUnmounted(() => {
  stopRecording()
  unlistenInit?.()
})

const handleSave = async () => {
  saving.value = true
  hotkeyError.value = ''

  // 1. 保存后端设置（hotkey/autostart）
  try {
    await saveSettings({
      hotkey: localConfig.value.hotkey,
      hotkeyEnabled: localConfig.value.hotkeyEnabled,
      autostart: localConfig.value.autostart,
      language: localConfig.value.language,
    })
  } catch (e) {
    console.error('Failed to save settings:', e)
    saving.value = false
    return
  }

  // 2. 注册/注销快捷键
  try {
    if (localConfig.value.hotkeyEnabled) {
      await registerHotkey(localConfig.value.hotkey)
    } else {
      await unregisterHotkey()
    }
  } catch (e) {
    hotkeyError.value = t('settings.hotkeyError')
  }

  // 3. 语言存 localStorage（主窗口下次启动时读取）
  localStorage.setItem('pico-denoise-lang', localConfig.value.language)

  // 4. 设备/模型变更存 localStorage，主窗口轮询检测
  localStorage.setItem('pico-denoise-pending', JSON.stringify({
    selectedInput: localConfig.value.selectedInput,
    selectedOutput: localConfig.value.selectedOutput,
    selectedModel: localConfig.value.selectedModel,
    monitorEnabled: localConfig.value.monitorEnabled,
    monitorDevice: localConfig.value.monitorDevice,
    applyAt: Date.now(),
  }))

  saving.value = false
  getCurrentWindow().hide()
}

const handleClose = () => {
  getCurrentWindow().hide()
}
</script>

<template>
  <div class="settings-page">
    <div class="settings-header">
      <h1>{{ t('settings.title') }}</h1>
    </div>

    <div class="settings-body">
      <!-- 设备选择 -->
      <section class="section">
        <h2>{{ t('settings.audioDevices') }}</h2>

        <div class="form-group">
          <label class="form-label">{{ t('device.input') }}</label>
          <div class="select-wrapper">
            <select v-model="localConfig.selectedInput">
              <option value="" disabled>{{ t('device.selectInput') }}</option>
              <option v-for="d in inputDevices" :key="d" :value="d">{{ d }}</option>
            </select>
            <span class="select-arrow"></span>
          </div>
        </div>

        <div class="form-group">
          <label class="form-label">{{ t('device.output') }}</label>
          <div class="select-wrapper">
            <select v-model="localConfig.selectedOutput">
              <option value="" disabled>{{ t('device.selectOutput') }}</option>
              <option v-for="d in outputDevices" :key="d" :value="d">{{ d }}</option>
            </select>
            <span class="select-arrow"></span>
          </div>
        </div>
      </section>

      <!-- 降噪模型 -->
      <section class="section">
        <h2>{{ t('denoise.model') }}</h2>
        <div class="form-group">
          <div class="select-wrapper">
            <select v-model="localConfig.selectedModel">
              <option v-for="m in availableModels" :key="m" :value="m">{{ m }}</option>
            </select>
            <span class="select-arrow"></span>
          </div>
        </div>
      </section>

      <!-- 监听 -->
      <section class="section">
        <h2>{{ t('denoise.monitor') }}</h2>
        <div class="setting-row">
          <span class="setting-label">{{ t('denoise.monitorDesc') }}</span>
          <button
            class="toggle"
            :class="{ active: localConfig.monitorEnabled }"
            @click="localConfig.monitorEnabled = !localConfig.monitorEnabled"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>
        <div v-if="localConfig.monitorEnabled" class="form-group">
          <label class="form-label">{{ t('device.selectMonitor') }}</label>
          <div class="select-wrapper">
            <select v-model="localConfig.monitorDevice">
              <option value="" disabled>{{ t('device.selectMonitor') }}</option>
              <option v-for="d in outputDevices" :key="d" :value="d">{{ d }}</option>
            </select>
            <span class="select-arrow"></span>
          </div>
        </div>
      </section>

      <!-- 快捷键 -->
      <section class="section">
        <h2>{{ t('settings.hotkey') }}</h2>
        <div class="setting-row">
          <span class="setting-label">{{ t('settings.hotkeyDesc') }}</span>
          <button
            class="toggle"
            :class="{ active: localConfig.hotkeyEnabled }"
            @click="localConfig.hotkeyEnabled = !localConfig.hotkeyEnabled"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>
        <div class="hotkey-section">
          <div class="hotkey-display">
            <span v-if="!recording" class="hotkey-value">{{ localConfig.hotkey }}</span>
            <span v-else class="hotkey-recording">{{ t('settings.hotkeyRecording') }}</span>
          </div>
          <button
            class="record-btn"
            :class="{ recording }"
            @click="recording ? stopRecording() : startRecording()"
          >
            {{ recording ? t('settings.cancel') : t('settings.record') }}
          </button>
        </div>
        <p v-if="hotkeyError" class="hotkey-error">{{ hotkeyError }}</p>
      </section>

      <!-- 开机自启 -->
      <section class="section">
        <h2>{{ t('settings.autostart') }}</h2>
        <div class="setting-row">
          <span class="setting-label">{{ t('settings.autostartDesc') }}</span>
          <button
            class="toggle"
            :class="{ active: localConfig.autostart }"
            @click="localConfig.autostart = !localConfig.autostart"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>
      </section>

      <!-- 语言 -->
      <section class="section">
        <h2>{{ t('settings.language') }}</h2>
        <div class="form-group">
          <select class="language-select" v-model="localConfig.language">
            <option v-for="l in availableLocales" :key="l.value" :value="l.value">
              {{ l.label }}
            </option>
          </select>
        </div>
      </section>
    </div>

    <div class="settings-footer">
      <button class="btn btn-secondary" @click="handleClose">{{ t('settings.cancel') }}</button>
      <button class="btn btn-primary" @click="handleSave" :disabled="saving">
        {{ saving ? t('settings.saving') : t('settings.save') }}
      </button>
    </div>
  </div>
</template>

<style>
html, body {
  margin: 0;
  padding: 0;
  overflow: hidden;
  height: 100vh;
  background: #0a0a0a;
  font-family: 'DM Sans', -apple-system, BlinkMacSystemFont, sans-serif;
}
</style>

<style scoped>
.settings-page {
  height: 100vh;
  background: #0a0a0a;
  color: #e5e5e5;
  display: flex;
  flex-direction: column;
}

.settings-header {
  padding: 24px 28px 16px;
  border-bottom: 1px solid #262626;
}

.settings-header h1 {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
}

.settings-body {
  flex: 1;
  padding: 24px 28px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 28px;
}

.settings-body::-webkit-scrollbar {
  width: 6px;
}

.settings-body::-webkit-scrollbar-track {
  background: #0a0a0a;
}

.settings-body::-webkit-scrollbar-thumb {
  background: #1e1e1e;
  border-radius: 3px;
}

.settings-body::-webkit-scrollbar-thumb:hover {
  background: #666666;
}

.section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.section h2 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: #a3a3a3;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding-bottom: 8px;
  border-bottom: 1px solid #262626;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-label {
  font-size: 13px;
  color: #a3a3a3;
}

.select-wrapper {
  position: relative;
}

.select-wrapper select {
  width: 100%;
  padding: 10px 32px 10px 12px;
  background: #171717;
  border: 1px solid #262626;
  border-radius: 8px;
  color: #e5e5e5;
  font-size: 13px;
  appearance: none;
  cursor: pointer;
  transition: border-color 0.2s;
}

.select-wrapper select:hover {
  border-color: #404040;
}

.select-wrapper select:focus {
  outline: none;
  border-color: #10b981;
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
  border-top: 5px solid #737373;
  pointer-events: none;
}

.setting-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.setting-label {
  font-size: 14px;
  color: #e5e5e5;
}

/* Toggle */
.toggle {
  width: 44px;
  height: 24px;
  border-radius: 12px;
  border: none;
  background: #404040;
  cursor: pointer;
  position: relative;
  transition: background 0.3s;
  padding: 0;
  flex-shrink: 0;
}

.toggle.active {
  background: #10b981;
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

/* Hotkey */
.hotkey-section {
  display: flex;
  gap: 8px;
  align-items: center;
}

.hotkey-display {
  flex: 1;
  background: #171717;
  border: 1px solid #262626;
  border-radius: 8px;
  padding: 8px 12px;
  text-align: center;
}

.hotkey-value {
  font-family: 'DM Mono', monospace;
  font-size: 14px;
  font-weight: 500;
  color: #e5e5e5;
  letter-spacing: 0.5px;
}

.hotkey-recording {
  font-size: 13px;
  color: #10b981;
  animation: pulse 1.5s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.record-btn {
  background: #171717;
  border: 1px solid #262626;
  border-radius: 8px;
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  color: #e5e5e5;
  cursor: pointer;
  transition: all 0.2s;
}

.record-btn:hover {
  border-color: #10b981;
  color: #10b981;
}

.record-btn.recording {
  background: #ef4444;
  border-color: #ef4444;
  color: white;
}

.hotkey-error {
  font-size: 12px;
  color: #ef4444;
  margin: 0;
}

/* Language */
.language-select {
  width: 100%;
  padding: 10px 12px;
  background: #171717;
  border: 1px solid #262626;
  border-radius: 8px;
  color: #e5e5e5;
  font-size: 13px;
  appearance: none;
  cursor: pointer;
}

.language-select:focus {
  outline: none;
  border-color: #10b981;
}

/* Footer */
.settings-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 16px 28px;
  border-top: 1px solid #262626;
}

.btn {
  padding: 8px 20px;
  border-radius: 8px;
  border: none;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-secondary {
  background: #171717;
  color: #e5e5e5;
  border: 1px solid #262626;
}

.btn-secondary:hover {
  border-color: #404040;
}

.btn-primary {
  background: #10b981;
  color: white;
}

.btn-primary:hover {
  filter: brightness(1.1);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
