<script setup lang="ts">
import { computed,onDeactivated,ref } from 'vue';
import { isTauri } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import { Activity,Cpu,MemoryStick,HardDrive,ArrowDownLeft,ArrowUpRight,SlidersHorizontal } from 'lucide-vue-next';
import PageHeader from '../layout/PageHeader.vue';
import SummaryDetails from '../components/SummaryDetails.vue';
import MiniChart from '../components/MiniChart.vue';
import TraySettings from '../components/TraySettings.vue';
import {monitorSnapshot,monitorHistory,monitorError,formatBytes,formatRate} from '../monitor';
import {preferences,savePreferences} from '../preferences';
import {platformInfo} from '../platform';
const native=isTauri(),settingsDialog=ref(false);
const memory=computed(()=>{const s=monitorSnapshot.value;return s?.memory_total&&s.memory_used!=null?s.memory_used/s.memory_total*100:null;});
const free=computed(()=>{const s=monitorSnapshot.value;return s?.disk_total&&s.disk_available!=null?s.disk_available/s.disk_total*100:null;});
function points(kind:'cpu'|'memory'){return monitorHistory.value.map((s,i)=>{const value=kind==='cpu'?s.cpu_percent:s.memory_total&&s.memory_used!=null?s.memory_used/s.memory_total*100:null;return value==null?null:`${i*600/Math.max(1,monitorHistory.value.length-1)},${90-value*.8}`}).filter(Boolean).join(' ');}
async function toggle(value:boolean){try{await savePreferences({monitor_enabled:value});}catch(error){ElMessage.error(String(error));}}
onDeactivated(()=>{settingsDialog.value=false;});
</script>
<template>
  <main class="monitor-page">
    <PageHeader title="本机监控" subtitle="轻量观察资源使用与后台任务，不扫描应用文件。"><template #actions>
      <template v-if="native"><SummaryDetails label="tips" title="采样说明" :count="monitorSnapshot?.warnings.length||undefined" :tone="monitorSnapshot?.warnings.length?'warning':undefined"><p v-if="monitorSnapshot">最近采样：{{ new Date(monitorSnapshot.sampled_at).toLocaleTimeString() }} · 磁盘每 30 秒刷新</p><p>首次 CPU 和网速需要两个采样点；缺失数据显示为 —。网速按实际时间差统计，自动模式合计物理网卡，不重复计算 VPN 隧道流量。</p><p>最近最多 60 次记录仅保留在本次会话。暂停后保留最后数据，菜单栏与主窗口共享采样设置。</p><p v-for="note in monitorSnapshot?.warnings" :key="note" class="detail-message warning">{{ note }}</p></SummaryDetails><SummaryDetails v-if="monitorError" label="采样未完成" :count="1" tone="warning"><p>{{ monitorError }}</p></SummaryDetails><el-button v-if="platformInfo?.os==='macos'" @click="settingsDialog=true"><SlidersHorizontal :size="14"/>菜单栏设置</el-button></template><el-switch :model-value="preferences.monitor_enabled" :disabled="!native" active-text="资源采样" aria-label="启用资源采样" @change="toggle(Boolean($event))"/>
    </template></PageHeader>
    <div class="monitor-content">
      <div v-if="!native" class="notice"><Activity :size="40"/><h2>本机资源，在这里看清</h2><p>请在桌面应用中查看 CPU、内存和网络统计。</p></div>
      <template v-else>
        <p v-if="!preferences.monitor_enabled" class="sampling-status">采样已关闭 · 保留最后结果</p>
        <div class="metrics"><article><Cpu/><span>CPU 使用率</span><strong>{{ monitorSnapshot?.cpu_percent==null?'—':monitorSnapshot.cpu_percent.toFixed(1)+'%' }}</strong><p>两次采样之间的平均使用率</p></article><article><MemoryStick/><span>内存使用</span><strong>{{ memory==null?'—':memory.toFixed(1)+'%' }}</strong><p>{{ formatBytes(monitorSnapshot?.memory_used) }} / {{ formatBytes(monitorSnapshot?.memory_total) }}</p></article><article><HardDrive/><span>磁盘可用</span><strong>{{ formatBytes(monitorSnapshot?.disk_available) }}</strong><p>{{ free==null?'—':free.toFixed(1)+'% 可用' }} · 用户目录所在磁盘</p></article></div>
        <section class="network-section"><div class="network-heading"><h2>网络流量</h2><small :title="monitorSnapshot?.network_interface">{{ monitorSnapshot?.network_interface||'等待网卡采样' }}</small></div><div class="network-grid"><article class="download"><span><ArrowDownLeft :size="15"/>下载速度</span><strong>{{ formatRate(monitorSnapshot?.network_down,preferences.tray.rate_unit==='bits') }}</strong><MiniChart :values="monitorHistory.map(s=>s.network_down??null)" label="近期下载速度"/></article><article class="upload"><span><ArrowUpRight :size="15"/>上传速度</span><strong>{{ formatRate(monitorSnapshot?.network_up,preferences.tray.rate_unit==='bits') }}</strong><MiniChart :values="monitorHistory.map(s=>s.network_up??null)" label="近期上传速度"/></article></div></section>
        <section class="chart"><div><h2>近期采样</h2><span><i class="cpu"/>CPU <i class="memory"/>内存</span></div><svg viewBox="0 0 600 110" role="img" aria-label="最近最多60次 CPU 与内存使用率采样"><path d="M0 10H600M0 50H600M0 90H600" stroke="var(--line)"/><polyline :points="points('cpu')" fill="none" stroke="var(--accent)" stroke-width="2"/><polyline :points="points('memory')" fill="none" stroke="var(--cyan)" stroke-width="2"/></svg><p>{{ monitorHistory.length }} 次采样 · 每 {{ preferences.tray.interval_ms/1000 }} 秒自动刷新</p></section>
      </template>
    </div>
    <el-dialog v-model="settingsDialog" title="自定义菜单栏" width="min(460px,90vw)" append-to-body><div class="settings-body"><TraySettings/></div></el-dialog>
  </main>
