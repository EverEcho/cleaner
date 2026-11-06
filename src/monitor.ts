import { ref, watch } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { preferences } from './preferences';
export interface MonitorSnapshot {sampled_at:number;cpu_percent:number|null;memory_total:number|null;memory_used:number|null;disk_total:number|null;disk_available:number|null;disk_path:string;network_down:number|null;network_up:number|null;network_interface:string;network_interfaces:string[];warnings:string[]}
export const monitorSnapshot=ref<MonitorSnapshot|null>(null);
export const monitorHistory=ref<MonitorSnapshot[]>([]);
export const monitorError=ref('');
let timer:ReturnType<typeof setInterval>|undefined, busy=false;
let stopWatch:(()=>void)|undefined, unlisten:(()=>void)|undefined, generation=0;
function accept(result:MonitorSnapshot) {
  if (!preferences.monitor_enabled || result.sampled_at < (monitorSnapshot.value?.sampled_at || 0)) return;
  const previous = monitorSnapshot.value;
  if (previous && previous.network_interface !== result.network_interface) monitorHistory.value = [];
  monitorSnapshot.value=result; monitorError.value='';
  if(monitorHistory.value[monitorHistory.value.length-1]?.sampled_at!==result.sampled_at)monitorHistory.value=[...monitorHistory.value,result].slice(-60);
}
export async function sampleMonitor(){if(!isTauri()||busy||!preferences.monitor_enabled)return;busy=true;try{accept(await invoke<MonitorSnapshot>('get_monitor_snapshot'));}catch(error){monitorError.value=String(error);}finally{busy=false;}}
export function startMonitor(){
  if(stopWatch)return;
  const token=++generation;
  if(isTauri())void listen<MonitorSnapshot>('monitor-snapshot',event=>accept(event.payload)).then(stop=>{if(token!==generation)stop();else unlisten=stop;}).catch(error=>{monitorError.value=String(error);});
  stopWatch=watch(()=>[preferences.monitor_enabled,preferences.tray.interval_ms,preferences.tray.network_interface] as const,([enabled,interval])=>{
    if(timer)clearInterval(timer);timer=undefined;
    if(enabled&&isTauri()){void sampleMonitor();timer=setInterval(()=>{if(!document.hidden)void sampleMonitor();},interval);}
  },{immediate:true});
}
export function stopMonitor(){++generation;stopWatch?.();stopWatch=undefined;unlisten?.();unlisten=undefined;if(timer)clearInterval(timer);timer=undefined;}
export function formatBytes(value:number|null|undefined){if(value==null)return '—';if(value<1024)return `${value} B`;const power=Math.min(4,Math.floor(Math.log(value)/Math.log(1024)));return `${(value/1024**power).toFixed(1)} ${['B','KB','MB','GB','TB'][power]}`;}
export function formatRate(value:number|null|undefined,bits=false){if(value==null)return '—';const amount=value*(bits?8:1),base=bits?1000:1024;const power=amount>0?Math.min(3,Math.floor(Math.log(amount)/Math.log(base))):0;return `${(amount/base**power).toFixed(power&&amount/base**power<10?1:0)} ${bits?['b','Kb','Mb','Gb'][power]:['B','KB','MB','GB'][power]}/s`;}
