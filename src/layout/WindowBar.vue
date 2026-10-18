<script setup lang="ts">
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { ElMessage } from 'element-plus';
import { LockKeyhole } from 'lucide-vue-next';

const native = isTauri();
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
  <div class="window-bar" @mousedown.self.left="windowAction('drag')" @dblclick.self="windowAction('maximize')">
    <div class="window-controls">
      <button class="control close" aria-label="关闭窗口" :disabled="!native" @click="windowAction('close')"><span>×</span></button>
      <button class="control minimize" aria-label="最小化窗口" :disabled="!native" @click="windowAction('minimize')"><span>−</span></button>
      <button class="control maximize" aria-label="切换最大化" :disabled="!native" @click="windowAction('maximize')"><span>+</span></button>
    </div>
    <span class="window-title" aria-hidden="true">Cleaner <span>/</span> YOUR SPACE, REIMAGINED</span>
    <span class="window-privacy"><LockKeyhole :size="10" aria-hidden="true" />{{ native ? '本机管理' : '界面预览' }}</span>
  </div>
</template>

<style scoped>
.window-bar { height: 40px; flex-shrink: 0; display: flex; align-items: center; position: relative; border-bottom: 1px solid #bc9ee907; background: #100c1e2e; }
.window-controls { display: flex; gap: 8px; padding: 12px 21px; width: 208px; flex-shrink: 0; pointer-events: none; }
.window-controls button { pointer-events: auto; }
.window-title { color: #8d819e; font-size: 9px; letter-spacing: 1.2px; pointer-events: none; padding-left: 29px; }
.window-title span { color: #635574; padding: 0 8px; }
.window-privacy { margin-left: auto; padding: 0 24px; color: #847890; font-size: 10px; display: flex; align-items: center; gap: 6px; pointer-events: none; }
.control { border: 0; padding: 0; width: 12px; height: 12px; border-radius: 50%; display: grid; place-items: center; cursor: pointer; }
.close { background: #ff5f57; } .minimize { background: #febc2e; } .maximize { background: #28c840; }
.control span { opacity: 0; font-size: 11px; color: #634b30; line-height: 12px; }
.window-controls:hover span, .control:focus-visible span { opacity: 1; }
.control:disabled { cursor: default; }
@media (max-width: 960px) { .window-controls { width: 184px; } .window-title { padding-left: 23px; } }
@media (max-width: 700px) { .window-controls { width: 68px; padding-left: 12px; gap: 6px; } .control { width: 10px; height: 10px; } .window-title { padding-left: 18px; font-size: 8px; } .window-privacy { display: none; } }
</style>
