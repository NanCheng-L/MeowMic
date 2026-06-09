<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const handleClose = () => {
  emit('close')
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="open" class="overlay" @click.self="handleClose">
        <Transition name="slide">
          <div v-if="open" class="dialog">
            <div class="dialog-header">
              <h2>{{ t('tutorial.title') }}</h2>
              <button class="close-btn" @click="handleClose">
                <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="4" y1="4" x2="12" y2="12"/>
                  <line x1="12" y1="4" x2="4" y2="12"/>
                </svg>
              </button>
            </div>

            <div class="dialog-body">
              <!-- 快速上手 -->
              <div class="section">
                <h3>{{ t('tutorial.quickStart') }}</h3>
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
              </div>

              <!-- 设备说明 -->
              <div class="section">
                <h3>{{ t('tutorial.deviceGuide') }}</h3>
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
              </div>

              <!-- VB-Cable -->
              <div class="section">
                <h3>{{ t('tutorial.vbCable') }}</h3>
                <p>{{ t('tutorial.vbCableDesc') }}</p>
                <div class="tip-box">
                  <span class="tip-icon">💡</span>
                  <span>{{ t('tutorial.vbCableTip') }}</span>
                </div>
              </div>

              <!-- 常见问题 -->
              <div class="section">
                <h3>{{ t('tutorial.faq') }}</h3>
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
              </div>

              <!-- 关注作者 -->
              <div class="section">
                <h3>{{ t('tutorial.follow') }}</h3>
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
              </div>
            </div>

            <div class="dialog-footer">
              <button class="btn btn-primary" @click="handleClose">{{ t('tutorial.gotIt') }}</button>
            </div>
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
  backdrop-filter: blur(4px);
}

.dialog {
  background: var(--surface);
  border-radius: 16px;
  border: 1px solid var(--border);
  width: 420px;
  max-height: 80vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border);
}

.dialog-header h2 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.close-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  transition: color 0.2s, background 0.2s;
}

.close-btn:hover {
  color: var(--text-primary);
  background: var(--border);
}

.dialog-body {
  padding: 20px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.section {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.section h3 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  padding-bottom: 6px;
  border-bottom: 1px solid var(--border);
}

.section p {
  margin: 0;
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.6;
}

/* Steps */
.steps {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.step {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}

.step-num {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--accent);
  color: #000;
  font-size: 12px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.step-content strong {
  font-size: 13px;
  color: var(--text-primary);
  display: block;
  margin-bottom: 2px;
}

.step-content p {
  font-size: 12px;
  color: var(--text-muted);
  margin: 0;
}

/* Info cards */
.info-card {
  display: flex;
  gap: 12px;
  padding: 12px;
  background: var(--bg);
  border-radius: 8px;
  border: 1px solid var(--border);
}

.info-icon {
  font-size: 20px;
  flex-shrink: 0;
}

.info-text strong {
  font-size: 13px;
  color: var(--text-primary);
  display: block;
  margin-bottom: 2px;
}

.info-text p {
  font-size: 12px;
  color: var(--text-muted);
  margin: 0;
}

/* Tip box */
.tip-box {
  display: flex;
  gap: 8px;
  padding: 10px 12px;
  background: rgba(245, 158, 11, 0.1);
  border: 1px solid rgba(245, 158, 11, 0.2);
  border-radius: 8px;
  font-size: 12px;
  color: var(--warning);
}

.tip-icon {
  flex-shrink: 0;
}

/* FAQ */
.faq-item {
  padding: 8px 0;
}

.faq-item strong {
  font-size: 13px;
  color: var(--text-primary);
  display: block;
  margin-bottom: 4px;
}

.faq-item p {
  font-size: 12px;
  color: var(--text-muted);
  margin: 0;
}

/* Social links */
.social-links {
  display: flex;
  gap: 10px;
}

.social-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  border-radius: 8px;
  font-size: 13px;
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
  font-size: 16px;
}

/* Footer */
.dialog-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--border);
  display: flex;
  justify-content: flex-end;
}

.btn {
  padding: 8px 20px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  border: none;
  transition: all 0.2s;
}

.btn-primary {
  background: var(--accent);
  color: #000;
}

.btn-primary:hover {
  opacity: 0.9;
}

/* Transitions */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.slide-enter-active {
  transition: all 0.3s ease;
}
.slide-leave-active {
  transition: all 0.2s ease;
}
.slide-enter-from {
  opacity: 0;
  transform: translateY(20px) scale(0.95);
}
.slide-leave-to {
  opacity: 0;
  transform: translateY(10px) scale(0.98);
}
</style>
