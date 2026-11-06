<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { File, Folder } from 'lucide-vue-next';
import { layoutStorage, type StorageItem } from '../treemap';
const props = defineProps<{ items: StorageItem[]; highlighted?: string }>();
const emit = defineEmits<{ open: [item: StorageItem]; highlight: [path: string]; showList: [] }>();
const container = ref<HTMLElement>();
const width = ref(600), height = ref(400);
let observer: ResizeObserver | undefined;
onMounted(() => {
  observer = new ResizeObserver(([entry]) => {
    if (entry && entry.contentRect.width > 0 && entry.contentRect.height > 0) {
      width.value = entry.contentRect.width; height.value = entry.contentRect.height;
    }
  });
  if (container.value) observer.observe(container.value);
});
onUnmounted(() => observer?.disconnect());
const tiles = computed(() => layoutStorage(props.items, width.value, height.value));
const total = computed(() => props.items.reduce((sum, item) => sum + item.size_bytes, 0));
const tones = ['var(--accent)', 'var(--cyan)', 'var(--blue)', 'var(--pink)', 'var(--text-muted)'];
function name(item?: StorageItem) { return item?.path.split('/').filter(Boolean).pop() || '/'; }
function title(tile: typeof tiles.value[number]) {
  return tile.data.item ? `${tile.data.item.path}\n${tile.data.item.size} · ${(tile.value! / total.value * 100).toFixed(1)}%\n${tile.data.item.is_dir ? '点击打开目录' : '点击查看位置'}` : `其余 ${tile.data.count} 项 · 点击切换列表查看`;
}
</script>
<template>
  <div ref="container" class="treemap" aria-label="按面积显示目录与文件占用">
    <button v-for="(tile, index) in tiles" :key="tile.data.item?.path || 'other'" class="tile" :class="{ highlighted: tile.data.item?.path === highlighted, compact: tile.x1 - tile.x0 < 75 || tile.y1 - tile.y0 < 65 }" :style="{ left: `${tile.x0}px`, top: `${tile.y0}px`, width: `${tile.x1 - tile.x0}px`, height: `${tile.y1 - tile.y0}px`, '--tile-tone': tones[index % tones.length] }" :title="title(tile)" :aria-label="title(tile)" @mouseenter="emit('highlight', tile.data.item?.path || '')" @mouseleave="emit('highlight', '')" @focus="emit('highlight', tile.data.item?.path || '')" @blur="emit('highlight', '')" @click="tile.data.item ? emit('open', tile.data.item) : emit('showList')">
      <template v-if="tile.x1 - tile.x0 > 32 && tile.y1 - tile.y0 > 24">
        <component :is="tile.data.item?.is_dir ? Folder : File" :size="26" />
        <span>{{ tile.data.item ? name(tile.data.item) : `其余 ${tile.data.count} 项` }}</span>
        <small v-if="tile.data.item">{{ tile.data.item.size }}</small>
      </template>
    </button>
    <p v-if="!tiles.length" class="map-empty">没有可显示的文件占用</p>
  </div>
</template>
<style scoped>
.treemap { position: relative; width: 100%; height: var(--storage-map-height, clamp(240px, 53vh, 620px)); min-width: 0; overflow: hidden; }
.tile { position: absolute; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 7px; min-width: 0; overflow: hidden; padding: 9px; border: 1px solid var(--line); border-radius: 8px; background: color-mix(in srgb, var(--tile-tone) 11%, var(--surface)); color: var(--text); font: inherit; cursor: pointer; }
.tile:hover, .tile.highlighted { border-color: var(--tile-tone); background: color-mix(in srgb, var(--tile-tone) 20%, var(--surface)); box-shadow: inset 0 0 0 1px var(--tile-tone); }
.tile svg { color: var(--tile-tone); flex-shrink: 0; }
.tile span { display: block; width: 100%; min-width: 0; text-overflow: ellipsis; white-space: nowrap; overflow: hidden; font-size: 12px; }
.tile small { font-size: 11px; color: var(--text-muted); white-space: nowrap; }
.tile.compact { padding: 3px; gap: 2px; }
.tile.compact svg { display: none; }
.tile.compact span, .tile.compact small { font-size: 10px; }
.map-empty { display: grid; place-items: center; height: 100%; margin: 0; color: var(--text-muted); font-size: 12px; }
</style>
