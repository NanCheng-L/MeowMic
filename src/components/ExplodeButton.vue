<script setup lang="ts">
import { ref, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const isExploding = ref(false)
const countdown = ref(0)
let timer: ReturnType<typeof setInterval> | null = null

const toggle = async () => {
  if (isExploding.value) {
    stopExplode()
  } else {
    startExplode()
  }
}

const startExplode = async () => {
  isExploding.value = true
  countdown.value = 10
  await invoke('set_explode_mode', { enabled: true })

  timer = setInterval(() => {
    countdown.value--
    if (countdown.value <= 0) {
      stopExplode()
    }
  }, 1000)
}

const stopExplode = async () => {
  isExploding.value = false
  countdown.value = 0
  if (timer) {
    clearInterval(timer)
    timer = null
  }
  await invoke('set_explode_mode', { enabled: false })
}

onUnmounted(() => {
  if (timer) clearInterval(timer)
  if (isExploding.value) {
    invoke('set_explode_mode', { enabled: false }).catch(() => {})
  }
})
</script>

<template>
  <div class="explode-section">
    <div class="explode-header">
      <span class="explode-label">恶搞模式</span>
      <span class="explode-hint">⚠️ 请勿长时间使用</span>
    </div>
    <button
      class="explode-btn"
      :class="{ active: isExploding }"
      @click="toggle"
    >
      <span class="explode-icon">💥</span>
      <span class="explode-text">{{ isExploding ? '关闭炸麦' : '一键炸麦' }}</span>
      <span v-if="isExploding" class="countdown">{{ countdown }}s</span>
    </button>
  </div>
</template>

<style scoped>
.explode-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.explode-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.explode-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.explode-hint {
  font-size: 11px;
  color: var(--text-muted);
}

.explode-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: 100%;
  padding: 14px 16px;
  border: 2px solid #ef4444;
  border-radius: 10px;
  background: linear-gradient(135deg, #1a1a2e, #16213e);
  color: #ef4444;
  font-size: 15px;
  font-weight: 700;
  cursor: pointer;
  transition: all 0.2s;
  position: relative;
  overflow: hidden;
}

.explode-btn:hover {
  background: linear-gradient(135deg, #1a1a2e, #1e1024);
  border-color: #f87171;
  box-shadow: 0 0 20px rgba(239, 68, 68, 0.3);
}

.explode-btn.active {
  background: linear-gradient(135deg, #7f1d1d, #991b1b);
  border-color: #fca5a5;
  color: #fca5a5;
  animation: shake 0.1s infinite;
  box-shadow: 0 0 30px rgba(239, 68, 68, 0.5);
}

.explode-icon {
  font-size: 20px;
}

.explode-text {
  letter-spacing: 1px;
}

.countdown {
  background: rgba(0, 0, 0, 0.4);
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 13px;
  font-weight: 600;
  font-family: 'DM Mono', monospace;
  color: #fca5a5;
}

@keyframes shake {
  0%, 100% { transform: translateX(0); }
  25% { transform: translateX(-3px) rotate(-1deg); }
  50% { transform: translateX(3px) rotate(1deg); }
  75% { transform: translateX(-2px) rotate(-0.5deg); }
}
</style>
