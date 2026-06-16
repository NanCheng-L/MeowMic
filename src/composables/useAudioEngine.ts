import { invoke } from '@tauri-apps/api/core'

export function useAudioEngine() {
  const startDenoising = async (inputDevice?: string, outputDevice?: string, model?: string, monitorEnabled?: boolean) => {
    await invoke('start_denoising', { inputDevice, outputDevice, model, monitorEnabled })
  }

  const stopDenoising = async () => {
    await invoke('stop_denoising')
  }

  const updateConfig = async (config: { enabled?: boolean; strength?: number; micGain?: number }) => {
    await invoke('update_denoise_config', config)
  }

  const listInputDevices = async (): Promise<string[]> => {
    return await invoke('list_input_devices')
  }

  const listOutputDevices = async (): Promise<string[]> => {
    return await invoke('list_output_devices')
  }

  const listDenoiseModels = async (): Promise<string[]> => {
    return await invoke('list_denoise_models')
  }

  const installVBCable = async (): Promise<string> => {
    return await invoke('install_vb_cable')
  }

  const setMonitorMode = async (enabled: boolean) => {
    await invoke('set_monitor_mode', { enabled })
  }

  const setMonitorPoint = async (point: number) => {
    await invoke('set_monitor_point', { point })
  }

  return {
    startDenoising,
    stopDenoising,
    updateConfig,
    listInputDevices,
    listOutputDevices,
    listDenoiseModels,
    installVBCable,
    setMonitorMode,
    setMonitorPoint,
  }
}
