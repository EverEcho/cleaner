import { ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';

export interface PlatformInfo {
  os: string;
  desktop_visibility: boolean;
  autostart_note: string;
  updates_note: string;
  authorization_cache_available: boolean;
}
export const platformInfo = ref<PlatformInfo | null>(null);
let pending: Promise<void> | undefined;
export function loadPlatform() {
  if (!pending && isTauri()) {
    pending = invoke<PlatformInfo>('get_platform_info').then(info => { platformInfo.value = info; });
  }
  return pending ?? Promise.resolve();
}
