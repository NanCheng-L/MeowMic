import { invoke } from '@tauri-apps/api/core'

export function useAudioEngine() {
  const startDenoising = async (inputDevice?: string, outputDevice?: string) => {
    await invoke('start_denoising', { inputDevice, outputDevice })
  }

  const stopDenoising = async () => {
    await invoke('stop_denoising')
  }

  const updateConfig = async (config: { enabled?: boolean; strength?: number }) => {
    await invoke('update_denoise_config', config)
  }

  const listInputDevices = async (): Promise<string[]> => {
    return await invoke('list_input_devices')
  }

  const listOutputDevices = async (): Promise<string[]> => {
    return await invoke('list_output_devices')
  }

  return {
    startDenoising,
    stopDenoising,
    updateConfig,
    listInputDevices,
    listOutputDevices,
  }
}
