<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTheme } from '../composables/useTheme'
import { setLocale } from '../locales'
import afdianLogo from '../assets/afdian-logo.svg'

const { t } = useI18n()
useTheme()

// 轮询语言变更
let timer: ReturnType<typeof setInterval> | null = null
onMounted(() => {
  timer = setInterval(() => {
    const lang = localStorage.getItem('meowmic-lang')
    if (lang) setLocale(lang)
  }, 1000)
})
onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="tutorial-page">
    <div class="tutorial-header">
      <h1>{{ t('tutorial.title') }}</h1>
    </div>

    <div class="tutorial-body">
      <!-- 快速上手 -->
      <section class="section">
        <h2>{{ t('tutorial.quickStart') }}</h2>
        <div class="steps">
          <div class="step">
            <span class="step-num">1</span>
            <div class="step-content">
              <strong>{{ t('tutorial.step1Title') }}</strong>
              <p>{{ t('tutorial.step1Desc') }}</p>
            </div>
          </div>
          <div class="step">
            <span class="step-num">2</span>
            <div class="step-content">
              <strong>{{ t('tutorial.step2Title') }}</strong>
              <p>{{ t('tutorial.step2Desc') }}</p>
            </div>
          </div>
          <div class="step">
            <span class="step-num">3</span>
            <div class="step-content">
              <strong>{{ t('tutorial.step3Title') }}</strong>
              <p>{{ t('tutorial.step3Desc') }}</p>
            </div>
          </div>
        </div>
      </section>

      <!-- 配置示例 -->
      <section class="section">
        <h2>{{ t('tutorial.exampleTitle') }}</h2>
        <div class="example-box">
          <div class="example-header">
            <span class="example-icon">🎙️</span>
            <strong>{{ t('tutorial.exampleMeowmic') }}</strong>
          </div>
          <div class="example-row">
            <span class="example-label">{{ t('tutorial.exampleInputLabel') }}</span>
            <span class="example-value">麦克风 (Realtek(R) Audio)</span>
          </div>
          <div class="example-row">
            <span class="example-label">{{ t('tutorial.exampleOutputLabel') }}</span>
            <span class="example-value highlight">CABLE Input (VB-Audio Virtual Cable)</span>
          </div>
        </div>

        <div class="example-box">
          <div class="example-header">
            <span class="example-icon">🎮</span>
            <strong>{{ t('tutorial.exampleApp') }}</strong>
          </div>
          <div class="example-row">
            <span class="example-label">{{ t('tutorial.exampleAppInputLabel') }}</span>
            <span class="example-value highlight">CABLE Output (VB-Audio Virtual Cable)</span>
          </div>
        </div>

        <div class="example-box">
          <div class="example-header">
            <span class="example-icon">🔊</span>
            <strong>{{ t('tutorial.systemOutputTitle') }}</strong>
          </div>
          <p class="example-desc">{{ t('tutorial.systemOutputDesc') }}</p>
          <div class="example-row">
            <span class="example-label">{{ t('tutorial.exampleOutputLabel') }}</span>
            <span class="example-value">耳机 (Realtek(R) Audio)</span>
          </div>
        </div>

        <div class="tip-box">
          <span class="tip-icon">💡</span>
          <span>{{ t('tutorial.exampleNote') }}</span>
        </div>
      </section>

      <!-- 设备说明 -->
      <section class="section">
        <h2>{{ t('tutorial.deviceGuide') }}</h2>
        <div class="info-card">
          <div class="info-icon">🎙️</div>
          <div class="info-text">
            <strong>{{ t('tutorial.inputDevice') }}</strong>
            <p>{{ t('tutorial.inputDeviceDesc') }}</p>
          </div>
        </div>
        <div class="info-card">
          <div class="info-icon">🔊</div>
          <div class="info-text">
            <strong>{{ t('tutorial.outputDevice') }}</strong>
            <p>{{ t('tutorial.outputDeviceDesc') }}</p>
          </div>
        </div>
      </section>

      <!-- VB-Cable -->
      <section class="section">
        <h2>{{ t('tutorial.vbCable') }}</h2>
        <p>{{ t('tutorial.vbCableDesc') }}</p>
        <div class="tip-box">
          <span class="tip-icon">💡</span>
          <span>{{ t('tutorial.vbCableTip') }}</span>
        </div>
      </section>

      <!-- 常见问题 -->
      <section class="section">
        <h2>{{ t('tutorial.faq') }}</h2>
        <div class="faq-item">
          <strong>{{ t('tutorial.faq1Q') }}</strong>
          <p>{{ t('tutorial.faq1A') }}</p>
        </div>
        <div class="faq-item">
          <strong>{{ t('tutorial.faq2Q') }}</strong>
          <p>{{ t('tutorial.faq2A') }}</p>
        </div>
        <div class="faq-item">
          <strong>{{ t('tutorial.faq3Q') }}</strong>
          <p>{{ t('tutorial.faq3A') }}</p>
        </div>
        <div class="faq-item">
          <strong>{{ t('tutorial.faq4Q') }}</strong>
          <p>{{ t('tutorial.faq4A') }}</p>
        </div>
        <div class="faq-item">
          <strong>{{ t('tutorial.faq5Q') }}</strong>
          <p>{{ t('tutorial.faq5A') }}</p>
        </div>
      </section>

      <!-- 关注作者 -->
      <section class="section">
        <h2>{{ t('tutorial.follow') }}</h2>
        <div class="social-links">
          <a href="https://www.bilibili.com/video/BV1ABE16SEKk/" target="_blank" class="social-btn bilibili">
            <svg class="social-icon" viewBox="0 0 18 18" width="20" height="20" fill="currentColor">
              <path fill-rule="evenodd" clip-rule="evenodd" d="M3.73252 2.67094C3.33229 2.28484 3.33229 1.64373 3.73252 1.25764C4.11291 0.890684 4.71552 0.890684 5.09591 1.25764L7.21723 3.30403C7.27749 3.36218 7.32869 3.4261 7.37081 3.49407H10.5789C10.6211 3.4261 10.6723 3.36218 10.7325 3.30403L12.8538 1.25764C13.2342 0.890684 13.8368 0.890684 14.2172 1.25764C14.6175 1.64373 14.6175 2.28484 14.2172 2.67094L13.364 3.49407H14C16.2091 3.49407 18 5.28493 18 7.49407V12.9996C18 15.2087 16.2091 16.9996 14 16.9996H4C1.79086 16.9996 0 15.2087 0 12.9996V7.49406C0 5.28492 1.79086 3.49407 4 3.49407H4.58579L3.73252 2.67094ZM4 5.42343C2.89543 5.42343 2 6.31886 2 7.42343V13.0702C2 14.1748 2.89543 15.0702 4 15.0702H14C15.1046 15.0702 16 14.1748 16 13.0702V7.42343C16 6.31886 15.1046 5.42343 14 5.42343H4ZM5 9.31747C5 8.76519 5.44772 8.31747 6 8.31747C6.55228 8.31747 7 8.76519 7 9.31747V10.2115C7 10.7638 6.55228 11.2115 6 11.2115C5.44772 11.2115 5 10.7638 5 10.2115V9.31747ZM12 8.31747C11.4477 8.31747 11 8.76519 11 9.31747V10.2115C11 10.7638 11.4477 11.2115 12 11.2115C12.5523 11.2115 13 10.7638 13 10.2115V9.31747C13 8.76519 12.5523 8.31747 12 8.31747Z"/>
            </svg>
            <span>Bilibili</span>
          </a>
          <a href="https://v.douyin.com/iRa9zpvavIY/" target="_blank" class="social-btn douyin">
            <svg class="social-icon" viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
              <path d="M19.59 6.69a4.83 4.83 0 01-3.77-4.25V2h-3.45v13.89a2.89 2.89 0 01-2.88 2.59 2.89 2.89 0 01-2.89-2.89 2.89 2.89 0 012.89-2.89c.3 0 .59.05.86.13V9.36a6.34 6.34 0 00-.86-.06 6.35 6.35 0 100 12.7 6.35 6.35 0 006.34-6.35V8.33a8.27 8.27 0 004.76 1.52V6.45a4.97 4.97 0 01-.73-.05v.29z"/>
            </svg>
            <span>抖音</span>
          </a>
          <a href="https://github.com/NanCheng-L/MeowMic" target="_blank" class="social-btn github">
            <svg class="social-icon" viewBox="0 0 16 16" width="20" height="20" fill="currentColor">
              <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
            </svg>
            <span>GitHub</span>
          </a>
          <a href="https://afdian.com/a/NanCheng-L" target="_blank" class="social-btn afdian">
            <img :src="afdianLogo" class="social-icon" alt="afdian" />
            <span>{{ t('tutorial.afdian') }}</span>
          </a>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.tutorial-page {
  min-height: 100vh;
  background: var(--bg);
  color: var(--text-primary);
}

