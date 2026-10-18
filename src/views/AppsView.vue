<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, onDeactivated } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { ElMessage } from 'element-plus';
import { Package, Search, RotateCw, Monitor, HardDrive, Layers, ShieldCheck, Info, ListChecks, FolderOpen } from 'lucide-vue-next';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';
import { runTask, activeTask, cancelTask } from '../tasks';
interface AppItem { key:string;name:string;pkg_type:string;version:string;size:string;size_bytes:number;description:string;icon:string;desktop_file:string;is_system:boolean;has_ui:boolean;can_uninstall:boolean;install_path:string;uninstall_note:string }
interface AppChannel {id:string;status:string;count:number|null;note:string}
interface ScanReport {apps:AppItem[];channels:AppChannel[];warnings:string[];checked_at:string}
interface UninstallItem {path:string;name:string;kind:string;size_bytes:number;size:string;selected:boolean;required:boolean;owners:string[];group:string}
interface ManualPlan {id:string;name:string;note:string;items:UninstallItem[]}
interface PackagePlan {id:string;name:string;pkg_type:string;key:string;packages:{name:string;version:string;size_bytes:number|null}[];estimated_size:string;mode:string;note:string;supported:boolean;review_identity:string}
interface QueueItem {app:AppItem;manual?:ManualPlan;plan?:PackagePlan;paths:string[];completed:string[];status:'pending'|'running'|'success'|'failed'|'unsupported';error:string}
interface Profile {app:AppItem;caches:{path:string;size_bytes:number|null;note:string}[];startup:{path:string;name:string;enabled:boolean}[];update:{current:string;latest:string;channel:string;checked_at:string}|null;notes:string[]}
interface UninstallProgress {request_id:string;message:string;percentage:number|null;done?:number;total?:number}
const desktopAvailable=isTauri();
const scanning=ref(desktopAvailable),loaded=ref(false),loadError=ref(''),apps=ref<AppItem[]>([]),report=ref<ScanReport|null>(null);
const appSearch=ref(''),activeCategory=ref<'user'|'all'>('user'),source=ref('all'),sort=ref('name');
const selectedApps=ref<AppItem[]>([]),failedIcons=ref(new Set<string>());
const channels=computed(()=>[...new Set(apps.value.map(a=>a.pkg_type))].sort());
const filteredApps=computed(()=>apps.value.filter(a=>(activeCategory.value==='all'||a.has_ui)&&(source.value==='all'||a.pkg_type===source.value)&&(!appSearch.value||`${a.name} ${a.key} ${a.description}`.toLowerCase().includes(appSearch.value.toLowerCase()))).sort((a,b)=>sort.value==='size'?b.size_bytes-a.size_bytes||a.name.localeCompare(b.name):sort.value==='source'?a.pkg_type.localeCompare(b.pkg_type)||a.name.localeCompare(b.name):a.name.localeCompare(b.name)));
const userAppsCount=computed(()=>apps.value.filter(a=>a.has_ui).length);
const installedSize=computed(()=>formatBytes(apps.value.reduce((sum,a)=>sum+a.size_bytes,0)));
const unknownSizes=computed(()=>apps.value.filter(a=>!a.size_bytes && a.size!=='0 B').length);
function formatBytes(bytes:number|null){if(bytes===null)return '未知';return bytes>=1024**3?(bytes/1024**3).toFixed(2)+' GB':bytes>=1024**2?(bytes/1024**2).toFixed(1)+' MB':bytes>=1024?(bytes/1024).toFixed(1)+' KB':bytes+' B';}
function appRowKey(app:AppItem){return `${app.pkg_type}:${app.key}`;}
function iconFailed(key:string){failedIcons.value=new Set([...failedIcons.value,key]);}
async function loadApps(){
  if(scanning.value && loaded.value || uninstalling.value)return;
  scanning.value=true;loadError.value='';
  try{report.value=await runTask<ScanReport>('get_installed_apps_report');apps.value=report.value.apps;loaded.value=true;selectedApps.value=[];}
  catch(error){loadError.value=String(error);ElMessage.error('应用读取未完成：'+error);}
  finally{scanning.value=false;}
}
const uninstallDialog=ref(false),planLoading=ref(false),uninstalling=ref(false),queue=ref<QueueItem[]>([]),queueIndex=ref(0),stopQueue=ref(false);
const currentEntry=computed(()=>queue.value[queueIndex.value]);
const uninstallProgress=ref<UninstallProgress>({request_id:'',message:'',percentage:null});
const selectedUninstallSize=computed(()=>formatBytes(currentEntry.value?.manual?.items.filter(i=>currentEntry.value?.paths.includes(i.path)).reduce((sum,i)=>sum+i.size_bytes,0)??null));
let unlisten:UnlistenFn|undefined;let disposed=false;
function itemDisabled(entry:QueueItem,item:UninstallItem){return uninstalling.value||item.required||entry.completed.includes(item.path)||(item.kind!=='ide'&&item.owners?.some(owner=>!entry.paths.includes(owner)&&!entry.completed.includes(owner)));}
function selectPlanItem(entry:QueueItem,item:UninstallItem,checked:boolean){
  if(itemDisabled(entry,item))return;
  const selected=new Set(entry.paths);if(checked)selected.add(item.path);else selected.delete(item.path);
  if(item.kind==='ide'){
    for(const related of entry.manual?.items??[]){if(related.path===item.path||!related.owners?.includes(item.path))continue;
      if(!checked)selected.delete(related.path);
      else if(related.selected&&related.owners.every(owner=>selected.has(owner)||entry.completed.includes(owner)))selected.add(related.path);
    }
  }
  entry.paths=[...selected];
}
async function prepareQueue(targets:AppItem[]){
  if(uninstalling.value||planLoading.value)return;
  const candidates=targets.filter(a=>a.can_uninstall&&!a.is_system);if(!candidates.length)return;
  queue.value=[];queueIndex.value=0;uninstallDialog.value=true;planLoading.value=true;
  try{for(const app of candidates){
    queue.value.push({app,paths:[],completed:[],status:'pending',error:''});const entry=queue.value[queue.value.length-1]!;
    try{if(app.pkg_type==='manual'){entry.manual=await invoke<ManualPlan>('get_manual_uninstall_plan',{key:app.key});entry.paths=entry.manual.items.filter(i=>i.selected||i.required).map(i=>i.path);}
      else{entry.plan=await invoke<PackagePlan>('get_package_uninstall_plan',{pkgType:app.pkg_type,key:app.key});if(!entry.plan.supported){entry.status='unsupported';entry.error=entry.plan.note;}}
    }catch(error){entry.status='unsupported';entry.error=String(error);}
  }}finally{planLoading.value=false;}
}
async function refreshPreview(entry:QueueItem){
  if(uninstalling.value||planLoading.value||entry.status==='success')return;
  planLoading.value=true;
  try{
    if(entry.manual){if(entry.completed.length)throw new Error('已有完成项，请使用原预览重试未完成项');entry.manual=await invoke<ManualPlan>('get_manual_uninstall_plan',{key:entry.app.key});entry.paths=entry.manual.items.filter(i=>i.selected||i.required).map(i=>i.path);}
    else{entry.plan=await invoke<PackagePlan>('get_package_uninstall_plan',{pkgType:entry.app.pkg_type,key:entry.app.key});if(!entry.plan.supported){entry.status='unsupported';entry.error=entry.plan.note;return;}}
    entry.status='pending';entry.error='';
  }catch(error){entry.status='failed';entry.error=String(error);}
  finally{planLoading.value=false;}
}
async function reconcileManual(entry:QueueItem){
  if(!entry.manual)return;
  try{const status=await invoke<{completed:string[]}>('get_manual_uninstall_status',{planId:entry.manual.id});entry.completed=status.completed;
    const programs=entry.manual.items.filter(i=>['program','ide'].includes(i.kind)&&status.completed.some(path=>i.path===path||i.path.startsWith(path+'/'))).map(i=>i.path);
    apps.value=apps.value.filter(a=>!programs.some(path=>a.install_path&&(a.install_path===path||a.install_path.startsWith(path+'/'))));
    if(entry.manual.items.some(i=>i.kind==='program'&&i.required&&status.completed.includes(i.path)))apps.value=apps.value.filter(a=>appRowKey(a)!==appRowKey(entry.app));
  }catch(error){entry.error+=(entry.error?'；':'')+'完成明细读取失败：'+error;}
}
async function reconcilePackage(entry:QueueItem){
  if(!entry.plan)return;
  try{
    const status=await invoke<{removed:{pkg_type:string;key:string}[];remaining:{pkg_type:string;key:string}[];unknown:string[]}>('get_package_uninstall_status',{planId:entry.plan.id});
    apps.value=apps.value.filter(app=>!status.removed.some(identity=>app.pkg_type===identity.pkg_type&&(app.key===identity.key||app.pkg_type==='deb'&&app.key.split(':')[0]===identity.key.split(':')[0])));
    if(status.unknown.length){entry.error+=(entry.error?'；':'')+'完成状态尚未确认：'+status.unknown.join('；');if(entry.status==='success')entry.status='failed';}
    else if(status.remaining.length&&entry.status==='success'){entry.status='failed';entry.error='原渠道返回完成，但以下安装身份仍存在：'+status.remaining.map(i=>i.key).join('、');}
    else if(!status.remaining.length&&status.removed.length){entry.status='success';entry.error='';}
  }catch(error){entry.error+=(entry.error?'；':'')+'完成明细读取失败：'+error;if(entry.status==='success')entry.status='failed';}
}
function scopeSignature(plan:PackagePlan){return JSON.stringify({packages:plan.packages,mode:plan.mode,key:plan.key,pkg_type:plan.pkg_type,review_identity:plan.review_identity});}
async function executeQueue(failedOnly=false){
  if(uninstalling.value||planLoading.value)return;
  const targets=queue.value.filter(e=>failedOnly?e.status==='failed':e.status==='pending');
  if(!targets.length)return;uninstalling.value=true;stopQueue.value=false;
  try{for(const entry of targets){
    if(stopQueue.value)break;queueIndex.value=queue.value.indexOf(entry);entry.status='running';entry.error='';
    const requestId=`uninstall-${Date.now()}-${Math.random().toString(36).slice(2,8)}`;uninstallProgress.value={request_id:requestId,message:'正在核对卸载范围…',percentage:null};
    try{let message:string;
      if(entry.manual){message=await runTask<string>('uninstall_manual_app',{planId:entry.manual.id,paths:entry.paths,requestId});}
      else if(entry.plan){
        const refreshed=await invoke<PackagePlan>('get_package_uninstall_plan',{pkgType:entry.app.pkg_type,key:entry.app.key});
        if(!refreshed.supported||scopeSignature(refreshed)!==scopeSignature(entry.plan))throw new Error('影响范围或版本已变化，请重新预览并确认');
        entry.plan=refreshed;message=await runTask<string>('uninstall_app_plan',{planId:entry.plan.id,requestId});
      }else throw new Error('缺少卸载预览');
      entry.status='success';uninstallProgress.value.percentage=100;ElMessage.success(message);
    }catch(error){entry.status='failed';entry.error=String(error);}
    finally{if(entry.manual)await reconcileManual(entry);else await reconcilePackage(entry);}
  }}finally{uninstalling.value=false;selectedApps.value=[];}
}
const profileDialog=ref(false),profileLoading=ref(false),profile=ref<Profile|null>(null),profileError=ref('');
async function openProfile(app:AppItem){profileDialog.value=true;profileLoading.value=true;profile.value=null;profileError.value='';try{profile.value=await invoke<Profile>('get_app_profile',{pkgType:app.pkg_type,key:app.key});}catch(error){profileError.value=String(error);}finally{profileLoading.value=false;}}
async function reveal(path:string){try{await revealItemInDir(path);}catch(error){ElMessage.error('无法定位：'+error);}}
onMounted(async()=>{if(!desktopAvailable)return;try{const stop=await listen<UninstallProgress>('uninstall-progress',event=>{if(event.payload.request_id===uninstallProgress.value.request_id)uninstallProgress.value=event.payload;});if(disposed){stop();return;}unlisten=stop;}catch{}if(!disposed)await loadApps();});
onUnmounted(()=>{disposed=true;unlisten?.();});
onDeactivated(()=>{uninstallDialog.value=false;profileDialog.value=false;});
</script>
<template>
  <div class="subpage-container">
    <PageHeader title="应用管理" subtitle="把安装来源、关联数据与每次卸载的影响放在一起。">
      <template #actions><el-button v-if="queue.length" @click="uninstallDialog=true"><ListChecks :size="15"/>卸载队列 {{ queue.length }}</el-button><el-button @click="loadApps" :loading="scanning" :disabled="!desktopAvailable||uninstalling"><RotateCw :size="15"/>刷新列表</el-button></template>
    </PageHeader>
    <div class="subpage-content">
      <div v-if="!desktopAvailable" class="desktop-notice"><Monitor :size="38"/><h2>你的应用，一目了然</h2><p>请在 Cleaner 桌面应用中查看本机已安装软件。</p><span>浏览器预览不会读取应用列表</span></div>
      <template v-else>
        <div v-if="scanning" class="scan-controls"><span>扫描在后台继续，可以切换页面</span><el-button v-if="activeTask('get_installed_apps_report')" @click="cancelTask(activeTask('get_installed_apps_report')!.id)">停止扫描</el-button></div>
        <ScanningRadar v-if="scanning&&!loaded" title="正在识别你的应用" sub-title="逐个读取安装渠道，并记录未能读取的范围" :tips="['读取桌面入口与独立安装结构…','核对系统包的手动安装标记…','识别 Flatpak 安装范围与完整应用标识…']"/>
        <div v-else class="table-container">
          <section class="apps-summary">
            <div class="summary-stat"><span class="summary-icon"><Monitor :size="18"/></span><div><strong>{{ loaded?userAppsCount:'—' }}</strong><span>已识别桌面应用</span></div></div>
            <div class="summary-stat"><span class="summary-icon cyan"><Layers :size="18"/></span><div><strong>{{ loaded?apps.length:'—' }}</strong><span>已识别应用与工具</span></div></div>
            <div class="summary-stat"><span class="summary-icon soft"><HardDrive :size="18"/></span><div><strong>{{ loaded?installedSize:'—' }}</strong><span>已统计占用 · {{ unknownSizes }} 项体积未知</span></div></div>
          </section>
          <div v-if="loadError" class="coverage-warning" role="alert">应用读取未完成：{{ loadError }}</div>
          <div v-if="report" class="channel-strip"><el-tooltip v-for="channel in report.channels" :key="channel.id" :content="channel.note"><el-tag :type="channel.status==='ok'?'success':channel.status==='unavailable'?'info':'warning'" size="small">{{ channel.id.toUpperCase() }} · {{ channel.status==='unavailable'?'未安装':channel.status==='failed'?'失败，数量未知':channel.status==='partial'?'部分读取':channel.status==='cancelled'?'已停止':channel.count===null?'已查询':channel.count+' 项' }}</el-tag></el-tooltip><span class="checked-time">{{ new Date(report.checked_at).toLocaleTimeString() }}</span></div>
          <details v-if="report?.warnings.length" class="coverage-warning"><summary>有 {{ report.warnings.length }} 条覆盖提示，结果可能不完整</summary><p v-for="warning in report.warnings" :key="warning">{{ warning }}</p></details>
          <div v-if="loaded" class="apps-toolbar">
            <el-radio-group v-model="activeCategory" size="small"><el-radio-button value="user">桌面应用</el-radio-button><el-radio-button value="all">全部工具</el-radio-button></el-radio-group>
            <el-select v-model="source" class="source-select" aria-label="安装渠道"><el-option label="全部安装渠道" value="all"/><el-option v-for="channel in channels" :key="channel" :label="channel.toUpperCase()" :value="channel"/></el-select>
            <el-select v-model="sort" class="sort-select" aria-label="应用排序"><el-option label="按名称" value="name"/><el-option label="占用从大到小" value="size"/><el-option label="按安装渠道" value="source"/></el-select>
            <el-input v-model="appSearch" placeholder="搜索名称、软件包或描述" clearable class="app-search"><template #prefix><Search :size="15"/></template></el-input>
          </div>
          <div v-if="loaded" class="list-meta"><span>{{ filteredApps.length }} 项 · 已选 {{ selectedApps.length }} 项 <el-button v-if="selectedApps.length" link type="danger" :disabled="uninstalling||planLoading" @click="prepareQueue(selectedApps)">逐项预览卸载</el-button></span><span><ShieldCheck :size="12"/>系统组件受保护</span></div>
          <div v-if="loaded" class="app-list"><el-table :data="filteredApps" :row-key="appRowKey" height="100%" stripe style="width:100%" :empty-text="appSearch?'没有找到匹配应用':'此分类没有已识别应用'" @selection-change="selectedApps=$event">
            <el-table-column type="selection" width="42" :selectable="(app:AppItem)=>app.can_uninstall&&!app.is_system&&!uninstalling"/>
            <el-table-column label="应用名称" min-width="245"><template #default="{row}"><div class="app-identity"><img v-if="row.icon.startsWith('data:image/')&&!failedIcons.has(row.key)" :src="row.icon" alt="" class="app-icon" @error="iconFailed(row.key)"/><span v-else class="app-icon app-icon-fallback"><Package :size="23"/></span><div class="app-name-cell"><button class="app-display-name app-detail-link" @click="openProfile(row)">{{ row.name }}</button><span class="app-pkg-name" :title="row.install_path||row.key">{{ row.install_path||row.key }}</span></div></div></template></el-table-column>
            <el-table-column label="安装渠道" width="112"><template #default="{row}"><el-tag size="small" :type="row.pkg_type==='flatpak'?'success':'primary'">{{ row.pkg_type==='manual'?'独立应用':row.pkg_type.toUpperCase() }}</el-tag></template></el-table-column>
            <el-table-column prop="version" label="版本" min-width="125" show-overflow-tooltip/>
            <el-table-column prop="size" label="占用空间" width="115"/>
            <el-table-column prop="description" label="描述" min-width="180" show-overflow-tooltip/>
            <el-table-column label="操作" width="130" fixed="right" align="center"><template #default="{row}"><el-button link @click="openProfile(row)">详情</el-button><el-tooltip :content="row.uninstall_note||(row.is_system?'系统组件受保护':'查看影响预览')"><span><el-button type="danger" link :disabled="row.is_system||!row.can_uninstall||uninstalling||planLoading" @click="prepareQueue([row])">卸载</el-button></span></el-tooltip></template></el-table-column>
          </el-table></div>
        </div>
      </template>
    </div>
    <el-dialog v-model="uninstallDialog" :title="`卸载队列 · ${queue.length} 个应用`" width="min(880px,calc(100vw - 40px))" :close-on-click-modal="false" :close-on-press-escape="!uninstalling&&!planLoading" :show-close="!uninstalling&&!planLoading">
      <div v-loading="planLoading" class="uninstall-preview">
        <p class="uninstall-note">逐项查看处理范围。队列串行执行，失败项保留原因；关闭窗口后可从顶部重新打开队列。</p>
        <div class="queue-tabs"><button v-for="(entry,index) in queue" :key="appRowKey(entry.app)" :class="{active:index===queueIndex}" @click="queueIndex=index"><strong>{{ entry.app.name }}</strong><span>{{ entry.status==='success'?'已完成':entry.status==='failed'?'失败，可重试':entry.status==='unsupported'?'需原渠道处理':entry.status==='running'?'正在处理':'待确认' }}</span></button></div>
        <div v-if="uninstalling" class="uninstall-progress" role="status"><strong>正在处理 {{ queue.find(e=>e.status==='running')?.app.name }}</strong><el-progress :percentage="uninstallProgress.percentage??100" :indeterminate="uninstallProgress.percentage===null" :show-text="uninstallProgress.percentage!==null"/><p>{{ uninstallProgress.message }}</p></div>
        <template v-if="currentEntry">
          <div v-if="currentEntry.error" class="coverage-warning" role="alert">{{ currentEntry.error }}<el-button v-if="!currentEntry.completed.length" link :disabled="uninstalling||planLoading" @click="refreshPreview(currentEntry)">重新查看此应用预览</el-button><el-button v-if="currentEntry.plan&&currentEntry.status==='failed'" link :disabled="uninstalling||planLoading" @click="reconcilePackage(currentEntry)">重新核对完成状态</el-button></div>
          <template v-if="currentEntry.manual">
            <p class="uninstall-note">{{ currentEntry.manual.note }}</p><p class="uninstall-summary">已选 {{ currentEntry.paths.length }} 项 · {{ selectedUninstallSize }}<span v-if="currentEntry.completed.length"> · 已完成 {{ currentEntry.completed.length }} 项</span></p>
            <div class="uninstall-items"><div v-for="item in currentEntry.manual.items" :key="item.path" class="uninstall-item"><el-checkbox :model-value="currentEntry.paths.includes(item.path)" :disabled="itemDisabled(currentEntry,item)||currentEntry.status==='success'" :aria-label="item.name" @change="selectPlanItem(currentEntry,item,Boolean($event))"/><div class="uninstall-item-copy"><strong>{{ item.name }}<el-tag size="small" :type="item.kind==='config'?'info':'warning'">{{ currentEntry.completed.includes(item.path)?'已完成':item.kind==='config'?'配置默认保留':item.group }}</el-tag></strong><span>{{ item.path }}</span></div><span class="uninstall-size">{{ item.size }}</span></div></div>
          </template>
          <template v-else-if="currentEntry.plan"><p class="uninstall-note">{{ currentEntry.plan.note }}</p><p class="uninstall-summary">{{ currentEntry.plan.mode==='simulation'?'原生仿真范围':currentEntry.plan.mode==='exact-scope'?'精确安装身份范围':'范围无法确认' }} · 预计移除 {{ currentEntry.plan.estimated_size }}</p><div class="uninstall-items"><div v-for="pkg in currentEntry.plan.packages" :key="pkg.name" class="uninstall-item"><div class="uninstall-item-copy"><strong>{{ pkg.name }}</strong><span>{{ pkg.version||'版本未知' }}</span></div><span class="uninstall-size">{{ formatBytes(pkg.size_bytes) }}</span></div></div></template>
        </template>
      </div>
      <template #footer><el-button :disabled="uninstalling||planLoading" @click="uninstallDialog=false">关闭</el-button><el-button v-if="uninstalling" @click="stopQueue=true" :disabled="stopQueue">{{ stopQueue?'完成当前项后停止':'完成当前项后停止队列' }}</el-button><el-button v-if="queue.some(e=>e.status==='failed')" :disabled="uninstalling||planLoading" @click="executeQueue(true)">重试失败项</el-button><el-button type="danger" :loading="uninstalling" :disabled="!queue.some(e=>e.status==='pending')||planLoading" @click="executeQueue()">确认所选范围并执行 {{ queue.filter(e=>e.status==='pending').length }} 项</el-button></template>
    </el-dialog>
    <el-dialog v-model="profileDialog" :title="`${profile?.app.name??'应用'} · 详情与关联`" width="min(760px,calc(100vw - 40px))">
      <div v-loading="profileLoading" class="profile-body"><p v-if="profileError" class="coverage-warning">{{ profileError }}</p><template v-if="profile"><div class="profile-id"><Package :size="32"/><div><strong>{{ profile.app.name }}</strong><span>{{ profile.app.pkg_type.toUpperCase() }} · {{ profile.app.version||'版本未知' }} · {{ profile.app.size||'占用未知' }}</span></div></div><p class="profile-path">{{ profile.app.install_path||profile.app.key }}</p><p class="uninstall-note">{{ profile.app.description }}</p>
      <h3>已确认缓存</h3><p v-if="!profile.caches.length" class="uninstall-note">尚无可可靠关联的缓存路径</p><div v-for="cache in profile.caches" :key="cache.path" class="profile-related"><div><strong>{{ formatBytes(cache.size_bytes) }}</strong><span>{{ cache.path }}</span><span v-if="cache.note">{{ cache.note }}</span></div><el-button link @click="reveal(cache.path)"><FolderOpen :size="15"/>定位</el-button></div>
      <h3>登录启动关联</h3><p v-if="!profile.startup.length" class="uninstall-note">尚无匹配实际程序路径的启动项</p><div v-for="item in profile.startup" :key="item.path" class="profile-related"><div><strong>{{ item.name }}</strong><span>{{ item.path }}</span></div><el-tag size="small">{{ item.enabled?'配置已启用':'配置已关闭' }}</el-tag></div>
      <h3>已检测更新</h3><p class="uninstall-note">{{ profile.update?`${profile.update.channel} · ${profile.update.current||'当前版本未知'} → ${profile.update.latest||'目标版本未知'} · ${new Date(profile.update.checked_at).toLocaleString()}`:'暂无已检测并可靠匹配的更新，请在软件更新页检查' }}</p><p v-for="note in profile.notes" :key="note" class="profile-note"><Info :size="13"/>{{ note }}</p>
      </template></div><template #footer><el-button @click="profileDialog=false">关闭</el-button></template>
    </el-dialog>
  </div>
