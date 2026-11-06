import { computed, ref } from 'vue';
import { invoke, isTauri } from '@tauri-apps/api/core';

export interface PlatformInfo {
  os: string;
  desktop_visibility: boolean;
  autostart_note: string;
  updates_note: string;
  authorization_cache_available: boolean;
}
export const platformInfo = ref<PlatformInfo | null>(null);
// Browser previews retain all tools; native clients wait for confirmed support.
export const desktopEntriesAvailable = computed(() => !isTauri() || platformInfo.value?.desktop_visibility === true);
let pending: Promise<void> | undefined;
export function loadPlatform() {
  if (!pending && isTauri()) {
    pending = invoke<PlatformInfo>('get_platform_info')
      .then(info => { platformInfo.value = info; })
      .catch(error => { pending = undefined; throw error; });
  }
  return pending ?? Promise.resolve();
}
