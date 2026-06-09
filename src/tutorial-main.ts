import { createApp } from 'vue'
import i18n from './locales'
import './assets/styles/main.css'
import TutorialPage from './components/TutorialPage.vue'

createApp(TutorialPage).use(i18n).mount('#app')
