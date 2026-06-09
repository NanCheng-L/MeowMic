import { createApp } from "vue";
import App from "./App.vue";
import "./assets/styles/main.css";
import i18n from "./locales";

createApp(App).use(i18n).mount("#app");
