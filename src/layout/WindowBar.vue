<script setup lang="ts">
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { ElMessage } from 'element-plus';
import { useRoute } from 'vue-router';

const native = isTauri();
const route = useRoute();
function titlebarAction(event: MouseEvent, action: 'drag' | 'maximize') {
  if ((event.target as HTMLElement).closest('button, a, input, .header-actions, .window-controls')) return;
  void windowAction(action);
}
async function windowAction(action: 'close' | 'minimize' | 'maximize' | 'drag') {
  if (!native) return;
  try {
    const window = getCurrentWindow();
    if (action === 'close') await window.close();
    else if (action === 'minimize') await window.minimize();
    else if (action === 'maximize') await window.toggleMaximize();
    else await window.startDragging();
  } catch (error) { ElMessage.error(String(error)); }
}
</script>

<template>
  <div class="window-bar" @mousedown.left="titlebarAction($event, 'drag')" @dblclick="titlebarAction($event, 'maximize')">
    <div class="window-controls">
      <button class="control close" aria-label="关闭窗口" :disabled="!native" @click="windowAction('close')"><span>×</span></button>
      <button class="control minimize" aria-label="最小化窗口" :disabled="!native" @click="windowAction('minimize')"><span>−</span></button>
      <button class="control maximize" aria-label="切换最大化" :disabled="!native" @click="windowAction('maximize')"><span>+</span></button>
    </div>
    <div id="page-titlebar" class="page-titlebar"><span v-if="route.path === '/'" class="window-title">AppClear <span>/</span> 空间概览</span></div>
  </div>
</template>

<style scoped>
.window-bar { min-height: 56px; flex-shrink: 0; display: flex; align-items: center; position: relative; border-bottom: 1px solid var(--line); background: var(--surface); }
.window-controls { display: flex; gap: 8px; padding: 12px 21px; width: 208px; flex-shrink: 0; pointer-events: none; }
.window-controls button { pointer-events: auto; }
.page-titlebar { flex: 1; min-width: 0; padding: 8px 28px; }
.window-title { color: var(--text-faint); font-size: 12px; pointer-events: none; }
.window-title span { color: var(--text-faint); padding: 0 8px; }
.control { border: 0; padding: 0; width: 12px; height: 12px; border-radius: 50%; display: grid; place-items: center; cursor: pointer; }
.close { background: #ff5f57; } .minimize { background: #febc2e; } .maximize { background: #28c840; }
.control span { opacity: 0; font-size: 11px; color: #634b30; line-height: 12px; }
.window-controls:hover span, .control:focus-visible span { opacity: 1; }
.control:disabled { cursor: default; }
@media (max-width: 960px) { .window-controls { width: 184px; } .page-titlebar { padding-inline: 22px; } }
@media (max-width: 700px) { .window-controls { width: 68px; padding-left: 12px; gap: 6px; } .control { width: 10px; height: 10px; } .page-titlebar { padding-inline: 18px; } }
</style>
