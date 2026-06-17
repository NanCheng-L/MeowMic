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
const { saveSettings, registerHotkey } = useSettings()
const { theme, setTheme } = useTheme()

// 暂存的配置
const localConfig = ref({
  selectedModel: 'RNNoise',
  hotkey: 'Ctrl+Shift+D',
  hotkeyEnabled: true,
  hotkeyExplode: '',
  hotkeyExplodeEnabled: false,
  hotkeyMonitor: '',
  hotkeyMonitorEnabled: false,
  hotkeyBgm: '',
  hotkeyBgmEnabled: false,
  hotkeyEq: '',
  hotkeyEqEnabled: false,
  autostart: false,
  language: 'zh-CN',
})

// 选项数据
const availableModels = ref<string[]>([])

const recording = ref<string | null>(null) // 当前正在录制的快捷键名称
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
  const code = e.code
  if (!['Control', 'Shift', 'Alt', 'Meta'].includes(key)) {
    if (key === ' ') parts.push('Space')
    else if (code === 'Numpad0') parts.push('Numpad0')
    else if (code === 'Numpad1') parts.push('Numpad1')
    else if (code === 'Numpad2') parts.push('Numpad2')
    else if (code === 'Numpad3') parts.push('Numpad3')
    else if (code === 'Numpad4') parts.push('Numpad4')
    else if (code === 'Numpad5') parts.push('Numpad5')
    else if (code === 'Numpad6') parts.push('Numpad6')
    else if (code === 'Numpad7') parts.push('Numpad7')
    else if (code === 'Numpad8') parts.push('Numpad8')
    else if (code === 'Numpad9') parts.push('Numpad9')
    else if (code === 'NumpadAdd') parts.push('Add')
    else if (code === 'NumpadSubtract') parts.push('Subtract')
    else if (code === 'NumpadMultiply') parts.push('Multiply')
    else if (code === 'NumpadDivide') parts.push('Divide')
    else if (code === 'NumpadDecimal') parts.push('Decimal')
    else if (code === 'NumpadEnter') parts.push('Enter')
    else if (key.length === 1) parts.push(key.toUpperCase())
    else parts.push(key)
  }
  return parts.join('+')
}

const startRecording = (hotkeyName: string) => {
  recording.value = hotkeyName
  hotkeyError.value = ''
  recordedKeys = []
  keydownHandler = (e: KeyboardEvent) => {
    e.preventDefault()
    e.stopPropagation()
    // Escape 重置快捷键
    if (e.key === 'Escape') {
      resetHotkey(hotkeyName)
      return
    }
    recordedKeys = [formatKey(e)]
  }
  keyupHandler = (e: KeyboardEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (recordedKeys.length > 0) {
      const key = recordedKeys[0]
      switch (hotkeyName) {
        case 'hotkey':
          localConfig.value.hotkey = key
          break
        case 'hotkeyExplode':
          localConfig.value.hotkeyExplode = key
          localConfig.value.hotkeyExplodeEnabled = true
          break
        case 'hotkeyMonitor':
          localConfig.value.hotkeyMonitor = key
          localConfig.value.hotkeyMonitorEnabled = true
          break
        case 'hotkeyBgm':
          localConfig.value.hotkeyBgm = key
          localConfig.value.hotkeyBgmEnabled = true
          break
        case 'hotkeyEq':
          localConfig.value.hotkeyEq = key
          localConfig.value.hotkeyEqEnabled = true
          break
      }
      recording.value = null
      stopRecording()
    }
  }
  window.addEventListener('keydown', keydownHandler, true)
  window.addEventListener('keyup', keyupHandler, true)
}