</template>
<style scoped>
.monitor-page{height:100%;display:flex;flex-direction:column}.monitor-content{padding:20px 28px;overflow:auto;flex:1;min-height:0}.metrics{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px;margin:0 0 16px}.metrics article{position:relative;padding:22px;border:1px solid var(--accent-border);border-radius:16px;background:radial-gradient(ellipse at 100% 0,rgba(var(--accent-rgb),.09),transparent),rgba(var(--neutral-rgb),.01)}.metrics svg{width:19px;color:var(--accent);position:absolute;right:20px;top:20px}.metrics span{font-size:11px;color:var(--text-muted);display:block}.metrics strong{display:block;font-size:28px;color:var(--text);margin:18px 0 8px;font-weight:500}.metrics p{font-size:10px;color:var(--text-faint);margin:0;line-height:1.7}.chart,.network-section{border:1px solid var(--line);background:var(--surface-subtle);padding:20px;border-radius:16px}.chart>div,.network-heading{display:flex;align-items:center;justify-content:space-between}.chart h2,.network-heading h2{font-size:14px;font-weight:500;margin:0}.chart span{font-size:10px;color:var(--text-muted);display:flex;align-items:center;gap:7px}.chart i{width:5px;height:5px;border-radius:50%;display:inline-block}.cpu{background:var(--accent)}.memory{background:var(--cyan)}.chart svg{width:100%;height:150px;margin-top:20px}.chart p{font-size:10px;color:var(--text-faint);line-height:1.8}.network-section{margin-bottom:16px}.network-heading small{font-size:10px;color:var(--text-faint);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:60%}.network-grid{display:grid;grid-template-columns:1fr 1fr;gap:40px;margin-top:18px}.network-grid article>span{display:flex;align-items:center;gap:6px;font-size:11px;color:var(--text-muted)}.network-grid strong{display:block;font-size:25px;font-weight:500;margin:14px 0;color:var(--text);font-variant-numeric:tabular-nums}.download{color:var(--accent)}.upload{color:var(--cyan)}.sampling-status{margin:0 0 14px;font-size:11px;color:var(--text-faint)}.notice{display:flex;min-height:350px;align-items:center;justify-content:center;flex-direction:column;color:var(--accent)}.notice h2{font-size:20px;color:var(--text)}.notice p{font-size:12px;color:var(--text-muted)}.settings-body{max-height:65vh;overflow-y:auto;padding:0 3px}.header-actions :deep(.el-button svg){margin-right:6px}@media(max-width:900px){.monitor-content{padding:18px 20px}.metrics article{padding:18px 15px}.metrics strong{font-size:23px}}@media(max-width:700px){.metrics{grid-template-columns:1fr}.chart svg{height:120px}.network-grid{gap:20px}}
</style>
