import { ref, watch } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { preferences } from './preferences';
export interface MonitorSnapshot {sampled_at:number;cpu_percent:number|null;memory_total:number|null;memory_used:number|null;disk_total:number|null;disk_available:number|null;disk_path:string;warnings:string[]}
export const monitorSnapshot=ref<MonitorSnapshot|null>(null);
export const monitorHistory=ref<MonitorSnapshot[]>([]);
export const monitorError=ref('');
let timer:ReturnType<typeof setInterval>|undefined;let busy=false;let stopWatch:(()=>void)|undefined;
export async function sampleMonitor(){if(!isTauri()||busy||!preferences.monitor_enabled)return;busy=true;try{const result=await invoke<MonitorSnapshot>('get_monitor_snapshot');monitorSnapshot.value=result;monitorError.value='';if(monitorHistory.value[monitorHistory.value.length-1]?.sampled_at!==result.sampled_at)monitorHistory.value=[...monitorHistory.value,result].slice(-60);}catch(error){monitorError.value=String(error);}finally{busy=false;}}
export function startMonitor(){if(stopWatch)return;stopWatch=watch(()=>preferences.monitor_enabled,enabled=>{if(timer)clearInterval(timer);timer=undefined;if(enabled&&isTauri()){void sampleMonitor();timer=setInterval(()=>{if(!document.hidden)void sampleMonitor();},2500);}}, {immediate:true});}
export function stopMonitor(){stopWatch?.();stopWatch=undefined;if(timer)clearInterval(timer);timer=undefined;}
export function formatBytes(value:number|null|undefined){if(value==null)return '—';if(value<1024)return `${value} B`;const power=Math.min(4,Math.floor(Math.log(value)/Math.log(1024)));return `${(value/1024**power).toFixed(1)} ${['B','KB','MB','GB','TB'][power]}`;}
