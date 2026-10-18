<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { ref } from 'vue';
import { ElMessage } from 'element-plus';
import { FolderPlus, X } from 'lucide-vue-next';
import { preferences, savePreferences } from '../preferences';
const props = defineProps<{ disabled?: boolean; developer?: boolean; minMb?: number }>();
const emit = defineEmits<{ 'update:minMb': [value: number] }>();
const manualPath=ref('');
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
  <details class="scan-scope">
    <summary>扫描范围与保留名单 <span>{{ (developer ? preferences.developer_roots : preferences.scan_roots).length || '常用' }} 个目录 · {{ preferences.exclusions.length }} 项排除</span></summary>
    <p>{{ developer ? '未添加项目目录时只扫描开发工具缓存。' : '未添加目录时扫描常用用户目录；隐藏目录和工程缓存会跳过。' }}保留名单长期生效。</p>
    <div class="manual-path"><el-input v-model="manualPath" placeholder="也可输入绝对路径，如 /home/me/Projects" :disabled="disabled" @keyup.enter="addManual()"/><el-button :disabled="disabled||!manualPath" @click="addManual()">添加范围</el-button><el-button :disabled="disabled||!manualPath" @click="addManual(true)">加入保留</el-button></div>
    <div class="scope-row"><span>扫描目录</span><el-button text :disabled="disabled" @click="add()"><FolderPlus :size="14" />添加目录</el-button></div>
    <div v-for="path in developer ? preferences.developer_roots : preferences.scan_roots" :key="path" class="scope-path"><code>{{ path }}</code><el-button text :disabled="disabled" :aria-label="`移除 ${path}`" @click="remove(path)"><X :size="13" /></el-button></div>
    <div class="scope-row"><span>保留名单</span><el-button text :disabled="disabled" @click="add(true)"><FolderPlus :size="14" />排除目录</el-button></div>
    <div v-for="path in preferences.exclusions" :key="path" class="scope-path"><code>{{ path }}</code><el-button text :disabled="disabled" :aria-label="`取消排除 ${path}`" @click="remove(path, true)"><X :size="13" /></el-button></div>
    <div v-if="minMb !== undefined" class="scope-row"><span>最小文件大小（MB）</span><el-input-number :model-value="minMb" :min="1" :max="1000000" :disabled="disabled" @update:model-value="emit('update:minMb', Number($event || 1))" /></div>
  </details>
</template>
<style scoped>
.scan-scope { margin-bottom: 18px; border: 1px solid #ffffff14; background: #ffffff04; border-radius: 14px; padding: 14px 16px; color: #c7bedb; font-size: 12px; }
summary { cursor: pointer; } summary span { color: #8f85a4; font-size: 11px; margin-left: 12px; } p { color: #8f85a4; font-size: 11px; line-height: 1.7; }
.scope-row,.scope-path { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 34px; }
.scope-path code { font-size: 11px; overflow-wrap: anywhere; color: #a9bfd0; } .scope-path { margin-left: 12px; }
.manual-path{display:flex;gap:8px;margin:12px 0;flex-wrap:wrap}.manual-path .el-input{flex:1;min-width:220px}
</style>
