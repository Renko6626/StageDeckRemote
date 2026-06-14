// 导播端路由（spec §10）。当前单页面，预留后续设置/彩排页扩展。
// Tauri 自定义协议下使用 hash 历史更稳妥。
import { createRouter, createWebHashHistory } from "vue-router";

import DirectorView from "./views/DirectorView.vue";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [{ path: "/", name: "director", component: DirectorView }],
});
