<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettings } from '../composables/useSettings'
import { useTheme } from '../composables/useTheme'
import { availableLocales, setLocale } from '../locales'
import UpdateChecker from './UpdateChecker.vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()
const { saveSettings, registerHotkey, unregisterHotkey } = useSettings()
const { theme, setTheme } = useTheme()

// 暂存的配置
const localConfig = ref({
  selectedInput: '',
  selectedOutput: '',
  selectedModel: 'RNNoise',
  monitorEnabled: false,
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
  const savedLang = localStorage.getItem('meowmic-lang')
  if (savedLang) {
    setLocale(savedLang)
  }

  // 先直接加载模型列表（兜底，避免 settings-init 事件丢失）
  try {
    availableModels.value = await invoke<string[]>('list_denoise_models')
  } catch {}

  unlistenInit = await listen<any>('settings-init', (event) => {
    const p = event.payload
    localConfig.value.selectedInput = p.selectedInput || ''
    localConfig.value.selectedOutput = p.selectedOutput || ''
    localConfig.value.selectedModel = p.selectedModel || 'RNNoise'
    localConfig.value.monitorEnabled = p.monitorEnabled || false
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
    // 同步主题
    if (p.theme) {
      setTheme(p.theme)
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
  localStorage.setItem('meowmic-lang', localConfig.value.language)

  // 4. 设备/模型变更存 localStorage，主窗口轮询检测
  localStorage.setItem('meowmic-pending', JSON.stringify({
    selectedInput: localConfig.value.selectedInput,
    selectedOutput: localConfig.value.selectedOutput,
    selectedModel: localConfig.value.selectedModel,
    monitorEnabled: localConfig.value.monitorEnabled,
    applyAt: Date.now(),
  }))

  saving.value = false
  getCurrentWindow().hide()
}

// 语言即时切换
watch(() => localConfig.value.language, (lang) => {
  setLocale(lang)
  localStorage.setItem('meowmic-lang', lang)
})

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

      <!-- 主题 -->
      <section class="section">
        <h2>{{ t('settings.theme') }}</h2>
        <div class="theme-options">
          <button
            class="theme-card"
            :class="{ active: theme === 'light' }"
            @click="setTheme('light')"
          >
            <span class="theme-icon">☀️</span>
            <span class="theme-name">{{ t('settings.themeLight') }}</span>
          </button>
          <button
            class="theme-card"
            :class="{ active: theme === 'dark' }"
            @click="setTheme('dark')"
          >
            <span class="theme-icon">🌙</span>
            <span class="theme-name">{{ t('settings.themeDark') }}</span>
          </button>
        </div>
      </section>

      <!-- 检查更新 -->
      <UpdateChecker />
    </div>

    <div class="settings-footer">
      <span class="save-hint">{{ t('settings.saveHint') }}</span>
      <div class="footer-actions">
        <button class="btn btn-secondary" @click="handleClose">{{ t('settings.cancel') }}</button>
        <button class="btn btn-primary" @click="handleSave" :disabled="saving">
          {{ saving ? t('settings.saving') : t('settings.save') }}
        </button>
      </div>
    </div>
  </div>
</template>

<style>
html, body {
  margin: 0;
  padding: 0;
  overflow: hidden;
  height: 100vh;
  background: var(--bg);
  font-family: 'DM Sans', -apple-system, BlinkMacSystemFont, sans-serif;
}
</style>

<style scoped>
.settings-page {
  height: 100vh;
  background: var(--bg);
  color: var(--text-primary);
  display: flex;
  flex-direction: column;
}

.settings-header {
  padding: 24px 28px 16px;
  border-bottom: 1px solid var(--border);
  position: sticky;
  top: 0;
  z-index: 10;
  background: var(--bg);
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
  background: var(--bg);
}

.settings-body::-webkit-scrollbar-thumb {
  background: var(--border);
  border-radius: 3px;
}

.settings-body::-webkit-scrollbar-thumb:hover {
  background: var(--text-muted);
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
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border);
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-label {
  font-size: 13px;
  color: var(--text-muted);
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

.setting-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.setting-label {
  font-size: 14px;
  color: var(--text-primary);
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
  background: var(--accent);
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
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 12px;
  text-align: center;
}

.hotkey-value {
  font-family: 'DM Mono', monospace;
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
  letter-spacing: 0.5px;
}

.hotkey-recording {
  font-size: 13px;
  color: var(--accent);
  animation: pulse 1.5s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.record-btn {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
  cursor: pointer;
  transition: all 0.2s;
}

.record-btn:hover {
  border-color: var(--accent);
  color: var(--accent);
}

.record-btn.recording {
  background: var(--danger);
  border-color: var(--danger);
  color: white;
}

.hotkey-error {
  font-size: 12px;
  color: var(--danger);
  margin: 0;
}

/* Language */
.language-select {
  width: 100%;
  padding: 10px 12px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text-primary);
  font-size: 13px;
  appearance: none;
  cursor: pointer;
}

.language-select:focus {
  outline: none;
  border-color: var(--accent);
}

/* Footer */
.settings-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 28px;
  border-top: 1px solid var(--border);
}

.save-hint {
  font-size: 11px;
  color: var(--text-muted);
}

.footer-actions {
  display: flex;
  gap: 8px;
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
  background: var(--surface);
  color: var(--text-primary);
  border: 1px solid var(--border);
}

.btn-secondary:hover {
  border-color: var(--text-muted);
}

.btn-primary {
  background: var(--accent);
  color: white;
}

.btn-primary:hover {
  filter: brightness(1.1);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Theme selector */
.theme-options {
  display: flex;
  gap: 8px;
}

.theme-card {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 10px 12px;
  background: var(--surface);
  border: 1.5px solid var(--border);
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s;
  color: var(--text-primary);
  font-size: 13px;
}

.theme-card:hover {
  border-color: var(--text-muted);
}

.theme-card.active {
  border-color: var(--accent);
  background: var(--bg);
}

.theme-icon {
  font-size: 16px;
}

.theme-name {
  font-weight: 500;
}
</style>
