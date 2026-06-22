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
}

export function useAudioStats() {
  const stats = ref<AudioStats>({
    input_level: -100,
    output_level: -100,
    noise_reduction_db: 0,
    latency_ms: 0,
    cpu_usage: 0,
    frames_processed: 0,
    spectrum: [],
  })

  let pollingInterval: ReturnType<typeof setInterval> | null = null

  const startPolling = (interval = 100) => {
    stopPolling()
    pollingInterval = setInterval(async () => {
      try {
        const newStats = await invoke<AudioStats>('get_audio_stats')
        stats.value = newStats
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