.tutorial-header {
  padding: 24px 28px 16px;
  border-bottom: 1px solid var(--border);
  position: sticky;
  top: 0;
  z-index: 10;
  background: var(--bg);
}

.tutorial-header h1 {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
}

.tutorial-body {
  padding: 24px 28px;
  display: flex;
  flex-direction: column;
  gap: 32px;
}

.section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.section h2 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border);
}

.section p {
  margin: 0;
  font-size: 14px;
  color: var(--text-secondary);
  line-height: 1.7;
}

/* Steps */
.steps {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.step {
  display: flex;
  gap: 14px;
  align-items: flex-start;
}

.step-num {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  background: var(--accent);
  color: white;
  font-size: 13px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.step-content strong {
  font-size: 14px;
  color: var(--text-primary);
  display: block;
  margin-bottom: 4px;
}

.step-content p {
  font-size: 13px;
  color: var(--text-muted);
  margin: 0;
}

/* Info cards */
.info-card {
  display: flex;
  gap: 14px;
  padding: 14px;
  background: var(--surface);
  border-radius: 10px;
  border: 1px solid var(--border);
}

.info-icon {
  font-size: 24px;
  flex-shrink: 0;
}

.info-text strong {
  font-size: 14px;
  color: var(--text-primary);
  display: block;
  margin-bottom: 4px;
}

.info-text p {
  font-size: 13px;
  color: var(--text-muted);
  margin: 0;
}

/* Example boxes */
.example-box {
  padding: 14px;
  background: var(--surface);
  border-radius: 10px;
  border: 1px solid var(--border);
}

.example-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}

.example-header strong {
  font-size: 14px;
  color: var(--text-primary);
}

.example-icon {
  font-size: 18px;
}

.example-desc {
  margin: 0 0 10px;
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.6;
}

.example-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  background: var(--bg);
  border-radius: 6px;
  margin-bottom: 6px;
}

