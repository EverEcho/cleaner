<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue';
import { isTauri } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import { Sparkles, Trash2, HardDrive, AppWindow, ArrowUpCircle, Power, PanelsTopLeft, Files, Copy, Monitor, ChevronRight,Activity } from 'lucide-vue-next';
import WindowBar from './layout/WindowBar.vue';
import { loadPlatform, platformInfo } from './platform';
import TaskStatusBar from './layout/TaskStatusBar.vue';
import { initializeTasks,disposeTasks } from './tasks';
import { loadPreferences } from './preferences';
import { startMonitor,stopMonitor } from './monitor';

const native = isTauri();
const systemLabel = computed(() => platformInfo.value?.os === 'macos' ? 'macOS' : platformInfo.value?.os === 'linux' ? 'Linux' : native ? '桌面客户端' : '界面预览');
const groups = [
  { label: '工作空间', items: [
    { path: '/', label: '空间概览', icon: Sparkles },
    { path: '/clean', label: '垃圾清理', icon: Trash2 },
    { path: '/storage', label: '磁盘透视', icon: HardDrive },
    { path: '/monitor', label: '本机监控', icon: Activity },
  ] },
  { label: '应用与系统', items: [
    { path: '/apps', label: '应用程序', icon: AppWindow },
    { path: '/updates', label: '软件更新', icon: ArrowUpCircle },
    { path: '/autostart', label: '启动项', icon: Power },
    { path: '/entries', label: '桌面入口', icon: PanelsTopLeft },
  ] },
  { label: '文件整理', items: [
    { path: '/large-files', label: '大文件', icon: Files },
    { path: '/duplicates', label: '重复文件', icon: Copy },
  ] },
];
onMounted(() => { loadPlatform().catch(error => ElMessage.error('读取系统信息失败: ' + error)); initializeTasks().catch(error=>ElMessage.warning('任务状态连接失败: '+error)); loadPreferences().then(startMonitor).catch(error=>ElMessage.warning('设置读取失败: '+error)); });
onUnmounted(()=>{disposeTasks();stopMonitor();});
</script>

<template>
  <div class="cleaner-shell">
    <WindowBar />
    <div class="workspace">
      <aside class="sidebar" aria-label="工具导航">
        <router-link to="/" class="brand" aria-label="Cleaner 空间概览">
          <span class="brand-symbol" aria-hidden="true"><Sparkles :size="23" :stroke-width="1.7" /></span>
          <span class="brand-name">Cleaner<small>软件管家</small></span>
        </router-link>
        <nav class="navigation">
          <div v-for="group in groups" :key="group.label" class="nav-group">
            <p class="nav-label">{{ group.label }}</p>
            <router-link v-for="item in group.items" :key="item.path" :to="item.path" class="nav-item" :aria-label="item.label" :class="{ active: $route.path === item.path }" :aria-current="$route.path === item.path ? 'page' : undefined">
              <component :is="item.icon" :size="18" :stroke-width="1.7" aria-hidden="true" />
              <span>{{ item.label }}</span>
              <ChevronRight v-if="$route.path === item.path" class="nav-arrow" :size="13" aria-hidden="true" />
            </router-link>
          </div>
        </nav>
        <div class="machine-card">
          <span class="machine-icon"><Monitor :size="19" :stroke-width="1.6" aria-hidden="true" /></span>
          <div><strong>{{ systemLabel }}</strong><small>{{ native ? '在本机，掌控你的空间' : '本机功能请在桌面端使用' }}</small></div>
        </div>
      </aside>
      <div class="page-body">
        <router-view v-slot="{ Component }">
          <Transition name="page" mode="out-in">
            <KeepAlive :max="10"><component :is="Component" /></KeepAlive>
          </Transition>
        </router-view>
      </div>
    </div>
    <TaskStatusBar />
  </div>
</template>