const resetHotkey = (hotkeyName: string) => {
  switch (hotkeyName) {
    case 'hotkey':
      localConfig.value.hotkey = 'Ctrl+Shift+D'
      localConfig.value.hotkeyEnabled = true
      break
    case 'hotkeyExplode':
      localConfig.value.hotkeyExplode = ''
      localConfig.value.hotkeyExplodeEnabled = false
      break
    case 'hotkeyMonitor':
      localConfig.value.hotkeyMonitor = ''
      localConfig.value.hotkeyMonitorEnabled = false
      break
    case 'hotkeyBgm':
      localConfig.value.hotkeyBgm = ''
      localConfig.value.hotkeyBgmEnabled = false
      break
    case 'hotkeyEq':
      localConfig.value.hotkeyEq = ''
      localConfig.value.hotkeyEqEnabled = false
      break
  }
  recording.value = null
  stopRecording()
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
  recording.value = null
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
    localConfig.value.selectedModel = p.selectedModel || 'RNNoise'
    localConfig.value.hotkey = p.hotkey || 'Ctrl+Shift+D'
    localConfig.value.hotkeyEnabled = p.hotkeyEnabled ?? true
    localConfig.value.hotkeyExplode = p.hotkeyExplode || ''
    localConfig.value.hotkeyExplodeEnabled = p.hotkeyExplodeEnabled ?? false
    localConfig.value.hotkeyMonitor = p.hotkeyMonitor || ''
    localConfig.value.hotkeyMonitorEnabled = p.hotkeyMonitorEnabled ?? false
    localConfig.value.hotkeyBgm = p.hotkeyBgm || ''
    localConfig.value.hotkeyBgmEnabled = p.hotkeyBgmEnabled ?? false
    localConfig.value.hotkeyEq = p.hotkeyEq || ''
    localConfig.value.hotkeyEqEnabled = p.hotkeyEqEnabled ?? false
    localConfig.value.autostart = p.autostart || false
    localConfig.value.language = p.language || 'zh-CN'
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

  // 1. 保存后端设置
  try {
    await saveSettings({
      hotkey: localConfig.value.hotkey,
      hotkeyEnabled: localConfig.value.hotkeyEnabled,
      hotkeyExplode: localConfig.value.hotkeyExplode,
      hotkeyExplodeEnabled: localConfig.value.hotkeyExplodeEnabled,
      hotkeyMonitor: localConfig.value.hotkeyMonitor,
      hotkeyMonitorEnabled: localConfig.value.hotkeyMonitorEnabled,
      hotkeyBgm: localConfig.value.hotkeyBgm,
      hotkeyBgmEnabled: localConfig.value.hotkeyBgmEnabled,
      hotkeyEq: localConfig.value.hotkeyEq,
      hotkeyEqEnabled: localConfig.value.hotkeyEqEnabled,
      autostart: localConfig.value.autostart,
      language: localConfig.value.language,
    })
  } catch (e) {
    console.error('Failed to save settings:', e)
    saving.value = false
    return
  }

  // 2. 注册快捷键
  try {
    await registerHotkey(
      localConfig.value.hotkeyEnabled ? localConfig.value.hotkey : '',
      localConfig.value.hotkeyExplodeEnabled ? localConfig.value.hotkeyExplode : '',
      localConfig.value.hotkeyMonitorEnabled ? localConfig.value.hotkeyMonitor : '',
      localConfig.value.hotkeyBgmEnabled ? localConfig.value.hotkeyBgm : '',
      localConfig.value.hotkeyEqEnabled ? localConfig.value.hotkeyEq : '',
    )
  } catch (e) {
    hotkeyError.value = t('settings.hotkeyError')
  }

  // 3. 语言存 localStorage（主窗口下次启动时读取）
  localStorage.setItem('meowmic-lang', localConfig.value.language)

  // 4. 模型变更存 localStorage，主窗口轮询检测
  localStorage.setItem('meowmic-pending', JSON.stringify({
    selectedModel: localConfig.value.selectedModel,
    applyAt: Date.now(),
  }))

  saving.value = false
  // 通知主窗口重新读取设置
  try {
    const { emit } = await import('@tauri-apps/api/event')
    await emit('settings-saved')
  } catch {}
  getCurrentWindow().hide()
}

// 语言即时切换
watch(() => localConfig.value.language, (lang) => {
  setLocale(lang)
  localStorage.setItem('meowmic-lang', lang)
})

const handleClose = async () => {
  // 通知主窗口重新读取设置
  try {
    const { emit } = await import('@tauri-apps/api/event')
    await emit('settings-saved')
  } catch {}
  getCurrentWindow().hide()
}
</script>

