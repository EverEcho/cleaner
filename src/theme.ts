import { ref } from 'vue';

export type ThemePreference = 'dark' | 'light' | 'system';
const storageKey = 'appclear.theme';
export const themePreference = ref<ThemePreference>('dark');
let systemTheme: MediaQueryList;

function validTheme(value: unknown): value is ThemePreference {
  return value === 'dark' || value === 'light' || value === 'system';
}

function applyTheme() {
  document.documentElement.dataset.theme = themePreference.value === 'system'
    ? systemTheme.matches ? 'dark' : 'light'
    : themePreference.value;
}

export function initializeTheme() {
  systemTheme = window.matchMedia('(prefers-color-scheme: dark)');
  try {
    const saved = localStorage.getItem(storageKey);
    if (validTheme(saved)) themePreference.value = saved;
  } catch { /* The current session still supports switching if storage is unavailable. */ }
  applyTheme();
  systemTheme.addEventListener('change', applyTheme);
  window.addEventListener('storage', event => {
    if (event.key !== storageKey && event.key !== null) return;
    themePreference.value = validTheme(event.newValue) ? event.newValue : 'dark';
    applyTheme();
  });
}

export function setTheme(preference: ThemePreference): boolean {
  themePreference.value = preference;
  applyTheme();
  try {
    localStorage.setItem(storageKey, preference);
    return true;
  } catch {
    return false;
  }
}
