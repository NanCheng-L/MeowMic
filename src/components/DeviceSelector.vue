<script setup lang="ts">
import { computed } from 'vue'
import { open } from '@tauri-apps/plugin-shell'

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
}>()

const hasVirtualCable = computed(() => {
  return props.outputDevices.some(d =>
    d.toLowerCase().includes('cable') || d.toLowerCase().includes('vb-audio')
  )
})

const openVBCableDownload = async () => {
  await open('https://vb-audio.com/Cable/')
}
</script>

<template>
  <div class="device-selector" :class="{ disabled }">
    <div class="selector-row">
      <label>输入设备</label>
      <div class="select-wrapper">
        <select
          :value="inputValue"
          @change="emit('update:inputValue', ($event.target as HTMLSelectElement).value)"
          :disabled="disabled"
        >
          <option value="" disabled>选择麦克风</option>
          <option v-for="device in inputDevices" :key="device" :value="device">
            {{ device }}
          </option>
        </select>
        <span class="select-arrow"></span>
      </div>
    </div>

    <div class="selector-row">
      <label>输出设备</label>
      <div class="select-wrapper">
        <select
          :value="outputValue"
          @change="emit('update:outputValue', ($event.target as HTMLSelectElement).value)"
          :disabled="disabled"
        >
          <option value="" disabled>选择输出</option>
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
      未检测到输入设备
    </p>

    <div v-if="!hasVirtualCable && outputDevices.length > 0 && !disabled" class="vb-warning">
      <span>未检测到虚拟音频设备</span>
      <a href="#" @click.prevent="openVBCableDownload">点击安装 VB-Cable</a>
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
</style>
