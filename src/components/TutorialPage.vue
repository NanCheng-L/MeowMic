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
            <span class="social-icon">📺</span>
            <span>Bilibili</span>
          </a>
          <a href="https://www.douyin.com" target="_blank" class="social-btn douyin">
            <span class="social-icon">🎵</span>
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
  background: rgba(254, 44, 85, 0.15);
  color: #fe2c55;
  border: 1px solid rgba(254, 44, 85, 0.3);
}

.social-btn.douyin:hover {
  background: rgba(254, 44, 85, 0.25);
}

.social-icon {
  font-size: 18px;
}
</style>