</template>
<style scoped>
.source-select { width: 145px; }.sort-select { width: 130px; }
.channel-strip { display:flex;gap:7px;align-items:center;flex-wrap:wrap;margin:0 0 14px; }.checked-time {font-size:10px;color:#8d80a0;margin-left:auto;}
.coverage-warning {color:#d5ba90;background:#d5ba900b;border:1px solid #d5ba9028;border-radius:10px;padding:11px 14px;font-size:12px;line-height:1.7;margin-bottom:12px;overflow-wrap:anywhere;max-height:150px;overflow:auto;}.coverage-warning summary {cursor:pointer;}.coverage-warning p{margin:5px 0;}
.scan-controls {display:flex;justify-content:space-between;align-items:center;font-size:12px;color:#a699bf;gap:12px;margin-bottom:12px;}
.app-detail-link {border:0;background:transparent;padding:0;text-align:left;cursor:pointer;}.app-detail-link:hover{color:#cbb5ff;text-decoration:underline;}
.queue-tabs {display:flex;gap:8px;overflow-x:auto;margin-bottom:15px;}.queue-tabs button {display:flex;flex-direction:column;gap:6px;flex-shrink:0;max-width:190px;text-align:left;padding:12px 15px;border-radius:10px;border:1px solid #ffffff13;background:#ffffff05;color:#b6a7cc;cursor:pointer;}.queue-tabs button.active{border-color:#ad90ed60;background:#ad90ed15;}.queue-tabs strong{font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}.queue-tabs span{font-size:10px;color:#a190ba;}
.profile-body {min-height:130px;}.profile-id{display:flex;align-items:center;gap:14px;color:#bba1f1;}.profile-id>div{display:flex;flex-direction:column;gap:6px;}.profile-id strong{font-size:20px;color:#eee4ff;}.profile-id span{font-size:12px;color:#998aac;}.profile-path{font-size:12px;color:#b9abcb;overflow-wrap:anywhere;user-select:text;}.profile-body h3{font-size:14px;color:#e0d2f5;margin:23px 0 10px;}.profile-related{display:flex;align-items:center;gap:12px;padding:12px;border:1px solid #ffffff0d;border-radius:10px;margin-bottom:7px;}.profile-related>div{flex:1;min-width:0;display:flex;flex-direction:column;gap:5px;}.profile-related strong{font-size:12px;color:#cbbbdf;}.profile-related span{font-size:11px;color:#9180a3;overflow-wrap:anywhere;user-select:text;}.profile-note{display:flex;align-items:flex-start;gap:7px;font-size:11px;color:#8a7b9b;line-height:1.7;}.profile-note svg{flex-shrink:0;margin-top:3px;}
:deep(.el-button svg.lucide) { margin-right: 6px; flex-shrink: 0; }
.subpage-container { display: flex; flex-direction: column; height: 100%; min-width: 0; color: #f1efff; background: transparent; }
.subpage-content { display: flex; flex-direction: column; flex: 1; min-height: 0; padding: 16px 28px 28px; overflow: hidden; position: relative; }
.table-container { display: flex; flex-direction: column; flex: 1; min-height: 0; }
.apps-summary { display: grid; grid-template-columns: 1fr 1fr 1.15fr; border: 1px solid #ffffff0c; border-radius: 16px; background: #ffffff03; padding: 22px 0; margin-bottom: 24px; }
.summary-stat { display: flex; align-items: center; gap: 13px; padding: 0 20px; border-left: 1px solid #ffffff0a; }
.summary-stat:first-child { border-left: 0; }
.summary-icon { display: grid; place-items: center; width: 39px; height: 39px; flex-shrink: 0; border-radius: 12px; background: #a78bfa14; color: #b5a0ee; }
.summary-icon.cyan { background: #64d5e910; color: #64d5e9; }
.summary-icon.soft { background: #96a7f414; color: #96a7f4; }
.summary-stat > div { display: flex; flex-direction: column; gap: 6px; }
.summary-stat strong { font-size: 23px; line-height: 1; font-weight: 550; letter-spacing: -.5px; white-space: nowrap; }
.summary-stat > div > span { font-size: 10px; color: #92879f; }
.apps-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 15px; flex-wrap: wrap; }
.app-search { width: 235px; flex-shrink: 0; }
.app-search :deep(.el-input__prefix) { color: #8e80a6; }
.list-meta { display: flex; justify-content: space-between; align-items: center; gap: 10px; margin: 20px 2px 12px; font-size: 10px; color: #8f849f; }
.list-meta span:last-child { display: flex; align-items: center; gap: 5px; color: #64d5e99e; }
.app-list { flex: 1; min-height: 0; overflow: hidden; border: 1px solid #ffffff0c; border-radius: 13px; background: #ffffff02; }
.app-identity { display: flex; align-items: center; gap: 12px; min-width: 0; padding: 3px 0; }
.app-icon { width: 39px; height: 39px; object-fit: contain; flex-shrink: 0; }
.app-icon-fallback { display: grid; place-items: center; border-radius: 11px; background: linear-gradient(145deg, #a78bfa20, #a78bfa0c); color: #b5a0ee; border: 1px solid #a78bfa1c; }
.app-name-cell { min-width: 0; display: flex; flex-direction: column; gap: 5px; }
.app-display-name, .app-pkg-name { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.app-display-name { font-weight: 500; color: #e9e0f6; font-size: 12px; }
.app-pkg-name { font-size: 10px; color: #877a9b; }
.uninstall-progress { padding: 16px 18px; margin-bottom: 16px; border: 1px solid #a78bfa26; background: #a78bfa0a; border-radius: 12px; }
.uninstall-progress > div:first-child { display: flex; justify-content: space-between; margin-bottom: 12px; font-size: 12px; }
.uninstall-progress strong { font-weight: 500; color: #c6b0f3; }
.uninstall-progress p { margin: 10px 0 0; font-size: 11px; color: #9c8daf; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.uninstall-preview { min-height: 110px; }
.uninstall-note { color: #a296b4; font-size: 12px; line-height: 1.8; }
.uninstall-summary { color: #c7acf7; font-size: 13px; padding: 12px 14px; background: #a78bfa10; border-radius: 8px; }
.uninstall-items { max-height: 330px; overflow: auto; border: 1px solid #ffffff0b; border-radius: 10px; }
.uninstall-item { display: flex; gap: 12px; align-items: center; padding: 12px 14px; border-top: 1px solid #ffffff08; }
.uninstall-item:first-child { border-top: 0; }
.uninstall-item-copy { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 5px; }
.uninstall-item-copy strong { display: flex; gap: 8px; align-items: center; font-size: 13px; font-weight: 500; color: #e4daf1; }
.uninstall-item-copy span { font-size: 11px; color: #91829f; overflow-wrap: anywhere; user-select: text; }
.uninstall-size { width: 85px; text-align: right; font-size: 12px; color: #b0a0c3; font-variant-numeric: tabular-nums; }
.load-error { display: flex; flex-direction: column; align-items: center; justify-content: center; flex: 1; min-height: 220px; text-align: center; padding: 24px 20px; }
.load-error > span { display: grid; place-items: center; width: 66px; height: 66px; border: 1px solid #dfb77b20; border-radius: 21px; color: #dfb77b; background: #dfb77b0b; }
.load-error h2 { font-size: 18px; font-weight: 500; color: #e7ddf4; margin: 19px 0 10px; }
.load-error p { font-size: 12px; color: #9583aa; line-height: 1.8; max-width: 500px; overflow-wrap: anywhere; user-select: text; margin: 0 0 20px; }
.desktop-notice { display: flex; flex-direction: column; align-items: center; justify-content: center; flex: 1; text-align: center; padding: 40px 20px; color: #a78bfa; }
.desktop-notice h2 { font-size: 19px; font-weight: 500; color: #e5dcf6; margin: 20px 0 9px; }
.desktop-notice p { color: #9486ac; font-size: 13px; margin: 0; }
.desktop-notice span { margin-top: 10px; color: #6e637f; font-size: 11px; }
@media (max-width: 900px) { .subpage-content { padding: 16px 20px 22px; } .summary-stat { padding: 0 15px; gap: 10px; } .summary-icon { width: 33px; height: 33px; } .summary-stat strong { font-size: 20px; } .apps-toolbar { gap: 10px; } .app-search { width: 220px; } }
</style>
