import { invoke } from '@tauri-apps/api/core'
import { ref } from 'vue'

interface AppSettings {
  hotkey: string
  hotkeyEnabled: boolean
  hotkeyExplode: string
  hotkeyExplodeEnabled: boolean
  hotkeyMonitor: string
  hotkeyMonitorEnabled: boolean
  hotkeyBgm: string
  hotkeyBgmEnabled: boolean
  hotkeyEq: string
  hotkeyEqEnabled: boolean
  autostart: boolean
  language: string
}

const settings = ref<AppSettings>({
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

const loading = ref(false)

export function useSettings() {
  const loadSettings = async () => {
    loading.value = true
    try {
      const result = await invoke<AppSettings>('get_settings')
      settings.value = result
    } catch (e) {
      console.error('Failed to load settings:', e)
    } finally {
      loading.value = false
    }
  }

  const saveSettings = async (newSettings: AppSettings) => {
    try {
      await invoke('save_settings', { settings: newSettings })
      settings.value = { ...newSettings }
    } catch (e) {
      console.error('Failed to save settings:', e)
      throw e
    }
  }

  const registerHotkey = async (
    hotkey: string,
    hotkeyExplode: string = '',
    hotkeyMonitor: string = '',
    hotkeyBgm: string = '',
    hotkeyEq: string = '',
  ) => {
    await invoke('register_hotkey', {
      hotkey,
      hotkeyExplode,
      hotkeyMonitor,
      hotkeyBgm,
      hotkeyEq,
    })
  }

  const unregisterHotkey = async () => {
    await invoke('unregister_hotkey')
  }

  return {
    settings,
    loading,
    loadSettings,
    saveSettings,
    registerHotkey,
    unregisterHotkey,
  }
}
