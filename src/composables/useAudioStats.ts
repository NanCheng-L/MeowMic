import { ref, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

interface AudioStats {
  input_level: number
  output_level: number
  noise_reduction_db: number
  latency_ms: number
  cpu_usage: number
  frames_processed: number
  spectrum: number[]
  frames_dropped: number
}

// 复用对象，减少每 100ms 的堆分配
const sharedStats: AudioStats = {
  input_level: -100,
  output_level: -100,
  noise_reduction_db: 0,
  latency_ms: 0,
  cpu_usage: 0,
  frames_processed: 0,
  spectrum: [],
  frames_dropped: 0,
}

export function useAudioStats() {
  const stats = ref<AudioStats>({ ...sharedStats })

  let pollingInterval: ReturnType<typeof setInterval> | null = null

  const startPolling = (interval = 200) => {
    stopPolling()
    pollingInterval = setInterval(async () => {
      try {
        const newStats = await invoke<AudioStats>('get_audio_stats')
        // 原地更新，不替换整个 ref（spectrum 是整体替换的数组，触发浅比较）
        sharedStats.input_level = newStats.input_level
        sharedStats.output_level = newStats.output_level
        sharedStats.noise_reduction_db = newStats.noise_reduction_db
        sharedStats.latency_ms = newStats.latency_ms
        sharedStats.cpu_usage = newStats.cpu_usage
        sharedStats.frames_processed = newStats.frames_processed
        sharedStats.frames_dropped = newStats.frames_dropped
        // spectrum 是新数组（来自 JSON parse），直接替换
        sharedStats.spectrum = newStats.spectrum
        // 触发 Vue 响应式
        stats.value = { ...sharedStats }
      } catch (e) {
        console.error('Failed to get audio stats:', e)
      }
    }, interval)
  }

  const stopPolling = () => {
    if (pollingInterval) {
      clearInterval(pollingInterval)
      pollingInterval = null
    }
  }

  onUnmounted(() => {
    stopPolling()
  })

  return {
    stats,
    startPolling,
    stopPolling,
  }
}
