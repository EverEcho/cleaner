<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ElMessage, ElMessageBox } from 'element-plus';
import PageHeader from '../layout/PageHeader.vue';
import ScanningRadar from '../layout/ScanningRadar.vue';

interface CleanItem {
  id: string;
  path: string;
  name: string;
  size_bytes: number;
  size_str: string;
  reason: string;
  can_delete: boolean;
  selected: boolean;
}

interface CleanCat {
  id: string;
  title: string;
  desc: string;
  icon: string;
  total_size_bytes: number;
  total_size_str: string;
  count: number;
  items: CleanItem[];
}

const scanning = ref(true);
const junkCategories = ref<CleanCat[]>([]);
const selectedJunkPaths = ref<string[]>([]);
const activeCollapseNames = ref<string[]>([]);

// 选中的垃圾总大小
const totalSelectedBytes = computed(() => {
  let sum = 0;
  const set = new Set(selectedJunkPaths.value);
  for (const cat of junkCategories.value) {
    for (const item of cat.items) {
      if (set.has(item.path)) {
        sum += item.size_bytes;
      }
    }
  }
  return sum;
});

const totalSelectedFormatted = computed(() => {
  const bytes = totalSelectedBytes.value;
  const GB = 1024 * 1024 * 1024;
  const MB = 1024 * 1024;
  if (bytes >= GB) return (bytes / GB).toFixed(2) + ' GB';
  if (bytes >= MB) return (bytes / MB).toFixed(1) + ' MB';
  return (bytes / 1024).toFixed(0) + ' KB';
});

// 判断某个分组是否处于全选、半选状态
function isGroupChecked(cat: CleanCat): boolean {
  if (!cat.items || cat.items.length === 0) return false;
  const set = new Set(selectedJunkPaths.value);
  return cat.items.every((i) => set.has(i.path));
}

function isGroupIndeterminate(cat: CleanCat): boolean {
  if (!cat.items || cat.items.length === 0) return false;
  const set = new Set(selectedJunkPaths.value);
  const selectedCount = cat.items.filter((i) => set.has(i.path)).length;
  return selectedCount > 0 && selectedCount < cat.items.length;
}

// 切换某个分组的全选/全不选 (阻止折叠面板点击冒泡)
function toggleGroupSelection(cat: CleanCat, event?: Event) {
  if (event) {
    event.stopPropagation();
  }
  const allSelected = isGroupChecked(cat);
  const currentSet = new Set(selectedJunkPaths.value);

  if (allSelected) {
    // 全部取消勾选
    for (const item of cat.items) {
      currentSet.delete(item.path);
    }
  } else {
    // 全部勾选
    for (const item of cat.items) {
      currentSet.add(item.path);
    }
  }
  selectedJunkPaths.value = Array.from(currentSet);
}

// 全局全选状态
const isAllSelected = computed(() => {
  const allPaths = junkCategories.value.flatMap((c) => c.items.map((i) => i.path));
  if (allPaths.length === 0) return false;
  return allPaths.every((p) => selectedJunkPaths.value.includes(p));
});

function toggleSelectAll() {
  if (isAllSelected.value) {
    selectedJunkPaths.value = [];
  } else {
    selectedJunkPaths.value = junkCategories.value.flatMap((c) => c.items.map((i) => i.path));
  }
}

async function runScan() {
  scanning.value = true;
  try {
    const res: CleanCat[] = await invoke('scan_junk');
    junkCategories.value = res;
    // 默认全选所有垃圾项
    selectedJunkPaths.value = res.flatMap((cat) => cat.items.map((i) => i.path));
    // 默认展开所有项
    activeCollapseNames.value = res.map((c) => c.id);
    ElMessage.success('系统垃圾扫描完成！');
  } catch (err: any) {
    ElMessage.error('扫描出错: ' + err);
  } finally {
    scanning.value = false;
  }
}

async function executeClean() {
  if (selectedJunkPaths.value.length === 0) {
    ElMessage.warning('请先勾选需要清理的项目');
    return;
  }
  try {
    await ElMessageBox.confirm('确定要清理所选的项目吗？该操作不可撤销。', '确认清理', {
      confirmButtonText: '立即清理',
      cancelButtonText: '取消',
      type: 'warning',
    });

    scanning.value = true;
    const deletedCount: number = await invoke('clean_items', { paths: selectedJunkPaths.value });
    ElMessage.success(`清理完成，已释放 ${deletedCount} 个残留文件/目录`);
    await runScan();
  } catch (err: any) {
    if (err !== 'cancel') {
      ElMessage.error('清理失败: ' + err);
    }
  } finally {
    scanning.value = false;
  }
}

