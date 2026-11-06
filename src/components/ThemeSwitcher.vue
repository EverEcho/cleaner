<script setup lang="ts">
import { Moon, Sun, Monitor } from 'lucide-vue-next';
import { ElMessage } from 'element-plus';
import { setTheme, themePreference, type ThemePreference } from '../theme';

const options = [
  { value: 'dark', label: '深色', icon: Moon },
  { value: 'light', label: '浅色', icon: Sun },
  { value: 'system', label: '系统', icon: Monitor },
] as const;

function select(value: ThemePreference) {
  if (!setTheme(value)) ElMessage.warning('主题已切换，但无法保存；下次启动需重新选择。');
}
</script>

<template>
  <div class="theme-setting">
    <span class="theme-label">外观主题</span>
    <div class="theme-options" role="group" aria-label="外观主题">
      <button v-for="option in options" :key="option.value" type="button"
        :aria-label="option.value === 'system' ? '跟随系统主题' : `${option.label}主题`"
        :aria-pressed="themePreference === option.value"
        :title="option.value === 'system' ? '跟随系统主题' : `${option.label}主题`"
        @click="select(option.value)">
        <component :is="option.icon" :size="13" aria-hidden="true" />
        <span>{{ option.label }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.theme-setting { padding: 14px 0 0; flex-shrink: 0; }
.theme-label { display: block; margin: 0 4px 8px; color: var(--text-muted); font-size: 10px; }
.theme-options { display: flex; gap: 3px; padding: 3px; border: 1px solid var(--line); background: var(--bg); border-radius: 10px; }
.theme-options button { display: flex; align-items: center; justify-content: center; gap: 5px; flex: 1; min-width: 0; min-height: 29px; padding: 5px 3px; border: 1px solid transparent; border-radius: 7px; background: transparent; color: var(--text-muted); cursor: pointer; font-size: 11px; }
.theme-options button:hover { background: var(--surface-raised); color: var(--text); }
.theme-options button[aria-pressed='true'] { background: var(--accent-soft); border-color: var(--accent-border); color: var(--accent); font-weight: 600; }
@media (max-height: 650px) { .theme-setting { padding-top: 6px; } .theme-label { display: none; } }
@media (max-width: 700px) { .theme-label, .theme-options button span { display: none; } .theme-options { flex-direction: column; } .theme-options button { min-height: 28px; } }
</style>
