import { ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface TaskSnapshot { id: string; kind: string; state: 'running'|'cancelling'|'completed'|'cancelled'|'failed'; phase: string; current: number; total: number|null; path: string; started_at: number; updated_at: number; cancellable: boolean }
export const taskList = ref<TaskSnapshot[]>([]);
export const taskResults = ref<Record<string,{at:number;summary:string}>>({});
export const taskLabels: Record<string,string> = { scan_junk:'垃圾扫描', scan_developer_junk:'项目扫描', scan_large_files:'大文件扫描', scan_duplicate_files:'重复文件比对', analyze_disk:'目录分析', get_installed_apps_report:'应用扫描', clean_items:'清理文件',trash_files:'移入废纸篓',uninstall_app_plan:'卸载应用',uninstall_manual_app:'卸载独立应用',upgrade_software:'软件升级',upgrade_tool:'工具升级',check_software_updates:'检查更新',refresh_software_sources:'刷新软件源' };
let unlisten: UnlistenFn|undefined;
let initializing: Promise<void>|undefined;
function terminal(state: TaskSnapshot['state']) { return ['completed', 'failed', 'cancelled'].includes(state); }
function update(task: TaskSnapshot, authoritative = false) {
  const previous = taskList.value.find(row => row.id === task.id);
  if (previous) {
    // Delayed progress/optimistic cancellation must never revive a finished task.
    if (terminal(previous.state) && !terminal(task.state)) return;
    // The backend finishes each ID once; its terminal event wins over any local synthesis.
    if (previous.updated_at > task.updated_at && !(authoritative && terminal(task.state))) return;
  }
  taskList.value = [task, ...taskList.value.filter(row => row.id !== task.id)]
    .sort((a, b) => b.started_at - a.started_at).slice(0, 60);
}
export function activeTask(command: string) { return taskList.value.find(row=>row.kind===command && ['running','cancelling'].includes(row.state)); }
export function initializeTasks() {
  if (!isTauri()) return Promise.resolve();
  if (!initializing) initializing = (async()=>{ unlisten=await listen<TaskSnapshot>('task-progress',e=>update(e.payload,true)); const tasks=await invoke<TaskSnapshot[]>('get_tasks'); for(const task of tasks)update(task,true); })().catch(error=>{ initializing=undefined;throw error; });
  return initializing;
}
export function disposeTasks() { unlisten?.();unlisten=undefined;initializing=undefined; }
export async function runTask<T>(command:string,args:Record<string,unknown>={}):Promise<T> {
  const id=`ui-${Date.now()}-${Math.random().toString(36).slice(2,10)}`;
  const stamp=Date.now();
  update({id,kind:command,state:'running',phase:'准备中',current:0,total:null,path:'',started_at:stamp,updated_at:stamp,cancellable:/^(scan_|analyze_disk|check_software_updates|get_installed_apps_report)/.test(command)});
  try {
    const result=await invoke<T>(command,{...args,taskId:id});
    let summary='检测已完成';
    if(Array.isArray(result))summary=`已返回 ${result.length} 个分类`;
    else if(result&&typeof result==='object'){
      const data=result as Record<string,unknown>;
      if(Array.isArray(data.apps))summary=`已识别 ${data.apps.length} 项应用`;
      else if(Array.isArray(data.packages)&&Array.isArray(data.tools))summary=`${data.packages.length+data.tools.filter((tool:unknown)=>tool&&typeof tool==='object'&&(tool as Record<string,unknown>).status==='available').length} 项可用更新`;
      else if(typeof data.succeeded==='number')summary=`成功 ${data.succeeded} 项，失败 ${data.failed??0} 项`;
      else if(Array.isArray(data.items))summary=`已发现 ${data.items.length} 项${data.cancelled?' · 已停止':''}`;
      else if(typeof data.count==='number')summary=`已统计 ${data.count} 个目标`;
      if(Array.isArray(data.warnings)&&data.warnings.length)summary+=' · 有未完成提示';
      if(data.warning)summary+=' · 部分结果';
    }
    taskResults.value={...taskResults.value,[command]:{at:Date.now(),summary}};
    const task=taskList.value.find(row=>row.id===id);
    if(task && ['running','cancelling'].includes(task.state)) update({...task,state:task.state==='cancelling'?'cancelled':'completed',phase:task.state==='cancelling'?'已停止，保留已完成结果':'已完成',updated_at:Date.now()});
    return result;
  } catch(error) {
    const task=taskList.value.find(row=>row.id===id);
    if(task && !terminal(task.state))update({...task,state:task.state==='cancelling'?'cancelled':'failed',phase:String(error),updated_at:Date.now()});
    throw error;
  }
}
export async function cancelTask(id: string) {
  const before = taskList.value.find(row => row.id === id);
  if (before && terminal(before.state)) return;
  await invoke('cancel_task', { id });
  const task = taskList.value.find(row => row.id === id);
  if (task?.state === 'running') update({ ...task, state: 'cancelling', phase: '正在停止…', updated_at: Date.now() });
}
