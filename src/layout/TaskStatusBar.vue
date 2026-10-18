<script setup lang="ts">
import { computed,ref } from 'vue';
import { isTauri } from '@tauri-apps/api/core';
import { ElMessage } from 'element-plus';
import { Activity, ChevronUp, X, CircleCheck, TriangleAlert, Square, LoaderCircle, Cpu, MemoryStick, HardDrive } from 'lucide-vue-next';
import { taskList,taskLabels,cancelTask } from '../tasks';
import { preferences,savePreferences } from '../preferences';
import { monitorSnapshot,monitorError,formatBytes } from '../monitor';
const opened=ref(false);const native=isTauri();
const running=computed(()=>taskList.value.filter(t=>['running','cancelling'].includes(t.state)));
const memory=computed(()=>{const s=monitorSnapshot.value;return s?.memory_total&&s.memory_used!=null?Math.round(s.memory_used/s.memory_total*100):null;});
async function stop(id:string){try{await cancelTask(id);}catch(error){ElMessage.error(String(error));}}
async function toggleMonitor(value:boolean){try{await savePreferences({monitor_enabled:value});}catch(error){ElMessage.error(String(error));}}
</script>
<template>
  <footer class="status-bar">
    <button class="task-toggle" :aria-expanded="opened" @click="opened=!opened"><LoaderCircle v-if="running.length" class="spin" :size="13"/><Activity v-else :size="13"/><span>{{ running.length ? `${running.length} 个任务进行中` : '任务中心' }}</span><ChevronUp :size="12" :class="{rotated:opened}"/></button>
    <router-link to="/monitor" class="resources" :title="monitorError || '查看本机资源与近期采样'" v-if="native && preferences.monitor_enabled"><span><Cpu :size="12"/>{{ monitorSnapshot?.cpu_percent==null?'—':`${monitorSnapshot.cpu_percent.toFixed(0)}%` }}</span><span><MemoryStick :size="12"/>{{ memory==null?'—':`${memory}%` }}</span><span><HardDrive :size="12"/>可用 {{ formatBytes(monitorSnapshot?.disk_available) }}</span></router-link>
    <span v-else class="status-note">{{ native?'资源监控已关闭':'浏览器预览 · 本机数据未连接' }}</span>
    <section v-if="opened" class="task-panel" aria-label="任务中心">
      <div class="task-heading"><div><strong>任务与本机状态</strong><p>切换页面后，进行中的任务仍会保留。</p></div><button aria-label="关闭任务中心" @click="opened=false"><X :size="17"/></button></div>
      <div v-if="native" class="monitor-setting"><span>轻量资源监控 · 每 2.5 秒采样，磁盘每 30 秒刷新</span><el-switch :model-value="preferences.monitor_enabled" aria-label="启用本机资源监控" @change="toggleMonitor(Boolean($event))"/></div>
      <div v-if="!taskList.length" class="no-tasks">还没有任务，扫描和处理结果会显示在这里。</div>
      <div v-for="task in taskList" :key="task.id" class="task-row"><span class="task-state"><LoaderCircle v-if="['running','cancelling'].includes(task.state)" class="spin" :size="16"/><TriangleAlert v-else-if="task.state==='failed'" :size="16"/><Square v-else-if="task.state==='cancelled'" :size="14"/><CircleCheck v-else :size="16"/></span><div class="task-copy"><strong>{{ taskLabels[task.kind]??task.kind }}<small>{{ ({running:'进行中',cancelling:'正在停止',completed:'已完成',cancelled:'已停止',failed:'未完成'})[task.state] }}</small></strong><p>{{ task.phase }}<span v-if="task.current"> · {{ task.current }}{{ task.total ? ` / ${task.total}` : '' }}</span></p><code v-if="task.path" :title="task.path">{{ task.path }}</code></div><button v-if="task.cancellable&&task.state==='running'" class="stop-task" @click="stop(task.id)">停止</button></div>
    </section>
  </footer>
</template>
<style scoped>
.status-bar{height:33px;flex-shrink:0;display:flex;align-items:center;justify-content:space-between;padding:0 22px;border-top:1px solid var(--line);background:#141120;position:relative;z-index:8;color:#9c8fae;font-size:10px}.task-toggle,.task-heading button{display:flex;align-items:center;gap:7px;border:0;background:none;color:inherit;cursor:pointer;padding:4px 0}.task-toggle:hover,.resources:hover{color:#cfb9f7}.resources{display:flex;gap:20px;text-decoration:none;color:#a89aba}.resources span{display:flex;align-items:center;gap:6px}.status-note{font-size:9px;color:#766b88}.rotated{transform:rotate(180deg)}.spin{animation:spin 2s linear infinite}.task-panel{position:absolute;bottom:42px;right:18px;width:460px;max-width:calc(100vw - 36px);max-height:min(560px,calc(100vh - 85px));overflow:auto;border:1px solid #a78bfa30;background:#201a30f5;backdrop-filter:blur(20px);border-radius:16px;padding:19px;box-shadow:0 15px 65px #0008}.task-heading{display:flex;justify-content:space-between;align-items:flex-start}.task-heading strong{font-size:15px;color:#ede4ff}.task-heading p{font-size:10px;color:#9386a7;margin:7px 0 18px}.monitor-setting{display:flex;justify-content:space-between;align-items:center;font-size:10px;margin-bottom:10px;gap:16px}.task-row{display:flex;gap:11px;padding:13px 0;border-top:1px solid #bba7e014;align-items:flex-start}.task-state{color:#baa0e8;padding-top:2px}.task-copy{flex:1;min-width:0}.task-copy strong{font-size:12px;color:#daceed;font-weight:500}.task-copy small{font-size:9px;color:#a292b9;margin-left:9px}.task-copy p{font-size:10px;margin:6px 0;color:#a092b5;line-height:1.6;overflow-wrap:anywhere}.task-copy code{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-size:9px;color:#82748f;user-select:text}.stop-task{border:1px solid #a78bfa35;border-radius:7px;background:#a78bfa13;color:#bda4ec;padding:5px 9px;font-size:10px;cursor:pointer}.no-tasks{padding:28px 0;text-align:center;font-size:11px}@keyframes spin{to{transform:rotate(360deg)}}@media(max-width:700px){.resources{gap:9px}.resources span:last-child{display:none}}@media(prefers-reduced-motion:reduce){.spin{animation:none}}
</style>
