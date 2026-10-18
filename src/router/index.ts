import { createRouter, createWebHashHistory } from 'vue-router';

const routes = [
  { path: '/monitor', name: 'Monitor', component: () => import('../views/MonitorView.vue'), meta: { title: '本机监控' } },
  { path: '/storage', name: 'Storage', component: () => import('../views/StorageView.vue'), meta: { title: '磁盘空间分析' } },
  {
    path: '/',
    name: 'Dashboard',
    component: () => import('../views/DashboardView.vue'),
    meta: { title: '首页概览' },
  },
  {
    path: '/clean',
    name: 'Clean',
    component: () => import('../views/CleanView.vue'),
    meta: { title: '系统垃圾清理' },
  },
  {
    path: '/apps',
    name: 'Apps',
    component: () => import('../views/AppsView.vue'),
    meta: { title: '应用强力卸载' },
  },
  {
    path: '/autostart',
    name: 'Autostart',
    component: () => import('../views/AutostartView.vue'),
    meta: { title: '开机启动项管理' },
  },
  {
    path: '/entries',
    name: 'Entries',
    component: () => import('../views/EntriesView.vue'),
    meta: { title: '桌面与菜单图标' },
  },
  {
    path: '/large-files',
    name: 'LargeFiles',
    component: () => import('../views/LargeFilesView.vue'),
    meta: { title: '大文件清理' },
  },
  {
    path: '/duplicates',
    name: 'Duplicates',
    component: () => import('../views/DuplicatesView.vue'),
    meta: { title: '重复文件清理' },
  },
  {
    path: '/updates',
    name: 'Updates',
    component: () => import('../views/UpdatesView.vue'),
    meta: { title: '软件更新检测' },
  },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;

