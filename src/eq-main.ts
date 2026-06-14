import { createApp } from 'vue'
import './assets/styles/main.css'
import i18n from './locales'
import EqPage from './components/EqPage.vue'

createApp(EqPage).use(i18n).mount('#app')
