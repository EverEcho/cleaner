import { createApp } from "vue";
import ElementPlus from "element-plus";
import "element-plus/dist/index.css";
import "./styles/theme.css";
import * as ElementPlusIconsVue from "@element-plus/icons-vue";
import router from "./router";
import App from "./App.vue";
import TrayPanel from "./TrayPanel.vue";
import { initializeTheme } from "./theme";

initializeTheme();
const trayWindow = window.location.hash === '#/tray';
const app = createApp(trayWindow ? TrayPanel : App);

for (const [key, component] of Object.entries(ElementPlusIconsVue)) {
  app.component(key, component);
}

app.use(ElementPlus);
if (!trayWindow) app.use(router);
app.mount("#app");
