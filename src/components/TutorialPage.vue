<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
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
      </section>

      <!-- 关注作者 -->
      <section class="section">
        <h2>{{ t('tutorial.follow') }}</h2>
        <div class="social-links">
          <a href="https://space.bilibili.com" target="_blank" class="social-btn bilibili">
            <svg class="social-icon" viewBox="0 0 18 18" width="20" height="20" fill="currentColor">
              <path fill-rule="evenodd" clip-rule="evenodd" d="M3.73252 2.67094C3.33229 2.28484 3.33229 1.64373 3.73252 1.25764C4.11291 0.890684 4.71552 0.890684 5.09591 1.25764L7.21723 3.30403C7.27749 3.36218 7.32869 3.4261 7.37081 3.49407H10.5789C10.6211 3.4261 10.6723 3.36218 10.7325 3.30403L12.8538 1.25764C13.2342 0.890684 13.8368 0.890684 14.2172 1.25764C14.6175 1.64373 14.6175 2.28484 14.2172 2.67094L13.364 3.49407H14C16.2091 3.49407 18 5.28493 18 7.49407V12.9996C18 15.2087 16.2091 16.9996 14 16.9996H4C1.79086 16.9996 0 15.2087 0 12.9996V7.49406C0 5.28492 1.79086 3.49407 4 3.49407H4.58579L3.73252 2.67094ZM4 5.42343C2.89543 5.42343 2 6.31886 2 7.42343V13.0702C2 14.1748 2.89543 15.0702 4 15.0702H14C15.1046 15.0702 16 14.1748 16 13.0702V7.42343C16 6.31886 15.1046 5.42343 14 5.42343H4ZM5 9.31747C5 8.76519 5.44772 8.31747 6 8.31747C6.55228 8.31747 7 8.76519 7 9.31747V10.2115C7 10.7638 6.55228 11.2115 6 11.2115C5.44772 11.2115 5 10.7638 5 10.2115V9.31747ZM12 8.31747C11.4477 8.31747 11 8.76519 11 9.31747V10.2115C11 10.7638 11.4477 11.2115 12 11.2115C12.5523 11.2115 13 10.7638 13 10.2115V9.31747C13 8.76519 12.5523 8.31747 12 8.31747Z"/>
            </svg>
            <span>Bilibili</span>
          </a>
          <a href="https://www.douyin.com" target="_blank" class="social-btn douyin">
            <svg class="social-icon" viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
              <path d="M19.59 6.69a4.83 4.83 0 01-3.77-4.25V2h-3.45v13.89a2.89 2.89 0 01-2.88 2.59 2.89 2.89 0 01-2.89-2.89 2.89 2.89 0 012.89-2.89c.3 0 .59.05.86.13V9.36a6.34 6.34 0 00-.86-.06 6.35 6.35 0 100 12.7 6.35 6.35 0 006.34-6.35V8.33a8.27 8.27 0 004.76 1.52V6.45a4.97 4.97 0 01-.73-.05v.29z"/>
            </svg>
            <span>抖音</span>
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
  color: #000;
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
  background: rgba(138, 43, 226, 0.1);
  color: #a855f7;
  border: 1px solid rgba(138, 43, 226, 0.2);
}

.social-btn.douyin:hover {
  background: rgba(138, 43, 226, 0.18);
}

.social-icon {
  flex-shrink: 0;
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
  background: #0a0a0a;
}
body::-webkit-scrollbar-thumb {
  background: #1e1e1e;
  border-radius: 3px;
}
body::-webkit-scrollbar-thumb:hover {
  background: #666666;
}
</style>
