<script setup lang="ts">
import { useRouter } from 'vue-router';

const router = useRouter();

function startQuickScan() {
  router.push('/clean');
}
</script>

<template>
  <div class="lemon-main-window">
    <!-- 模拟 macOS / 原生精致红黄绿红绿灯圆点 -->
    <div class="mac-traffic-lights">
      <div class="traffic-light close"></div>
      <div class="traffic-light minimize"></div>
      <div class="traffic-light maximize"></div>
    </div>

    <div class="lemon-content-split">
      <!-- 左侧品牌与扫描主操作区 (像素级还原 Lemon Cleaner) -->
      <div class="lemon-left-panel">
        <div class="lemon-logo-container">
          <!-- 悬浮浅色大六边形底衬 -->
          <div class="logo-bg-hex-large"></div>
          <div class="logo-bg-hex-small-1"></div>
          <div class="logo-bg-hex-small-2"></div>
          <div class="logo-bg-dot"></div>

          <!-- 核心黄色金标双层六边形 -->
          <div class="lemon-hex-badge">
            <div class="hex-outer-band">
              <div class="hex-white-gap">
                <div class="hex-inner-gold">
                  <div class="hex-core-white"></div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <h1 class="lemon-slogan">畅快清理，清爽一下</h1>
        <p class="lemon-sub-welcome">欢迎使用 Lemon Cleaner · Linux 软件管家</p>

        <!-- 极具代表性的高饱和柠檬绿按钮 -->
        <button class="lemon-scan-button" @click="startQuickScan">
          开始扫描
        </button>

        <!-- 装饰性四个小方块金菱形 -->
        <div class="lemon-bottom-decor">
          <span class="decor-diamond"></span>
          <span class="decor-diamond"></span>
          <span class="decor-diamond"></span>
          <span class="decor-diamond"></span>
        </div>
      </div>

      <!-- 右侧功能网格 (2列4行，1:1 还原图标线框与排版) -->
      <div class="lemon-right-panel">
        <div class="features-grid">
          <!-- 1. 大文件清理 -->
          <div class="lemon-feature-item" @click="router.push('/large-files')">
            <div class="feature-icon-wrapper">
              <!-- 大文件图标：文件夹+纸张 -->
              <svg class="lemon-svg-icon" viewBox="0 0 48 48" fill="none">
                <path d="M8 12C8 9.79086 9.79086 8 12 8H20L24 14H36C38.2091 14 40 15.7909 40 18V36C40 38.2091 38.2091 40 36 40H12C9.79086 40 8 38.2091 8 36V12Z" stroke="#5a5e66" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
                <path d="M22 24H32M22 30H28" stroke="#5a5e66" stroke-width="3" stroke-linecap="round"/>
              </svg>
            </div>
            <div class="feature-info">
              <div class="feature-title">大文件清理</div>
              <div class="feature-subtitle">查找 50MB 以上的文件</div>
            </div>
          </div>

          <!-- 2. 重复文件清理 -->
          <div class="lemon-feature-item" @click="router.push('/duplicates')">
            <div class="feature-icon-wrapper">
              <!-- 重复文件图标：四个边角框 -->
              <svg class="lemon-svg-icon" viewBox="0 0 48 48" fill="none">
                <path d="M12 18V12H18M30 12H36V18M36 30V36H30M18 36H12V30" stroke="#5a5e66" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </div>
            <div class="feature-info">
              <div class="feature-title">重复文件清理</div>
              <div class="feature-subtitle">查找内容一样的文件</div>
            </div>
          </div>

          <!-- 3. 系统垃圾清理 (对应原版相似照片位) -->
          <div class="lemon-feature-item" @click="router.push('/clean')">
            <div class="feature-icon-wrapper">
              <!-- 垃圾桶图标 -->
              <svg class="lemon-svg-icon" viewBox="0 0 48 48" fill="none">
                <path d="M12 14H36M18 14V10C18 8.89543 18.8954 8 20 8H28C29.1046 8 30 8.89543 30 10V14M14 14L16 38C16 39.1046 16.8954 40 18 40H30C31.1046 40 32 39.1046 32 38L34 14" stroke="#5a5e66" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </div>
            <div class="feature-info">
              <div class="feature-title">系统垃圾清理</div>
              <div class="feature-subtitle">快速清理缓存、死链与日志</div>
            </div>
          </div>

          <!-- 4. 应用卸载 -->
          <div class="lemon-feature-item" @click="router.push('/apps')">
            <div class="feature-icon-wrapper">
              <!-- 强力卸载箱 -->
              <svg class="lemon-svg-icon" viewBox="0 0 48 48" fill="none">
                <rect x="10" y="14" width="28" height="26" rx="2" stroke="#5a5e66" stroke-width="3"/>
                <path d="M8 14H40" stroke="#5a5e66" stroke-width="3" stroke-linecap="round"/>
                <path d="M20 22V32M28 22V32" stroke="#5a5e66" stroke-width="3" stroke-linecap="round"/>
              </svg>
            </div>
            <div class="feature-info">
              <div class="feature-title">应用卸载</div>
              <div class="feature-subtitle">强力卸载已安装应用</div>
            </div>
          </div>

          <!-- 5. 软件更新检测 (APT / DNF / YUM) -->
          <div class="lemon-feature-item" @click="router.push('/updates')">
            <div class="feature-icon-wrapper">
              <!-- 盾牌/更新 -->
              <svg class="lemon-svg-icon" viewBox="0 0 48 48" fill="none">
                <path d="M24 6L10 12V22C10 31.5 16 38.5 24 42C32 38.5 38 31.5 38 22V12L24 6Z" stroke="#5a5e66" stroke-width="3" stroke-linejoin="round"/>
                <path d="M18 24L22 28L30 18" stroke="#5a5e66" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </div>
            <div class="feature-info">
              <div class="feature-title">软件更新检测</div>
              <div class="feature-subtitle">自动检测 APT / DNF / YUM 源更新</div>
            </div>
          </div>

          <!-- 6. 开机启动项管理 -->
          <div class="lemon-feature-item" @click="router.push('/autostart')">
            <div class="feature-icon-wrapper">
              <!-- 开关电源图标 -->
              <svg class="lemon-svg-icon" viewBox="0 0 48 48" fill="none">
                <path d="M24 8V22" stroke="#5a5e66" stroke-width="3.5" stroke-linecap="round"/>
                <path d="M15 14C11.5 17 9 21.5 9 26.5C9 34.5 15.5 41 23.5 41C31.5 41 38 34.5 38 26.5C38 21.5 35.5 17 32 14" stroke="#5a5e66" stroke-width="3.2" stroke-linecap="round"/>
              </svg>
            </div>
            <div class="feature-info">
              <div class="feature-title">开机启动项管理</div>
              <div class="feature-subtitle">快速禁用开机自启应用</div>
            </div>
          </div>

          <!-- 7. 桌面与菜单图标 -->
          <div class="lemon-feature-item" @click="router.push('/entries')">
            <div class="feature-icon-wrapper">
              <!-- 磁盘/条目图标 -->
              <svg class="lemon-svg-icon" viewBox="0 0 48 48" fill="none">
                <rect x="10" y="8" width="28" height="32" rx="3" stroke="#5a5e66" stroke-width="3"/>
                <circle cx="24" cy="24" r="6" stroke="#5a5e66" stroke-width="3"/>
                <circle cx="24" cy="14" r="2" fill="#5a5e66"/>
              </svg>
            </div>
            <div class="feature-info">
              <div class="feature-title">桌面与菜单图标</div>
              <div class="feature-subtitle">管理、隐藏或恢复启动器条目</div>
            </div>
          </div>

          <!-- 8. 柠檬精选 / 关于 -->
          <div class="lemon-feature-item" @click="router.push('/updates')">
            <div class="feature-icon-wrapper">
              <!-- 精选四宫格图标 -->
              <svg class="lemon-svg-icon" viewBox="0 0 48 48" fill="none">
                <rect x="10" y="10" width="11" height="11" rx="2.5" fill="#c4c7cc"/>
                <rect x="27" y="10" width="11" height="11" rx="2.5" fill="#c4c7cc"/>
                <rect x="10" y="27" width="11" height="11" rx="2.5" fill="#c4c7cc"/>
                <rect x="27" y="27" width="11" height="11" rx="2.5" stroke="#5a5e66" stroke-width="3"/>
              </svg>
            </div>
            <div class="feature-info">
              <div class="feature-title">柠檬精选</div>
              <div class="feature-subtitle">发现更多优质 Linux 应用</div>
            </div>
          </div>
        </div>

        <!-- 底部右下角微文字反馈 -->
        <div class="lemon-feedback">
          <svg class="feedback-msg-icon" viewBox="0 0 16 16" fill="currentColor">
            <path d="M2 3C2 1.89543 2.89543 1 4 1H12C13.1046 1 14 1.89543 14 3V10C14 11.1046 13.1046 12 12 12H5.5L2 15V3Z"/>
          </svg>
          <span>分享宝贵建议</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.lemon-main-window {
  width: 100vw;
  height: 100vh;
  background-color: #ffffff;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
  user-select: none;
}

