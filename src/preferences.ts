import { reactive } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
export type TrayMetric = 'cpu' | 'memory' | 'download' | 'upload' | 'disk';
export interface TrayPreferences { enabled:boolean; metrics:TrayMetric[]; show_icon:boolean; labeled:boolean; interval_ms:number; network_interface:string; rate_unit:'bytes'|'bits'; close_to_tray:boolean; show_sparklines:boolean }
export const defaultTrayPreferences = (): TrayPreferences => ({enabled:true,metrics:['download','upload'],show_icon:true,labeled:false,interval_ms:2000,network_interface:'auto',rate_unit:'bytes',close_to_tray:true,show_sparklines:true});
export interface Preferences {scan_roots:string[];exclusions:string[];developer_roots:string[];developer_days:number;large_min_mb:number;monitor_enabled:boolean;tray:TrayPreferences}
export const preferences=reactive<Preferences>({scan_roots:[],exclusions:[],developer_roots:[],developer_days:7,large_min_mb:50,monitor_enabled:true,tray:defaultTrayPreferences()});
export function subscribePreferences() {
  return isTauri() ? listen<Preferences>('preferences-changed', event => Object.assign(preferences, event.payload)) : Promise.resolve(() => {});
}
let pending: Promise<void> | undefined;
let saving: Promise<void> = Promise.resolve();
const pathKeys = ['scan_roots', 'exclusions', 'developer_roots'] as const;
type PreferencesPatch = Partial<Preferences> | ((current: Readonly<Preferences>) => Partial<Preferences>);
function snapshot(): Preferences {
  return { ...preferences, scan_roots: [...preferences.scan_roots], exclusions: [...preferences.exclusions], developer_roots: [...preferences.developer_roots], tray: { ...preferences.tray, metrics: [...preferences.tray.metrics] } };
}
export function loadPreferences() {
  if (!pending) pending = (async () => {
    if (isTauri()) Object.assign(preferences, await invoke<Preferences>('get_preferences'));
  })().catch(error => { pending = undefined; throw error; });
  return pending;
}
export function savePreferences(patch: PreferencesPatch): Promise<void> {
  const baseline = snapshot();
  // Existing object callers keep working; updater callers compute arrays after prior saves finish.
  const requested = typeof patch === 'function' ? patch : {
    ...patch,
    ...(patch.scan_roots ? { scan_roots: [...patch.scan_roots] } : {}),
    ...(patch.exclusions ? { exclusions: [...patch.exclusions] } : {}),
    ...(patch.developer_roots ? { developer_roots: [...patch.developer_roots] } : {}),
  };
  const operation = saving.then(async () => {
    await loadPreferences();
    const current = snapshot();
    const changes = typeof requested === 'function' ? requested(current) : requested;
    const value: Preferences = { ...current, ...changes };
    if (typeof requested !== 'function') {
      // Replay legacy path-list edits against the latest state so rapid "保留" clicks cannot lose one another.
      for (const key of pathKeys) {
        const paths = changes[key];
        if (!paths) continue;
        const removed = new Set(baseline[key].filter(path => !paths.includes(path)));
        const added = paths.filter(path => !baseline[key].includes(path));
        value[key] = [...new Set([...current[key].filter(path => !removed.has(path)), ...added])];
      }
    }
    const saved = isTauri() ? await invoke<Preferences>('save_preferences', { preferences: value }) : value;
    Object.assign(preferences, saved);
  });
  // A failed save rejects its own caller but does not block subsequent edits or an explicit retry.
  saving = operation.catch(() => {});
  return operation;
}
