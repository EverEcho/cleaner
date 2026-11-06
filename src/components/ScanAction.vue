<script setup lang="ts">
import { ref } from 'vue';
import { RefreshCw } from 'lucide-vue-next';
import { ElMessage } from 'element-plus';
import { cancelTask, type TaskSnapshot } from '../tasks';

const props = withDefaults(defineProps<{
  scanning: boolean;
  task?: TaskSnapshot;
  disabled?: boolean;
  hint?: string;
  label?: string;
  stopLabel?: string;
  initial?: boolean;
}>(), { label: '重新扫描', stopLabel: '停止扫描' });
defineEmits<{ start: [] }>();
const stopping = ref(false);
async function stop() {
  if (!props.task || stopping.value) return;
  stopping.value = true;
  try { await cancelTask(props.task.id); }
  catch (error) { ElMessage.error('停止失败：' + String(error)); }
  finally { stopping.value = false; }
}
</script>

<template>
  <div class="scan-action" :class="{ 'scan-action-initial': initial }">
    <span v-if="scanning && hint" class="scan-action-hint" :title="hint" role="status">{{ hint }}</span>
    <el-button v-if="scanning" :disabled="!task || stopping || task.state === 'cancelling'" @click="stop">{{ stopping || task?.state === 'cancelling' ? '正在停止…' : stopLabel }}</el-button>
    <el-button v-else :type="initial ? 'primary' : 'default'" :disabled="disabled" @click="$emit('start')"><RefreshCw :size="14" />{{ label }}</el-button>
  </div>
</template>

<style scoped>
.scan-action { display: flex; align-items: center; justify-content: flex-end; gap: 12px; min-width: 0; }
.scan-action-initial { justify-content: center; max-width: 100%; margin-top: 20px; }
.scan-action-hint { color: var(--text-muted); font-size: 11px; line-height: 1.6; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.scan-action > .el-button { flex-shrink: 0; margin: 0; }
.scan-action svg { margin-right: 6px; }
</style>