/* 顶部红黄绿按钮 */
.mac-traffic-lights {
  position: absolute;
  top: 14px;
  left: 16px;
  display: flex;
  gap: 8px;
  z-index: 10;
}

.traffic-light {
  width: 12px;
  height: 12px;
  border-radius: 50%;
}

.traffic-light.close {
  background-color: #ff5f57;
}

.traffic-light.minimize {
  background-color: #febc2e;
}

.traffic-light.maximize {
  background-color: #28c840;
}

.lemon-content-split {
  display: flex;
  width: 100%;
  height: 100%;
}

/* 左侧白底高雅展示区 */
.lemon-left-panel {
  width: 44%;
  min-width: 380px;
  background: #ffffff;
  border-right: 1px solid #f6f6f7;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px;
  position: relative;
}

/* 六边形主视觉 */
.lemon-logo-container {
  width: 240px;
  height: 240px;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 24px;
}

/* 浅色半透明背景六边形 */
.logo-bg-hex-large {
  position: absolute;
  width: 220px;
  height: 220px;
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  background-color: #f8fafc;
  transform: rotate(-15deg);
  opacity: 0.85;
}

.logo-bg-hex-small-1 {
  position: absolute;
  width: 32px;
  height: 32px;
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  background-color: #fed7aa;
  top: 24px;
  right: 12px;
  opacity: 0.6;
}

