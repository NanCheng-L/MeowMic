<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { open } from '@tauri-apps/plugin-shell'
import { useAudioEngine } from '../composables/useAudioEngine'

const { t } = useI18n()
const { installVBCable } = useAudioEngine()

const props = defineProps<{
  inputDevices: string[]
  outputDevices: string[]
  inputValue: string
  outputValue: string
  disabled?: boolean
}>()

const emit = defineEmits<{
  'update:inputValue': [value: string]
  'update:outputValue': [value: string]
  refresh: []
  'install-success': []
}>()

const hasVirtualCable = computed(() => {
  return props.outputDevices.some(d =>
    d.toLowerCase().includes('cable') || d.toLowerCase().includes('vb-audio')
  )
})

// 检测输入输出是否是同一物理设备（同一 USB 设备会导致 WASAPI 读取失败）
const sameDeviceWarning = computed(() => {
  if (!props.inputValue || !props.outputValue) return false
  // 提取括号里的型号名，如 "麦克风 (K7)" -> "k7"
  const extractModel = (name: string) => {
    const match = name.match(/\((.+?)\)/)
    return match ? match[1].trim().toLowerCase() : ''
  }
  const inputModel = extractModel(props.inputValue)
  const outputModel = extractModel(props.outputValue)
  // 型号相同且不为空，且不是 VB-Cable
  return inputModel && inputModel === outputModel && !inputModel.includes('cable')
})

const installing = ref(false)
const installError = ref('')

const translateError = (msg: string): string => {
  if (msg.includes('timed out')) return t('device.installTimeout')
  if (msg.includes('not found')) return t('device.installNotFound')
  if (msg.includes('UAC') || msg.includes('denied')) return t('device.installDenied')
  return t('device.installFailed')
}

const handleInstallVBCable = async () => {
  installing.value = true
  installError.value = ''
  try {
    await installVBCable()
    emit('install-success')
    emit('refresh')
  } catch (e: any) {
    installError.value = translateError(e?.toString() || '')
  } finally {
    installing.value = false
  }
}

const openVBCableWebsite = async () => {
  await open('https://vb-audio.com/Cable/')
}
</script>

<template>
  <div class="device-selector" :class="{ disabled }">
    <div class="selector-row">
      <label>{{ t('device.input') }}</label>
      <div class="select-wrapper">
        <select
          :value="inputValue"
          @change="emit('update:inputValue', ($event.target as HTMLSelectElement).value)"
          :disabled="disabled"
        >
          <option value="" disabled>{{ t('device.selectInput') }}</option>
          <option v-for="device in inputDevices" :key="device" :value="device">
            {{ device }}
          </option>
        </select>
        <span class="select-arrow"></span>
      </div>
    </div>

    <div class="selector-row">
      <label>{{ t('device.output') }}</label>
      <div class="select-wrapper">
        <select
          :value="outputValue"
          @change="emit('update:outputValue', ($event.target as HTMLSelectElement).value)"
          :disabled="disabled"
        >
          <option value="" disabled>{{ t('device.selectOutput') }}</option>
          <option v-for="device in outputDevices" :key="device" :value="device">
            {{ device }}
          </option>
        </select>
        <span class="select-arrow"></span>
      </div>
    </div>

    <button class="refresh-btn" @click="emit('refresh')" :disabled="disabled">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M23 4v6h-6M1 20v-6h6"/>
        <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
      </svg>
    </button>

    <p v-if="inputDevices.length === 0 && !disabled" class="empty-hint">
      {{ t('device.noInput') }}
    </p>

    <p v-if="sameDeviceWarning" class="same-device-warning">
      {{ t('device.sameDeviceWarning') }}
    </p>

    <div v-if="!hasVirtualCable && outputDevices.length > 0 && !disabled" class="vb-warning">
      <span>{{ t('device.noVirtualCable') }}</span>
      <div class="vb-actions">
        <a v-if="!installing" href="#" @click.prevent="handleInstallVBCable">{{ t('device.installVBCable') }}</a>
        <span v-else class="installing-hint">{{ t('device.installing') }}</span>
        <span class="vb-divider">|</span>
        <a href="#" @click.prevent="openVBCableWebsite">{{ t('device.downloadVBCable') }}</a>
      </div>
      <div v-if="installError" class="install-error">{{ installError }}</div>
    </div>
  </div>
</template>

<style scoped>
.device-selector {
  display: flex;
  flex-direction: column;
  gap: 12px;
  position: relative;
}

.device-selector.disabled {
  opacity: 0.5;
  pointer-events: none;
}

.selector-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.selector-row label {
  font-size: 12px;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.select-wrapper {
  position: relative;
}

select {
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

select:hover:not(:disabled) {
  border-color: var(--text-muted);
}

select:focus {
  outline: none;
  border-color: var(--accent);
}

select:disabled {
  cursor: not-allowed;
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

.refresh-btn {
  position: absolute;
  right: 0;
  top: 0;
  padding: 6px;
  background: transparent;
  border: none;
  border-radius: 6px;
  color: var(--text-muted);
  cursor: pointer;
  transition: all 0.2s;
}

.refresh-btn:hover:not(:disabled) {
  background: var(--surface);
  color: var(--text-primary);
}

.refresh-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.empty-hint {
  font-size: 12px;
  color: var(--warning);
  margin: 0;
}

.vb-warning {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  background: rgba(245, 158, 11, 0.1);
  border: 1px solid rgba(245, 158, 11, 0.3);
  border-radius: 8px;
  font-size: 12px;
  color: var(--warning);
}

.vb-warning a {
  color: var(--accent);
  text-decoration: none;
}

.vb-warning a:hover {
  text-decoration: underline;
}

.installing-hint {
  color: var(--accent);
  animation: pulse 1.5s ease-in-out infinite;
}

.install-error {
  color: #ef4444;
  font-size: 11px;
  width: 100%;
}

.same-device-warning {
  font-size: 12px;
  color: #f59e0b;
  margin: 0;
  padding: 8px 12px;
  background: rgba(245, 158, 11, 0.1);
  border: 1px solid rgba(245, 158, 11, 0.3);
  border-radius: 6px;
}

.vb-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.vb-divider {
  color: var(--text-muted);
  opacity: 0.4;
}
</style>
