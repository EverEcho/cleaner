<script setup lang="ts">
import { useRouter } from 'vue-router';

const props = defineProps<{
  title: string;
}>();

const router = useRouter();

function goBack() {
  router.push('/');
}
</script>

<template>
  <div class="subpage-header">
    <div class="header-left">
      <el-button link :icon="'ArrowLeft'" @click="goBack">
        返回首页
      </el-button>
      <span class="subpage-title">{{ title }}</span>
    </div>
    <div class="header-actions">
      <slot name="actions"></slot>
    </div>
  </div>
</template>

<style scoped>
.subpage-header {
  height: 60px;
  background: #ffffff;
  border-bottom: 1px solid #e5e7eb;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 24px;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.subpage-title {
  font-size: 18px;
  font-weight: 600;
  color: #111827;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 16px;
}
</style>
