<script setup lang="ts">
// App.vue 作为顶层路由承载容器
</script>

<template>
  <div class="lemon-app-container">
    <router-view />
  </div>
</template>

<style>
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
</style>