<template>
  <div class="settings-page">
    <div class="settings-header">
      <h1>{{ t('settings.title') }}</h1>
    </div>

    <div class="settings-body">
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

      <!-- 快捷键 -->
      <section class="section">
        <h2>{{ t('settings.hotkey') }}</h2>

        <!-- 开关降噪 -->
        <div class="hotkey-row">
          <div class="hotkey-info">
            <span class="hotkey-label">{{ t('settings.hotkeyDenoise') }}</span>
            <span class="hotkey-value">{{ localConfig.hotkey || t('settings.hotkeyDefault') }}</span>
          </div>
          <div class="hotkey-actions">
            <button
              class="toggle small"
              :class="{ active: localConfig.hotkeyEnabled }"
              @click="localConfig.hotkeyEnabled = !localConfig.hotkeyEnabled"
            >
              <span class="toggle-knob"></span>
            </button>
            <button
              class="reset-btn"
              @click="resetHotkey('hotkey')"
            >{{ t('settings.reset') }}</button>
            <button
              class="record-btn"
              :class="{ recording: recording === 'hotkey' }"
              @click="recording === 'hotkey' ? stopRecording() : startRecording('hotkey')"
            >
              {{ recording === 'hotkey' ? t('settings.cancel') : t('settings.record') }}
            </button>
          </div>
        </div>

        <!-- 开关EQ -->
        <div class="hotkey-row">
          <div class="hotkey-info">
            <span class="hotkey-label">{{ t('settings.hotkeyEq') }}</span>
            <span class="hotkey-value">{{ localConfig.hotkeyEq || t('settings.hotkeyDefault') }}</span>
          </div>
          <div class="hotkey-actions">
            <button
              class="toggle small"
              :class="{ active: localConfig.hotkeyEqEnabled }"
              @click="localConfig.hotkeyEqEnabled = !localConfig.hotkeyEqEnabled"
            >
              <span class="toggle-knob"></span>
            </button>
            <button
              class="reset-btn"
              @click="resetHotkey('hotkeyEq')"
            >{{ t('settings.reset') }}</button>
            <button
              class="record-btn"
              :class="{ recording: recording === 'hotkeyEq' }"
              @click="recording === 'hotkeyEq' ? stopRecording() : startRecording('hotkeyEq')"
            >
              {{ recording === 'hotkeyEq' ? t('settings.cancel') : t('settings.record') }}
            </button>
          </div>
        </div>

        <!-- 开关BGM -->
        <div class="hotkey-row">
          <div class="hotkey-info">
            <span class="hotkey-label">{{ t('settings.hotkeyBgm') }}</span>
            <span class="hotkey-value">{{ localConfig.hotkeyBgm || t('settings.hotkeyDefault') }}</span>
          </div>
          <div class="hotkey-actions">
            <button
              class="toggle small"
              :class="{ active: localConfig.hotkeyBgmEnabled }"
              @click="localConfig.hotkeyBgmEnabled = !localConfig.hotkeyBgmEnabled"
            >
              <span class="toggle-knob"></span>
            </button>
            <button
              class="reset-btn"
              @click="resetHotkey('hotkeyBgm')"
            >{{ t('settings.reset') }}</button>
            <button
              class="record-btn"
              :class="{ recording: recording === 'hotkeyBgm' }"
              @click="recording === 'hotkeyBgm' ? stopRecording() : startRecording('hotkeyBgm')"
            >
              {{ recording === 'hotkeyBgm' ? t('settings.cancel') : t('settings.record') }}
            </button>
          </div>
        </div>

        <!-- 开关炸麦 -->
        <div class="hotkey-row">
          <div class="hotkey-info">
            <span class="hotkey-label">{{ t('settings.hotkeyExplode') }}</span>
            <span class="hotkey-value">{{ localConfig.hotkeyExplode || t('settings.hotkeyDefault') }}</span>
          </div>
          <div class="hotkey-actions">
            <button
              class="toggle small"
              :class="{ active: localConfig.hotkeyExplodeEnabled }"
              @click="localConfig.hotkeyExplodeEnabled = !localConfig.hotkeyExplodeEnabled"
            >
              <span class="toggle-knob"></span>
            </button>
            <button
              class="reset-btn"
              @click="resetHotkey('hotkeyExplode')"
            >{{ t('settings.reset') }}</button>
            <button
              class="record-btn"
              :class="{ recording: recording === 'hotkeyExplode' }"
              @click="recording === 'hotkeyExplode' ? stopRecording() : startRecording('hotkeyExplode')"
            >
              {{ recording === 'hotkeyExplode' ? t('settings.cancel') : t('settings.record') }}
            </button>
          </div>
        </div>

        <!-- 开关监听 -->
        <div class="hotkey-row">
          <div class="hotkey-info">
            <span class="hotkey-label">{{ t('settings.hotkeyMonitor') }}</span>
            <span class="hotkey-value">{{ localConfig.hotkeyMonitor || t('settings.hotkeyDefault') }}</span>
          </div>
          <div class="hotkey-actions">
            <button
              class="toggle small"
              :class="{ active: localConfig.hotkeyMonitorEnabled }"
              @click="localConfig.hotkeyMonitorEnabled = !localConfig.hotkeyMonitorEnabled"
            >
              <span class="toggle-knob"></span>
            </button>
            <button
              class="reset-btn"
              @click="resetHotkey('hotkeyMonitor')"
            >{{ t('settings.reset') }}</button>
            <button
              class="record-btn"
              :class="{ recording: recording === 'hotkeyMonitor' }"
              @click="recording === 'hotkeyMonitor' ? stopRecording() : startRecording('hotkeyMonitor')"
            >
              {{ recording === 'hotkeyMonitor' ? t('settings.cancel') : t('settings.record') }}
            </button>
          </div>
        </div>

        <p v-if="hotkeyError" class="hotkey-error">{{ hotkeyError }}</p>
        <p class="hotkey-hint">{{ t('settings.hotkeyHint') }}</p>
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

.toggle.small {
  width: 36px;
  height: 20px;
}

.toggle.small .toggle-knob {
  width: 16px;
  height: 16px;
}

.toggle.small.active .toggle-knob {
  transform: translateX(16px);
}

/* Reset button */
.reset-btn {
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

.reset-btn:hover {
  border-color: var(--accent);
  color: var(--accent);
}

/* Hotkey rows */
.hotkey-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 0;
  border-bottom: 1px solid var(--border);
}

.hotkey-row:last-child {
  border-bottom: none;
}

.hotkey-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.hotkey-label {
  font-size: 14px;
  color: var(--text-primary);
}

.hotkey-value {
  font-family: 'DM Mono', monospace;
  font-size: 12px;
  color: var(--text-muted);
}

.hotkey-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.hotkey-hint {
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 8px;
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
