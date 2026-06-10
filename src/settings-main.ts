import { createApp } from 'vue'
import './assets/styles/main.css'
import i18n from './locales'
import SettingsPage from './components/SettingsPage.vue'

createApp(SettingsPage).use(i18n).mount('#app')
