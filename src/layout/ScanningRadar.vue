<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import AuroraCore from './AuroraCore.vue';

const props = withDefaults(
  defineProps<{
    title?: string;
    subTitle?: string;
    tips?: string[];
  }>(),
  {
    title: '正在扫描你的空间…',
    subTitle: '正在检索本地文件，请稍候',
    tips: () => [],
  }
);

const currentTipIndex = ref(0);
let timer: ReturnType<typeof setInterval> | undefined;

onMounted(() => {
  if (props.tips.length > 1) {
    timer = setInterval(() => {
      currentTipIndex.value = (currentTipIndex.value + 1) % props.tips.length;
    }, 2600);
  }
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
});
</script>

<template>
  <div class="scanner-overlay" role="status" aria-live="polite" aria-busy="true">
    <div class="scanner-art"><AuroraCore active /></div>
    <h2 class="scanner-title">{{ title }}</h2>
    <div class="scanner-tip-box" aria-live="off">
      <transition name="fade-slide" mode="out-in">
        <span :key="currentTipIndex" class="scanner-tip">{{ tips[currentTipIndex] || subTitle }}</span>
      </transition>
    </div>
    <div class="progress-track" aria-hidden="true"><div class="progress-active" /></div>
    <span class="scanner-caption">正在读取 · 完成后展示扫描结果</span>
  </div>
</template>

<style scoped>
.scanner-overlay { width: 100%; min-height: 390px; flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; position: relative; background: radial-gradient(ellipse at 50% 35%, #a78bfa08, transparent 60%); padding: 20px 16px 32px; user-select: none; }
.scanner-art { width: 250px; height: 250px; margin-top: -22px; margin-bottom: -13px; }
.scanner-title { font-size: 19px; font-weight: 550; color: #ece6ff; letter-spacing: .1px; margin: 0 0 10px; text-align: center; }
.scanner-tip-box { min-height: 24px; display: flex; align-items: center; justify-content: center; margin-bottom: 21px; text-align: center; }
.scanner-tip { font-size: 12px; color: #9693ad; line-height: 1.6; }
.progress-track { width: 180px; height: 3px; background: #c0aaff12; border-radius: 99px; overflow: hidden; position: relative; }
.progress-active { position: absolute; inset-block: 0; width: 40%; background: linear-gradient(90deg, #8b5fe9, #9eabf1, #64d5e9); border-radius: 99px; animation: progress-indeterminate 2s infinite ease-in-out; }
.scanner-caption { font-size: 9px; letter-spacing: .5px; color: #736d88; margin-top: 13px; }
.fade-slide-enter-active, .fade-slide-leave-active { transition: opacity .25s ease, transform .25s ease; }
.fade-slide-enter-from { opacity: 0; transform: translateY(5px); }
.fade-slide-leave-to { opacity: 0; transform: translateY(-5px); }
@keyframes progress-indeterminate { 0% { left: -40%; } 100% { left: 100%; } }
@media (max-height: 650px) { .scanner-overlay { min-height: 330px; } .scanner-art { width: 215px; height: 215px; } }
@media (prefers-reduced-motion: reduce) { .progress-active { animation: none; left: 30%; } .fade-slide-enter-active, .fade-slide-leave-active { transition: none; } }
</style>
