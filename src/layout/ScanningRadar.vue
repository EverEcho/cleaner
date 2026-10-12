<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';

const props = withDefaults(
  defineProps<{
    title?: string;
    subTitle?: string;
    tips?: string[];
  }>(),
  {
    title: '正在全力扫描中…',
    subTitle: '深度检索系统中，请稍候',
    tips: () => [
      '正在检索本地磁盘索引…',
      '正在比对软件包元数据…',
      '正在分析缓存与冗余孤儿文件…',
      '正在检测启动项与桌面关联关系…',
    ],
  }
);

const currentTipIndex = ref(0);
let timer: any = null;

onMounted(() => {
  timer = setInterval(() => {
    currentTipIndex.value = (currentTipIndex.value + 1) % props.tips.length;
  }, 2200);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
});
</script>

<template>
  <div class="lemon-scanner-overlay">
    <!-- 雷达扫描与波纹动效容器 -->
    <div class="radar-box">
      <div class="ripple r1"></div>
      <div class="ripple r2"></div>
      <div class="ripple r3"></div>
      <div class="radar-core">
        <div class="hex-badge-spin">
          <div class="hex-shape outer">
            <div class="hex-shape inner">
              <div class="hex-center-dot"></div>
            </div>
          </div>
        </div>
      </div>
      <div class="scanner-sweep"></div>
    </div>

    <!-- 扫描文案与实时滚动提示 -->
    <div class="scanner-title">{{ title }}</div>
    <div class="scanner-tip-box">
      <transition name="fade-slide" mode="out-in">
        <span :key="currentTipIndex" class="scanner-tip">
          {{ tips[currentTipIndex] || subTitle }}
        </span>
      </transition>
    </div>

    <!-- 底部线性进度呼吸条 -->
    <div class="progress-bar-container">
      <div class="progress-bar-active"></div>
    </div>
  </div>
</template>

<style scoped>
.lemon-scanner-overlay {
  width: 100%;
  height: 100%;
  min-height: 420px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: #ffffff;
  user-select: none;
}

.radar-box {
  width: 160px;
  height: 160px;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 28px;
}

/* 扩散波纹 */
.ripple {
  position: absolute;
  border-radius: 50%;
  border: 1px solid rgba(46, 204, 139, 0.45);
  box-shadow: 0 0 12px rgba(46, 204, 139, 0.15);
  animation: ripple-anim 3s infinite cubic-bezier(0.2, 0.6, 0.3, 1);
}

.r1 {
  width: 80px;
  height: 80px;
  animation-delay: 0s;
}

.r2 {
  width: 110px;
  height: 110px;
  animation-delay: 0.9s;
}

.r3 {
  width: 140px;
  height: 140px;
  animation-delay: 1.8s;
}

@keyframes ripple-anim {
  0% {
    transform: scale(0.6);
    opacity: 0.9;
  }
  100% {
    transform: scale(1.6);
    opacity: 0;
  }
}

/* 雷达扫掠效果 */
.scanner-sweep {
  position: absolute;
  width: 140px;
  height: 140px;
  border-radius: 50%;
  background: conic-gradient(
    from 0deg,
    rgba(46, 204, 139, 0.35) 0deg,
    rgba(46, 204, 139, 0.05) 60deg,
    transparent 120deg,
    transparent 360deg
  );
  animation: sweep-spin 2.2s linear infinite;
  pointer-events: none;
}

@keyframes sweep-spin {
  0% {
    transform: rotate(0deg);
  }
  100% {
    transform: rotate(360deg);
  }
}

/* 核心 Lemon 风格六边形 */
.radar-core {
  position: relative;
  z-index: 2;
}

.hex-badge-spin {
  animation: float-pulse 3s infinite ease-in-out;
}

.hex-shape {
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  display: flex;
  align-items: center;
  justify-content: center;
}

.outer {
  width: 72px;
  height: 72px;
  background: linear-gradient(135deg, #fcd34d 0%, #f59e0b 100%);
  box-shadow: 0 4px 16px rgba(245, 158, 11, 0.35);
}

.inner {
  width: 54px;
  height: 54px;
  background: #fffbeb;
}

.hex-center-dot {
  width: 20px;
  height: 20px;
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  background: #f59e0b;
}

@keyframes float-pulse {
  0%, 100% {
    transform: scale(1);
  }
  50% {
    transform: scale(1.08);
  }
}

.scanner-title {
  font-size: 18px;
  font-weight: 600;
  color: #1f2937;
  letter-spacing: 0.5px;
  margin-bottom: 8px;
}

.scanner-tip-box {
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 24px;
}

.scanner-tip {
  font-size: 13px;
  color: #9ca3af;
}

.fade-slide-enter-active,
.fade-slide-leave-active {
  transition: all 0.3s ease;
}

.fade-slide-enter-from {
  opacity: 0;
  transform: translateY(6px);
}

.fade-slide-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

.progress-bar-container {
  width: 240px;
  height: 4px;
  background: #f3f4f6;
  border-radius: 999px;
  overflow: hidden;
  position: relative;
}

.progress-bar-active {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 40%;
  background: linear-gradient(90deg, #10b981, #2ecc71);
  border-radius: 999px;
  animation: progress-indet 1.8s infinite ease-in-out;
}

@keyframes progress-indet {
  0% {
    left: -40%;
    width: 30%;
  }
  50% {
    left: 40%;
    width: 60%;
  }
  100% {
    left: 100%;
    width: 30%;
  }
}
</style>