.example-row:last-child {
  margin-bottom: 0;
}

.example-label {
  font-size: 13px;
  color: var(--text-muted);
  white-space: nowrap;
}

.example-value {
  font-size: 13px;
  color: var(--text-primary);
  font-family: 'DM Mono', monospace;
}

.example-value.highlight {
  color: var(--accent);
  font-weight: 500;
}

/* Tip box */
.tip-box {
  display: flex;
  gap: 10px;
  padding: 12px 14px;
  background: rgba(245, 158, 11, 0.1);
  border: 1px solid rgba(245, 158, 11, 0.2);
  border-radius: 10px;
  font-size: 13px;
  color: var(--warning);
}

.tip-icon {
  flex-shrink: 0;
}

/* FAQ */
.faq-item {
  padding: 10px 0;
}

.faq-item strong {
  font-size: 14px;
  color: var(--text-primary);
  display: block;
  margin-bottom: 4px;
}

.faq-item p {
  font-size: 13px;
  color: var(--text-muted);
  margin: 0;
}

/* Social links */
.social-links {
  display: flex;
  gap: 12px;
}

.social-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 20px;
  border-radius: 10px;
  font-size: 14px;
  font-weight: 500;
  text-decoration: none;
  white-space: nowrap;
  transition: all 0.2s;
}

.social-btn.bilibili {
  background: rgba(0, 174, 236, 0.15);
  color: #00aeec;
  border: 1px solid rgba(0, 174, 236, 0.3);
}

.social-btn.bilibili:hover {
  background: rgba(0, 174, 236, 0.25);
}

.social-btn.douyin {
  background: rgba(255, 255, 255, 0.08);
  color: var(--text-primary);
  border: 1px solid var(--border);
}

.social-btn.douyin:hover {
  background: rgba(255, 255, 255, 0.15);
}

.social-btn.github {
  background: rgba(255, 255, 255, 0.08);
  color: var(--text-primary);
  border: 1px solid var(--border);
}

.social-btn.github:hover {
  background: rgba(255, 255, 255, 0.15);
}

.social-btn.afdian {
  background: rgba(154, 114, 242, 0.1);
  color: #9a72f2;
  border: 1px solid rgba(154, 114, 242, 0.2);
}

.social-btn.afdian:hover {
  background: rgba(154, 114, 242, 0.18);
}

.social-icon {
  flex-shrink: 0;
  width: 20px;
  height: 20px;
}
</style>

<style>
html {
  overflow: hidden !important;
}
body {
  overflow-y: auto !important;
  overflow-x: hidden !important;
  height: 100vh !important;
}
body::-webkit-scrollbar {
  width: 6px;
}
body::-webkit-scrollbar-track {
  background: var(--bg);
}
body::-webkit-scrollbar-thumb {
  background: var(--border);
  border-radius: 3px;
}
body::-webkit-scrollbar-thumb:hover {
  background: var(--text-muted);
}
</style>
