import { ref, watchEffect, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

type Theme = 'dark' | 'light'

const STORAGE_KEY = 'meowmic-theme'

// 模块级共享状态
const theme = ref<Theme>((localStorage.getItem(STORAGE_KEY) as Theme) || 'light')

function applyTheme(t: Theme) {
  const root = document.documentElement
  if (t === 'light') {
    root.dataset.theme = 'light'
  } else {
    delete root.dataset.theme
  }
  // 同步更新窗口标题栏 Acrylic 效果颜色
  invoke('set_window_theme', { theme: t }).catch(() => {})
}

// 初始化：立即应用
applyTheme(theme.value)

// 持久化 + 应用
watchEffect(() => {
  const t = theme.value
  localStorage.setItem(STORAGE_KEY, t)
  applyTheme(t)
})

export function useTheme() {
  // 跨窗口同步
  const handleStorage = (e: StorageEvent) => {
    if (e.key === STORAGE_KEY && e.newValue) {
      theme.value = e.newValue as Theme
    }
  }

  onMounted(() => {
    window.addEventListener('storage', handleStorage)
  })

  onUnmounted(() => {
    window.removeEventListener('storage', handleStorage)
  })

  const setTheme = (t: Theme) => {
    theme.value = t
  }

  const toggleTheme = () => {
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
  }

  return {
    theme,
    setTheme,
    toggleTheme,
  }
}
