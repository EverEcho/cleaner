<script setup lang="ts">
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { ElMessage } from 'element-plus';

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
  </div>
</template>

<style scoped>
.window-bar { height: 34px; flex-shrink: 0; background: #fff; }
.window-controls { display: flex; gap: 8px; padding: 12px 16px; width: fit-content; }
.control { border: 0; padding: 0; width: 12px; height: 12px; border-radius: 50%; display: grid; place-items: center; cursor: pointer; }
.close { background: #ff5f57; } .minimize { background: #febc2e; } .maximize { background: #28c840; }
.control span { opacity: 0; font-size: 11px; color: #634b30; line-height: 12px; }
.window-controls:hover span, .control:focus-visible span { opacity: 1; }
.control:disabled { cursor: default; }
</style>
