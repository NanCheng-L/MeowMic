<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{
  enabled: boolean
}>()

const emit = defineEmits<{
  'update:enabled': [value: boolean]
  'open-eq': []
}>()
</script>

<template>
  <div class="eq-control">
    <div class="eq-left">
      <svg class="eq-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <line x1="4" y1="21" x2="4" y2="14"/>
        <line x1="4" y1="10" x2="4" y2="3"/>
        <line x1="12" y1="21" x2="12" y2="12"/>
        <line x1="12" y1="8" x2="12" y2="3"/>
        <line x1="20" y1="21" x2="20" y2="16"/>
        <line x1="20" y1="12" x2="20" y2="3"/>
        <line x1="1" y1="14" x2="7" y2="14"/>
        <line x1="9" y1="8" x2="15" y2="8"/>
        <line x1="17" y1="16" x2="23" y2="16"/>
      </svg>
      <span class="eq-label">{{ t('eq.label') }}</span>
    </div>
    <div class="eq-right">
      <button class="eq-open-btn" @click="emit('open-eq')">
        {{ t('eq.open') }}
      </button>
      <button
        class="toggle"
        :class="{ active: enabled }"
        @click="emit('update:enabled', !enabled)"
      >
        <span class="toggle-knob"></span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.eq-control {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.eq-left {
  display: flex;
  align-items: center;
  gap: 6px;
}

.eq-icon {
  color: var(--text-muted);
}

.eq-label {
  font-size: 13px;
  color: var(--text-primary);
}

.eq-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.eq-open-btn {
  padding: 4px 10px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--text-muted);
  font-size: 11px;
  cursor: pointer;
  transition: all 0.2s;
}

.eq-open-btn:hover {
  border-color: var(--accent);
  color: var(--accent);
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
</style>