.logo-bg-hex-small-2 {
  position: absolute;
  width: 22px;
  height: 22px;
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  background-color: #fde68a;
  bottom: 24px;
  left: 12px;
  opacity: 0.7;
}

/* 黄色主标志 (双层六边形) */
.lemon-hex-badge {
  position: relative;
  z-index: 2;
  width: 160px;
  height: 160px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.hex-outer-band {
  width: 154px;
  height: 154px;
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  background: linear-gradient(180deg, #fbb019 0%, #f6941d 100%);
  display: flex;
  align-items: center;
  justify-content: center;
}

.hex-white-gap {
  width: 114px;
  height: 114px;
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  background: #ffffff;
  display: flex;
  align-items: center;
  justify-content: center;
}

.hex-inner-gold {
  width: 80px;
  height: 80px;
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  background: linear-gradient(180deg, #fbb019 0%, #f6941d 100%);
  display: flex;
  align-items: center;
  justify-content: center;
}

.hex-core-white {
  width: 48px;
  height: 48px;
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  background: #ffffff;
}

.lemon-slogan {
  font-size: 28px;
  font-weight: 600;
  color: #272c35;
  letter-spacing: 0.6px;
  margin: 0 0 10px 0;
}

.lemon-sub-welcome {
  font-size: 14px;
  color: #9aa0a6;
  margin: 0 0 36px 0;
}

/* 绿色主扫描按钮 */
.lemon-scan-button {
  width: 172px;
  height: 48px;
  background-color: #2ecc71;
  border: none;
  border-radius: 4px;
  color: #ffffff;
  font-size: 17px;
  font-weight: 500;
  letter-spacing: 1px;
  cursor: pointer;
  box-shadow: 0 4px 14px rgba(46, 204, 113, 0.35);
  transition: all 0.2s ease;
}

.lemon-scan-button:hover {
  background-color: #27ae60;
  transform: translateY(-1px);
  box-shadow: 0 6px 18px rgba(46, 204, 113, 0.45);
}

.lemon-scan-button:active {
  transform: translateY(0);
}

/* 左下角小金菱形 */
.lemon-bottom-decor {
  position: absolute;
  bottom: 24px;
  left: 28px;
  display: grid;
  grid-template-columns: repeat(2, 6px);
  gap: 3px;
  transform: rotate(45deg);
}

.decor-diamond {
  width: 6px;
  height: 6px;
  background-color: #fbb019;
  border-radius: 1px;
}

/* 右侧网格区域 */
.lemon-right-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 44px 48px 24px 36px;
  background-color: #ffffff;
}

.features-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  column-gap: 36px;
  row-gap: 32px;
}

.lemon-feature-item {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 8px;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.lemon-feature-item:hover {
  background-color: #f8fafc;
  transform: translateX(3px);
}

.feature-icon-wrapper {
  width: 44px;
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.lemon-svg-icon {
  width: 38px;
  height: 38px;
}

.feature-info {
  display: flex;
  flex-direction: column;
}

.feature-title {
  font-size: 15px;
  font-weight: 500;
  color: #2c3e50;
  margin-bottom: 3px;
}

.feature-subtitle {
  font-size: 12px;
  color: #8f959e;
}

/* 右下角建议反馈 */
.lemon-feedback {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
  font-size: 12px;
  color: #8c939d;
  cursor: pointer;
}

.lemon-feedback:hover {
  color: #5c626c;
}

.feedback-msg-icon {
  width: 14px;
  height: 14px;
  fill: currentColor;
}
</style>