onMounted(() => {
  runScan();
});
</script>

<template>
  <div class="subpage-container">
    <PageHeader title="系统垃圾清理">
      <template #actions>
        <span class="junk-total-stat">
          已勾选垃圾：<b>{{ totalSelectedFormatted }}</b>
        </span>
        <el-button @click="toggleSelectAll">
          {{ isAllSelected ? '取消全选' : '全部选择' }}
        </el-button>
        <el-button @click="runScan" :loading="scanning">重新扫描</el-button>
        <el-button
          type="danger"
          :disabled="selectedJunkPaths.length === 0"
          :loading="scanning"
          @click="executeClean"
        >
          立即清理选中项
        </el-button>
      </template>
    </PageHeader>

    <div class="subpage-content">
      <ScanningRadar
        v-if="scanning"
        title="正在全面扫描系统垃圾…"
        sub-title="深度扫描应用缓存、日志、死链与残留文件"
      />
      <div v-else class="clean-collapse-wrapper">
        <el-collapse v-model="activeCollapseNames">
          <el-collapse-item v-for="cat in junkCategories" :key="cat.id" :name="cat.id">
            <template #title>
              <div class="cat-title-row">
                <!-- 分组 CheckBox：支持全选、半选联动，且点击时不收起折叠面板 -->
                <div class="cat-checkbox-wrapper" @click.stop="toggleGroupSelection(cat, $event)">
                  <el-checkbox
                    :model-value="isGroupChecked(cat)"
                    :indeterminate="isGroupIndeterminate(cat)"
                    @click.stop
                    @change="toggleGroupSelection(cat)"
                  />
                </div>
                <span class="cat-title">{{ cat.title }}</span>
                <span class="cat-desc">{{ cat.desc }} ({{ cat.count }} 项)</span>
                <span class="cat-size-badge">{{ cat.total_size_str }}</span>
              </div>
            </template>

            <!-- 分组下的具体明细列表 -->
            <div class="cat-items-list">
              <div v-for="item in cat.items" :key="item.path" class="junk-item-row">
                <el-checkbox
                  :value="item.path"
                  v-model="selectedJunkPaths"
                  :label="item.name"
                />
                <span class="item-reason">{{ item.reason }}</span>
                <span class="item-size">{{ item.size_str }}</span>
              </div>
            </div>
          </el-collapse-item>
        </el-collapse>
      </div>
    </div>
  </div>
</template>

<style scoped>
.subpage-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #ffffff;
}

.subpage-content {
  flex: 1;
  padding: 16px 24px;
  overflow-y: auto;
}

.junk-total-stat {
  font-size: 14px;
  color: #6b7280;
  margin-right: 8px;
}

.junk-total-stat b {
  color: #e11d48;
}

.clean-collapse-wrapper {
  background: #ffffff;
}

/* 分组标题行样式 */
.cat-title-row {
  display: flex;
  align-items: center;
  width: 100%;
  padding-right: 12px;
}

.cat-checkbox-wrapper {
  display: flex;
  align-items: center;
  margin-right: 12px;
}

.cat-title {
  font-weight: 600;
  font-size: 15px;
  color: #1f2937;
  margin-right: 12px;
}

.cat-desc {
  font-size: 13px;
  color: #9ca3af;
  flex: 1;
}

/* 仿 Lemon 柔和的黄色高亮胶囊标签 */
.cat-size-badge {
  background-color: #fef3c7;
  color: #d97706;
  font-weight: 600;
  font-size: 13px;
  padding: 2px 10px;
  border-radius: 999px;
  margin-right: 10px;
}

/* 子项列表样式 */
.cat-items-list {
  padding: 8px 16px 12px 38px;
  background: #fafafa;
  border-radius: 6px;
  margin-bottom: 6px;
}

.junk-item-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 7px 0;
  border-bottom: 1px solid #f1f2f4;
}

.junk-item-row:last-child {
  border-bottom: none;
}

.item-reason {
  font-size: 12px;
  color: #9ca3af;
  margin: 0 16px;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.item-size {
  font-size: 13px;
  font-weight: 500;
  color: #4b5563;
}
</style>
