import { createI18n } from 'vue-i18n'
import zhCN from './zh-CN'
import en from './en'

const i18n = createI18n({
  legacy: false,
  locale: localStorage.getItem('meowmic-lang') || 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: {
    'zh-CN': zhCN,
    en,
  },
})

export default i18n

export const availableLocales = [
  { value: 'zh-CN', label: '中文' },
  { value: 'en', label: 'English' },
]

export function setLocale(locale: string) {
  ;(i18n.global.locale as any).value = locale
  localStorage.setItem('meowmic-lang', locale)
}
