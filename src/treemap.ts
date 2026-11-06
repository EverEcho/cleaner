import { hierarchy, treemap } from 'd3-hierarchy';

export interface StorageItem { path: string; size_bytes: number; size: string; is_dir: boolean }
interface MapNode { item?: StorageItem; count?: number; bytes?: number; children?: MapNode[] }
export function layoutStorage(items: StorageItem[], width: number, height: number) {
  const positive = items.filter(item => item.size_bytes > 0).sort((a, b) => b.size_bytes - a.size_bytes);
  const children: MapNode[] = positive.slice(0, 120).map(item => ({ item, bytes: item.size_bytes }));
  if (positive.length > 120) children.push({ count: positive.length - 120, bytes: positive.slice(120).reduce((sum, item) => sum + item.size_bytes, 0) });
  const root = hierarchy<MapNode>({ children }).sum(node => node.bytes || 0).sort((a, b) => (b.value || 0) - (a.value || 0));
  return treemap<MapNode>().size([Math.max(1, width), Math.max(1, height)]).paddingInner(4).round(true)(root).leaves().filter(node => node.value && node.x1 > node.x0 && node.y1 > node.y0);
}
