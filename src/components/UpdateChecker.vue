<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { getVersion } from '@tauri-apps/api/app'

const { t } = useI18n()

const currentVersion = ref('')
const status = ref<'idle' | 'checking' | 'available' | 'no-update' | 'error' | 'downloading' | 'download-error' | 'ready'>('idle')
const statusMessage = ref('')
const newVersion = ref('')
const downloadProgress = ref(0)
let availableUpdate: Update | null = null

const checkUpdate = async () => {
  status.value = 'checking'
  statusMessage.value = t('settings.checking')
  downloadProgress.value = 0
  availableUpdate = null

  try {
    const update = await check()
    if (update) {
      availableUpdate = update
      status.value = 'available'
      newVersion.value = update.version
      statusMessage.value = `${t('settings.updateAvailable')} v${update.version}`
    } else {
      status.value = 'no-update'
      statusMessage.value = t('settings.noUpdate')
    }
  } catch (e) {
    status.value = 'error'
    statusMessage.value = t('settings.updateError')
    console.error('[MeowMic] 检查更新失败:', e)
  }
}

const doUpdate = async () => {
  if (status.value !== 'available' || !availableUpdate) return
  status.value = 'downloading'
  statusMessage.value = t('settings.downloading')

  try {
    const update = availableUpdate
    let downloaded = 0
    let contentLength = 0

    await update.download((event) => {
      switch (event.event) {
        case 'Started':
          contentLength = event.data.contentLength ?? 0
          break
        case 'Progress':
          downloaded += event.data.chunkLength
          if (contentLength > 0) {
            downloadProgress.value = Math.round((downloaded / contentLength) * 100)
          }
          break
        case 'Finished':
          break
      }
    })

    status.value = 'ready'
    statusMessage.value = t('settings.installReady')
    availableUpdate = update
  } catch (e: any) {
    status.value = 'download-error'
    statusMessage.value = `${t('settings.downloadError')}: ${e?.message || e?.toString() || 'unknown'}`
    console.error('[MeowMic] 下载更新失败:', e)
  }
}

const doInstall = async () => {
  if (!availableUpdate) return
  try {
    await availableUpdate.install()
  } catch (e: any) {
    console.error('[MeowMic] 安装更新失败:', e)
  }
}

onMounted(async () => {
  try {
    currentVersion.value = await getVersion()
  } catch {}
  // 进入设置页自动检查更新
  await checkUpdate()
})
</script>

<template>
  <section class="section" :class="{ highlight: status === 'available' }">
    <h2>
      {{ t('settings.updateChecker') }}
      <span v-if="status === 'available'" class="badge">NEW</span>
    </h2>
    <div class="update-info">
      <span class="version-label">{{ t('settings.currentVersion') }}: v{{ currentVersion }}</span>
    </div>

    <div v-if="status !== 'idle'" class="update-status" :class="status">
      {{ statusMessage }}
    </div>

    <div v-if="status === 'downloading'" class="progress-wrap">
      <div class="progress-bar" :style="{ width: downloadProgress + '%' }"></div>
      <span class="progress-text">{{ downloadProgress }}%</span>
    </div>

    <div class="update-actions">
      <button
        v-if="status === 'available' || status === 'download-error'"
        class="update-btn"
        @click="doUpdate"
      >
        {{ t('settings.downloadUpdate') }}
      </button>
      <button
        v-if="status === 'ready'"
        class="update-btn install-btn"
        @click="doInstall"
      >
        {{ t('settings.installUpdate') }}
      </button>
      <button
        v-if="status !== 'downloading'"
        class="check-btn"
        :disabled="status === 'checking'"
        @click="checkUpdate"
      >
        {{ status === 'checking' ? t('settings.checking') : t('settings.checkUpdate') }}
      </button>
    </div>
  </section>
</template>

<style scoped>
.section {
  display: flex;
  flex-direction: column;
  gap: 12px;
  background: var(--surface);
  border-radius: 12px;
  padding: 16px;
}

.section.highlight {
  border: 1.5px solid var(--accent);
  background: rgba(99, 102, 241, 0.04);
}

.section h2 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  gap: 8px;
}

.badge {
  font-size: 10px;
  font-weight: 700;
  color: white;
  background: #ef4444;
  padding: 1px 6px;
  border-radius: 4px;
  letter-spacing: 0.5px;
}

.update-info {
  display: flex;
  align-items: center;
}

.version-label {
  font-size: 13px;
  color: var(--text-muted);
  font-family: 'DM Mono', monospace;
}

.update-status {
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 13px;
  border: 1px solid var(--border);
  background: var(--surface);
}

.update-status.available {
  border-color: var(--accent);
  background: rgba(99, 102, 241, 0.08);
  color: var(--accent);
  font-weight: 500;
}

.update-status.no-update {
  color: var(--text-muted);
}

.update-status.error,
.update-status.download-error {
  border-color: var(--danger);
  background: rgba(239, 68, 68, 0.06);
  color: var(--danger);
}

.update-status.downloading {
  border-color: var(--accent);
  color: var(--accent);
}

.update-status.ready {
  border-color: #10b981;
  background: rgba(16, 185, 129, 0.06);
  color: #10b981;
}

.progress-wrap {
  position: relative;
  height: 20px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}

.progress-bar {
  height: 100%;
  background: var(--accent);
  border-radius: 10px;
  transition: width 0.3s ease;
  opacity: 0.7;
}

.progress-text {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-primary);
}

.update-actions {
  display: flex;
  gap: 8px;
}

.update-btn {
  padding: 8px 16px;
  border-radius: 8px;
  border: none;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  background: var(--accent);
  color: white;
  transition: all 0.2s;
}

.update-btn:hover {
  filter: brightness(1.1);
}

.install-btn {
  background: #10b981;
}

.check-btn {
  padding: 8px 16px;
  border-radius: 8px;
  border: 1px solid var(--border);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  background: var(--surface);
  color: var(--text-primary);
  transition: all 0.2s;
}

.check-btn:hover {
  border-color: var(--accent);
  color: var(--accent);
}

.check-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
