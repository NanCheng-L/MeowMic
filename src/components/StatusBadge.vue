<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  running: boolean
  latency: number
}>()

const latencyColor = computed(() => {
  if (props.latency < 50) return 'var(--success)'
  return 'var(--danger)'
})
</script>

<template>
  <div class="status-badge">
    <div class="status-dot" :class="{ running }"></div>
    <span class="status-text">{{ running ? t('status.running') : t('status.stopped') }}</span>
    <span class="latency" :style="{ color: latencyColor }" v-if="running">
      {{ latency.toFixed(0) }}ms
    </span>
  </div>
</template>

<style scoped>
.status-badge {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: var(--surface);
  border-radius: 20px;
  font-size: 12px;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-muted);
  transition: background 0.3s;
}

.status-dot.running {
  background: var(--accent);
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0%, 100% {
    opacity: 1;
    box-shadow: 0 0 0 0 rgba(16, 185, 129, 0.4);
  }
  50% {
    opacity: 0.8;
    box-shadow: 0 0 0 6px rgba(16, 185, 129, 0);
  }
}

.status-text {
  color: var(--text-primary);
  font-weight: 500;
}

.latency {
  font-family: 'DM Mono', monospace;
  font-weight: 500;
}
</style>
