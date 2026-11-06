<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { ref } from 'vue';
import { ElMessage } from 'element-plus';
import { FolderPlus, X } from 'lucide-vue-next';
import { preferences, savePreferences } from '../preferences';
import SummaryDetails from './SummaryDetails.vue';
const props = defineProps<{ disabled?: boolean; developer?: boolean; exclusionsOnly?: boolean; embedded?: boolean; popup?: boolean; minMb?: number }>();
const emit = defineEmits<{ 'update:minMb': [value: number] }>();
const manualPath=ref('');
const manualTarget=ref<'scan'|'keep'>('scan');
async function addManual(exclusion=false){
  const path=manualPath.value.trim();if(!path.startsWith('/')){ElMessage.warning('请输入绝对路径');return;}
  const key=exclusion?'exclusions':props.developer?'developer_roots':'scan_roots';
  try{await savePreferences(current=>({[key]:[...new Set([...current[key],path])]}));manualPath.value='';}catch(error){ElMessage.error('保存目录失败：'+error);}
}
async function add(exclusion = false) {
  try {
    const path = await invoke<string | null>('pick_directory', { title: exclusion ? '选择需要保留的目录' : '选择扫描目录' });
    if (!path) return;
    const key = exclusion ? 'exclusions' : props.developer ? 'developer_roots' : 'scan_roots';
    await savePreferences(current => ({ [key]: [...new Set([...current[key], path])] }));
  } catch (error) { ElMessage.error(String(error)); }
}
async function remove(path: string, exclusion = false) {
  const key = exclusion ? 'exclusions' : props.developer ? 'developer_roots' : 'scan_roots';
  try{await savePreferences(current => ({ [key]: current[key].filter(value => value !== path) }));}catch(error){ElMessage.error('保存目录失败：'+error);}
}
</script>
<template>
  <SummaryDetails v-if="popup" :label="exclusionsOnly ? '保留名单' : '扫描设置'" :count="exclusionsOnly ? preferences.exclusions.length : `${(developer ? preferences.developer_roots : preferences.scan_roots).length ? (developer ? preferences.developer_roots : preferences.scan_roots).length + ' 个目录' : developer ? '工具缓存' : '常用目录'} · ${preferences.exclusions.length} 项保留`">
    <ScanScope embedded :disabled="disabled" :developer="developer" :exclusions-only="exclusionsOnly" :min-mb="minMb" @update:min-mb="emit('update:minMb', $event)" />
  </SummaryDetails>
  <component v-else :is="embedded ? 'div' : 'details'" class="scan-scope" :class="{ embedded }">
    <summary v-if="!embedded">{{ exclusionsOnly ? '保留名单' : '扫描设置' }} <span>{{ exclusionsOnly ? `${preferences.exclusions.length} 项保留` : `${(developer ? preferences.developer_roots : preferences.scan_roots).length ? (developer ? preferences.developer_roots : preferences.scan_roots).length + ' 个目录' : developer ? '工具缓存' : '常用目录'} · ${preferences.exclusions.length} 项保留` }}<template v-if="minMb !== undefined"> · {{ minMb }} MB 起</template></span></summary>
    <p>{{ exclusionsOnly ? '名单内的文件和目录会跳过扫描与清理。' : developer ? '未添加项目目录时只扫描开发工具缓存。' : '未添加目录时扫描常用用户目录；隐藏目录和工程缓存会跳过。' }}</p>
    <template v-if="!exclusionsOnly">
      <div class="scope-row"><span>扫描目录</span><el-button text :disabled="disabled" @click="add()"><FolderPlus :size="14" />添加目录</el-button></div>
      <div v-for="path in developer ? preferences.developer_roots : preferences.scan_roots" :key="path" class="scope-path"><code :title="path">{{ path }}</code><el-button text :disabled="disabled" :aria-label="`移除 ${path}`" @click="remove(path)"><X :size="13" /></el-button></div>
    </template>
    <div class="scope-row"><span>保留名单</span><el-button text :disabled="disabled" @click="add(true)"><FolderPlus :size="14" />添加目录</el-button></div>
    <div v-for="path in preferences.exclusions" :key="path" class="scope-path"><code :title="path">{{ path }}</code><el-button text :disabled="disabled" :aria-label="`取消排除 ${path}`" @click="remove(path, true)"><X :size="13" /></el-button></div>
    <p v-if="embedded && exclusionsOnly && !preferences.exclusions.length">暂无保留项目</p>
    <details class="manual-entry"><summary>手动输入路径</summary><div class="manual-path"><el-select v-if="!exclusionsOnly" v-model="manualTarget" :disabled="disabled" aria-label="路径用途"><el-option label="扫描目录" value="scan"/><el-option label="保留名单" value="keep"/></el-select><el-input v-model="manualPath" placeholder="输入绝对路径" :disabled="disabled" @keyup.enter="addManual(exclusionsOnly || manualTarget === 'keep')"/><el-button :disabled="disabled||!manualPath.trim()" @click="addManual(exclusionsOnly || manualTarget === 'keep')">添加</el-button></div></details>
    <div v-if="minMb !== undefined" class="scope-row"><span>最小文件大小（MB）</span><el-input-number :model-value="minMb" :min="1" :max="1000000" :disabled="disabled" @update:model-value="emit('update:minMb', Number($event || 1))" /></div>
  </component>
</template>
<style scoped>
.scan-scope { margin-bottom: 18px; border: 1px solid var(--line); background: var(--surface-subtle); border-radius: 14px; padding: 14px 16px; color: var(--text-secondary); font-size: 12px; }
.scan-scope.embedded { margin: 0; padding: 0; border: 0; background: none; }
summary { cursor: pointer; } summary span { color: var(--text-faint); font-size: 11px; margin-left: 12px; } p { color: var(--text-faint); font-size: 11px; line-height: 1.7; }
.scope-row,.scope-path { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 34px; }
.scope-path code { min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 11px; color: var(--text-secondary); } .scope-path { margin-left: 12px; }
.manual-path{display:flex;gap:8px;margin:12px 0;flex-wrap:wrap}.manual-path .el-input{flex:1;min-width:160px}.manual-path .el-select{width:125px}.manual-entry{margin:12px 0;font-size:11px}
</style>
