<script setup lang="ts">
import { computed } from 'vue';
import { isTauri } from '@tauri-apps/api/core';
import { useRouter } from 'vue-router';
import { ArrowUpRight, FolderSearch, HardDrive, PackageMinus, ScanLine, Sparkles, Trash2 } from 'lucide-vue-next';
import AuroraCore from '../layout/AuroraCore.vue';
import { platformInfo } from '../platform';
import { taskResults,taskLabels } from '../tasks';
const recentResults=computed(()=>Object.entries(taskResults.value).filter(([key])=>key.startsWith('scan_')||key==='check_software_updates'||key==='get_installed_apps_report').sort((a,b)=>b[1].at-a[1].at).slice(0,2));

const router = useRouter();
const native = isTauri();
const systemLabel = computed(() => {
  if (!native) return '浏览器预览';
  const system = platformInfo.value?.os;
  return system === 'macos' ? 'macOS 工作空间' : system === 'linux' ? 'Linux 工作空间' : '本地工作空间';
});

const features = [
  { title: '垃圾清理', subtitle: '整理缓存与历史日志', icon: Trash2, path: '/clean', tone: 'violet' },
  { title: '大文件探索', subtitle: '找到占用空间的大文件', icon: FolderSearch, path: '/large-files', tone: 'cyan' },
  { title: '应用管理', subtitle: '卸载应用，整理残留', icon: PackageMinus, path: '/apps', tone: 'pink' },
  { title: '空间分析', subtitle: '看清每个目录的占用', icon: HardDrive, path: '/storage', tone: 'blue' },
];

function startScan() {
  void router.push({ path: '/clean', query: { scan: Date.now().toString() } });
}
</script>

<template>
  <main class="dashboard">
    <div class="welcome-line">
      <span class="workspace-label"><span class="workspace-dot" />{{ systemLabel }}</span>
      <span class="welcome-note">给你的电脑，留一点余地。</span>
    </div>

    <section class="hero" aria-labelledby="dashboard-title">
      <div class="hero-copy">
        <div class="hero-eyebrow"><Sparkles :size="14" :stroke-width="1.6" /> A LITTLE SPACE. A NEW START.</div>
        <h1 id="dashboard-title">让空间，<br /><span>重新自由。</span></h1>
        <p class="hero-description">清理日积月累的冗余，<br />让每一次打开，都轻盈一点。</p>
        <button class="primary-scan" @click="startScan">
          <ScanLine :size="20" :stroke-width="1.8" />
          <span>开始扫描</span>
          <ArrowUpRight :size="18" :stroke-width="1.8" />
        </button>
        <p class="scan-footnote">先扫描，再由你决定清理哪些内容</p>
      </div>

      <div class="hero-visual" aria-hidden="true">
        <AuroraCore />
        <div class="visual-caption"><span /> SPACE TO BREATHE</div>
      </div>
    </section>

    <section class="tools-section" aria-labelledby="tools-title">
      <div v-if="recentResults.length" class="recent-results"><div v-for="[kind,result] in recentResults" :key="kind"><span>{{ taskLabels[kind] }}</span><strong>{{ result.summary }}</strong><small>{{ new Date(result.at).toLocaleTimeString() }} · 本次会话</small></div></div>
      <div class="section-heading">
        <h2 id="tools-title">你的空间，由你掌控</h2>
        <span>从这里开始</span>
      </div>
      <div class="features-grid">
        <button v-for="feature in features" :key="feature.path" class="feature" :class="feature.tone" @click="router.push(feature.path)">
          <span class="feature-icon"><component :is="feature.icon" :size="23" :stroke-width="1.65" aria-hidden="true" /></span>
          <span class="feature-copy"><strong>{{ feature.title }}</strong><small>{{ feature.subtitle }}</small></span>
          <ArrowUpRight class="feature-arrow" :size="17" :stroke-width="1.6" aria-hidden="true" />
        </button>
      </div>
    </section>
  </main>
</template>

