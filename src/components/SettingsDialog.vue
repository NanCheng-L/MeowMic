<script setup lang="ts">
import { ref, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettings } from '../composables/useSettings'
import { setLocale, availableLocales } from '../locales'

const { t, locale } = useI18n()

defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const { settings, saveSettings, registerHotkey, unregisterHotkey } = useSettings()

const localSettings = ref({
  hotkey: settings.value.hotkey,
  hotkeyEnabled: settings.value.hotkeyEnabled,
  autostart: settings.value.autostart,
  language: settings.value.language,
})

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
        localSettings.value.hotkey = recordedKeys[0]
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

const handleSave = async () => {
  saving.value = true
  hotkeyError.value = ''
  try {
    await saveSettings({ ...localSettings.value })
  } catch (e) {
    console.error('Failed to save settings:', e)
    saving.value = false
    return
  }

  try {
    if (localSettings.value.hotkeyEnabled) {
      await registerHotkey(localSettings.value.hotkey)
    } else {
      await unregisterHotkey()
    }
    emit('close')
  } catch (e) {
    hotkeyError.value = t('settings.hotkeyError')
  } finally {
    saving.value = false
  }
}

const handleClose = () => {
  localSettings.value = {
    hotkey: settings.value.hotkey,
    hotkeyEnabled: settings.value.hotkeyEnabled,
    autostart: settings.value.autostart,
    language: settings.value.language,
  }
  hotkeyError.value = ''
  emit('close')
}

onUnmounted(() => {
  stopRecording()
})
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="open" class="overlay" @click.self="handleClose">
        <Transition name="slide">
          <div v-if="open" class="dialog">
            <div class="dialog-header">
              <h2>{{ t('settings.title') }}</h2>
              <button class="close-btn" @click="handleClose">
                <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="4" y1="4" x2="12" y2="12"/>
                  <line x1="12" y1="4" x2="4" y2="12"/>
                </svg>
              </button>
            </div>

            <div class="dialog-body">
              <!-- 快捷键 -->
              <div class="setting-group">
                <div class="setting-header">
                  <span class="setting-label">{{ t('settings.hotkey') }}</span>
                  <button
                    class="toggle"
                    :class="{ active: localSettings.hotkeyEnabled }"
                    @click="localSettings.hotkeyEnabled = !localSettings.hotkeyEnabled"
                  >
                    <span class="toggle-knob"></span>
                  </button>
                </div>
                <p class="setting-desc">{{ t('settings.hotkeyDesc') }}</p>

                <div class="hotkey-section">
                  <div class="hotkey-display">
                    <span v-if="!recording" class="hotkey-value">{{ localSettings.hotkey }}</span>
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
              </div>

              <!-- 开机自启 -->
              <div class="setting-group">
                <div class="setting-header">
                  <span class="setting-label">{{ t('settings.autostart') }}</span>
                  <button
                    class="toggle"
                    :class="{ active: localSettings.autostart }"
                    @click="localSettings.autostart = !localSettings.autostart"
                  >
                    <span class="toggle-knob"></span>
                  </button>
                </div>
                <p class="setting-desc">{{ t('settings.autostartDesc') }}</p>
              </div>

              <!-- 自动更新占位 -->
              <div class="setting-group disabled">
                <div class="setting-header">
                  <span class="setting-label">{{ t('settings.autoUpdate') }}</span>
                  <span class="coming-soon">{{ t('settings.comingSoon') }}</span>
                </div>
                <p class="setting-desc">{{ t('settings.autoUpdateDesc') }}</p>
              </div>

              <!-- 语言切换 -->
              <div class="setting-group">
                <div class="setting-header">
                  <span class="setting-label">{{ t('settings.language') }}</span>
                  <select
                    class="language-select"
                    :value="locale"
                    @change="setLocale(($event.target as HTMLSelectElement).value)"
                  >
                    <option v-for="l in availableLocales" :key="l.value" :value="l.value">
                      {{ l.label }}
                    </option>
                  </select>
                </div>
              </div>
            </div>

            <div class="dialog-footer">
              <button class="btn btn-secondary" @click="handleClose">{{ t('settings.cancel') }}</button>
              <button class="btn btn-primary" @click="handleSave" :disabled="saving">
                {{ saving ? t('settings.saving') : t('settings.save') }}
              </button>
            </div>
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
  backdrop-filter: blur(4px);
}

.dialog {
  background: var(--surface);
  border-radius: 16px;
  border: 1px solid var(--border);
  width: 380px;
  max-height: 80vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border);
}

.dialog-header h2 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.close-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  transition: color 0.2s, background 0.2s;
}

.close-btn:hover {
  color: var(--text-primary);
  background: var(--border);
}

.dialog-body {
  padding: 20px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.setting-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.setting-group.disabled {
  opacity: 0.5;
  pointer-events: none;
}

.setting-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.setting-label {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.setting-desc {
  font-size: 12px;
  color: var(--text-muted);
  margin: 0;
}

/* Toggle switch */
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

/* Hotkey section */
.hotkey-section {
  display: flex;
  gap: 8px;
  align-items: center;
}

.hotkey-display {
  flex: 1;
  background: var(--bg);
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
  background: var(--bg);
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

/* Coming soon badge */
.coming-soon {
  font-size: 11px;
  color: var(--text-muted);
  background: var(--border);
  padding: 2px 8px;
  border-radius: 4px;
}

/* Footer */
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 16px 20px;
  border-top: 1px solid var(--border);
}

.btn {
  padding: 8px 16px;
  border-radius: 8px;
  border: none;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-secondary {
  background: var(--bg);
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

.language-select {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 6px 12px;
  font-size: 13px;
  color: var(--text-primary);
  cursor: pointer;
  outline: none;
}

.language-select:focus {
  border-color: var(--accent);
}

/* Transitions */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.slide-enter-active {
  transition: transform 0.2s ease, opacity 0.2s ease;
}

.slide-leave-active {
  transition: transform 0.15s ease, opacity 0.15s ease;
}

.slide-enter-from {
  transform: scale(0.95);
  opacity: 0;
}

.slide-leave-to {
  transform: scale(0.95);
  opacity: 0;
}
</style>
