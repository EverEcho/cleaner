<script setup lang="ts">
import { onMounted } from 'vue';
import { ElMessage } from 'element-plus';
import WindowBar from './layout/WindowBar.vue';
import { loadPlatform } from './platform';
onMounted(() => { loadPlatform().catch(error => ElMessage.error('读取系统信息失败: ' + error)); });
</script>

<template>
  <div class="lemon-app-container">
    <WindowBar />
    <div class="page-body"><router-view /></div>
  </div>
</template>

<style>
 :root {
  --el-color-primary: #00bd86;
  --el-color-primary-light-3: #55d4af;
  --el-color-primary-light-5: #88e2c7;
  --el-color-primary-light-7: #b8eddf;
  --el-color-primary-light-9: #effaf6;
  --el-color-primary-dark-2: #00996d;
  --el-border-radius-base: 3px;
}
.page-body { flex: 1; min-height: 0; overflow: hidden; }
.platform-note { font-size: 13px; color: #92979e; margin: 0 0 12px; line-height: 1.6; }
/* 全局基础重置与样式 */
* {
  box-sizing: border-box;
}

body {
  margin: 0;
  padding: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  background-color: #ffffff;
  color: #1f2937;
  user-select: none;
}

.lemon-app-container {
  width: 100vw;
  height: 100vh;
  margin: 0;
  padding: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

/* Shared Lemon visual language across lists, forms and scan results. */
.page-body .subpage-container { background: #fff; }
.page-body .subpage-content { min-height: 0; background: #fff; padding: 18px 30px; }
.el-table { --el-table-header-bg-color: #fff; --el-table-header-text-color: #8c9299; --el-table-row-hover-bg-color: #f3f6f4; --el-table-border-color: #f0f0f0; --el-table-tr-bg-color: #fff; color: #50555c; font-size: 12px; }
.el-table th.el-table__cell { font-weight: 400; }
.el-table .cell { line-height: 21px; }
.el-table .el-table__cell { padding: 11px 0; }
.el-table .el-table__row--striped td.el-table__cell { background: #fafbfa; }
.el-button { box-shadow: none; font-weight: 400; }
.el-radio-button__inner { font-size: 12px; }
.el-input__wrapper, .el-select__wrapper { box-shadow: 0 0 0 1px #e9ece9 inset; }
.el-tag { border-radius: 2px; font-size: 11px; }
.el-dialog { border-radius: 4px; }
.el-message-box { border-radius: 4px; }
.el-message-box__message { white-space: pre-line; }
.el-checkbox { --el-checkbox-checked-bg-color: #00c593; --el-checkbox-checked-input-border-color: #00c593; height: 28px; }
.el-checkbox__inner { width: 13px; height: 13px; border-color: #d5d8db; border-radius: 1px; }
button:focus-visible { outline: 2px solid #00c593; outline-offset: 3px; }
</style>