<style scoped>
.dashboard { height: 100%; min-width: 0; padding: 29px 36px 30px; color: #f1efff; position: relative; overflow: auto; overflow-x: hidden; background: radial-gradient(ellipse at 82% 28%, #8654ce14, transparent 48%), radial-gradient(ellipse at 70% 60%, #54c4db06, transparent 45%); }
.welcome-line { display: flex; align-items: center; justify-content: space-between; gap: 16px; position: relative; z-index: 1; }
.workspace-label { display: inline-flex; align-items: center; gap: 8px; font-size: 11px; color: #bbb5d0; }
.workspace-dot { width: 5px; height: 5px; background: #64d5e9; border-radius: 50%; box-shadow: 0 0 9px #64d5e965; }
.welcome-note { color: #78758c; font-size: 11px; }
.hero { display: grid; grid-template-columns: 1fr 1fr; align-items: center; min-height: 354px; position: relative; }
.hero-copy { position: relative; z-index: 2; padding: 28px 0 25px; }
.hero-eyebrow { display: flex; align-items: center; gap: 7px; color: #aca0cd; font-size: 8px; font-weight: 600; letter-spacing: 1.8px; white-space: nowrap; margin-bottom: 17px; }
.hero-eyebrow svg { color: #b99af8; }
h1 { font-size: clamp(36px, 4.4vw, 54px); line-height: 1.19; letter-spacing: -1.8px; margin: 0; font-weight: 650; }
h1 span { background: linear-gradient(95deg, #c7b3ff 2%, #9fbcf9 58%, #8ee1e7); background-clip: text; -webkit-text-fill-color: transparent; }
.hero-description { font-size: 12px; line-height: 1.85; margin: 17px 0 21px; color: #9693ad; }
.primary-scan { display: inline-flex; align-items: center; justify-content: center; gap: 12px; border: 1px solid #cfb6ff4d; background: linear-gradient(115deg, #9365e8, #6d5bce); color: #fff; font: inherit; font-size: 13px; font-weight: 600; padding: 13px 21px; border-radius: 13px; box-shadow: 0 8px 28px #7746cd35, inset 0 1px 0 #ffffff26; cursor: pointer; transition: transform .2s ease, box-shadow .2s ease, filter .2s ease; }
.primary-scan svg:last-child { margin-left: 12px; opacity: .8; }
.primary-scan:hover { transform: translateY(-2px); box-shadow: 0 10px 35px #9365e860, inset 0 1px 0 #ffffff26; filter: brightness(1.1); }
.primary-scan:active { transform: translateY(0); }
.primary-scan:focus-visible, .feature:focus-visible { outline: 2px solid #c7b3ff; outline-offset: 4px; }
.scan-footnote { margin: 11px 0 0; color: #77728d; font-size: 10px; }
.hero-visual { width: calc(100% + 45px); max-width: 390px; justify-self: center; position: relative; margin-left: -3px; }
.visual-caption { display: flex; justify-content: center; align-items: center; gap: 7px; font-size: 8px; letter-spacing: 2.5px; color: #88829e; margin-top: -23px; }
.visual-caption span { width: 3px; height: 3px; background: #a78bfa; border-radius: 50%; }
.tools-section { position: relative; z-index: 1; }
.recent-results{display:flex;flex-wrap:wrap;gap:10px;margin-bottom:18px}.recent-results>div{flex:1;min-width:220px;padding:13px 15px;border:1px solid #a78bfa20;border-radius:12px;background:#a78bfa08}.recent-results span{display:block;font-size:10px;color:#9a8aae}.recent-results strong{display:block;font-size:12px;color:#d8c6ef;font-weight:500;margin:6px 0}.recent-results small{font-size:9px;color:#837193}
.section-heading { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 14px; }
.section-heading h2 { font-size: 13px; font-weight: 500; margin: 0; color: #d7d0e9; }
.section-heading > span { font-size: 10px; color: #736e87; }
.features-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
.feature { --feature-accent: #b397ff; display: flex; align-items: center; gap: 12px; min-width: 0; border: 1px solid #b9a4ef13; border-radius: 14px; background: linear-gradient(120deg, #d4c1ff08, #d4c1ff03); color: #f1efff; padding: 16px 14px; cursor: pointer; text-align: left; font-family: inherit; transition: background .2s ease, border-color .2s ease, transform .2s ease; }
.feature:hover { border-color: #a78bfa4a; background: #ba9bff0e; transform: translateY(-2px); }
.feature.cyan { --feature-accent: #79d6db; }
.feature.pink { --feature-accent: #df9bbe; }
.feature.blue { --feature-accent: #94b5f5; }
.feature-icon { width: 40px; height: 40px; flex-shrink: 0; display: grid; place-items: center; color: var(--feature-accent); background: #b39afa0a; border: 1px solid #bba4fa13; border-radius: 11px; }
.feature-copy { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.feature-copy strong { font-size: 12px; font-weight: 500; color: #dcd7eb; }
.feature-copy small { font-size: 10px; color: #827d96; white-space: nowrap; }
.feature-arrow { margin-left: auto; flex-shrink: 0; color: #777086; opacity: .6; }
@media (max-width: 900px) { .dashboard { padding: 26px 26px 24px; } .hero { min-height: 325px; } h1 { font-size: 40px; } .hero-visual { width: calc(100% + 25px); } .feature { gap: 10px; padding: 14px 12px; } .feature-copy small { font-size: 9px; } }
@media (max-width: 720px) { .dashboard { padding: 24px 20px; } .hero { grid-template-columns: 1.06fr .94fr; min-height: 320px; } .welcome-note { display: none; } h1 { font-size: 35px; } .hero-eyebrow { font-size: 7px; letter-spacing: 1px; } .hero-visual { width: calc(100% + 45px); margin-left: -15px; } .visual-caption { font-size: 6px; letter-spacing: 1.7px; margin-top: -15px; } .primary-scan { padding: 12px 17px; gap: 8px; } .feature-icon { width: 34px; height: 34px; } .feature-arrow { display: none; } }
@media (max-width: 520px) { .hero { grid-template-columns: 1fr; } .hero-visual { position: absolute; right: -55px; top: 25px; width: 270px; opacity: .35; } .hero-copy { width: 100%; } .features-grid { grid-template-columns: 1fr; } .feature-arrow { display: block; } .visual-caption { display: none; } }
@media (max-height: 700px) and (min-width: 721px) { .dashboard { padding-top: 22px; } .hero { min-height: 315px; } .hero-visual { max-width: 335px; } .hero-copy { padding: 23px 0; } h1 { font-size: 43px; } .feature { padding-top: 12px; padding-bottom: 12px; } }
@media (max-height: 620px) and (min-width: 721px) { .dashboard { padding: 20px 26px; } .hero { min-height: 282px; } .hero-copy { padding: 18px 0; } .hero-eyebrow { margin-bottom: 12px; } h1 { font-size: 38px; } .hero-description { margin: 12px 0 16px; } .hero-visual { max-width: 285px; } .feature { padding-top: 10px; padding-bottom: 10px; } .section-heading { margin-bottom: 11px; } }
@media (prefers-reduced-motion: reduce) { .primary-scan, .feature { transition: none; } .primary-scan:hover, .feature:hover { transform: none; } }
</style>
