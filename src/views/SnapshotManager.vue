<template>
  <div class="snapshot-manager">
    <div class="toolbar">
      <el-button type="primary" :icon="Refresh" @click="store.refreshSnapshots()">
        刷新列表
      </el-button>
      <el-button
        type="success"
        :icon="View"
        :disabled="!selectedId"
        @click="openMemoryViewer"
      >
        查看内存布局
      </el-button>
      <el-button
        type="warning"
        :icon="Search"
        :disabled="!selectedId"
        @click="openScanner"
      >
        模式扫描
      </el-button>
      <el-button
        type="info"
        :icon="CopyDocument"
        @click="goCompare"
      >
        快照对比
      </el-button>
      <el-button
        type="success"
        plain
        :icon="VideoPlay"
        @click="goMonitor"
      >
        内存监控
      </el-button>
      <el-button
        type="danger"
        :icon="Delete"
        :disabled="!selectedId"
        @click="handleDelete"
      >
        删除快照
      </el-button>
    </div>

    <el-table
      :data="store.snapshots"
      height="600"
      highlight-current-row
      stripe
      @current-change="handleRowChange"
      style="width: 100%;"
    >
      <el-table-column type="index" label="#" width="60" />
      <el-table-column prop="id" label="ID" width="80">
        <template #default="{ row }">
          <el-tag type="primary" effect="dark" size="small">#{{ row.id }}</el-tag>
        </template>
      </el-table-column>
      <el-table-column prop="process_name" label="进程名" min-width="160" />
      <el-table-column prop="pid" label="PID" width="100" />
      <el-table-column prop="total_size_mb" label="大小 (MB)" width="120" sortable>
        <template #default="{ row }">
          <span class="size-text">{{ (row.total_size_mb || 0).toFixed(2) }}</span>
        </template>
      </el-table-column>
      <el-table-column prop="region_count" label="内存区域数" width="120" />
      <el-table-column prop="created_at" label="创建时间" min-width="200" sortable>
        <template #default="{ row }">
          <span class="time-text">{{ formatTime(row.created_at) }}</span>
        </template>
      </el-table-column>
      <el-table-column label="操作" width="180" fixed="right">
        <template #default="{ row }">
          <el-button size="small" type="primary" link @click="viewMemory(row.id)">
            内存布局
          </el-button>
          <el-button size="small" type="warning" link @click="viewScan(row.id)">
            模式扫描
          </el-button>
          <el-button size="small" type="danger" link @click="deleteRow(row.id)">
            删除
          </el-button>
        </template>
      </el-table-column>
    </el-table>

    <el-empty
      v-if="store.snapshots.length === 0"
      description="暂无快照，请在进程列表中创建"
      :image-size="120"
    />
  </div>
</template>

<script setup>
import { ref, onMounted, markRaw } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Refresh, View, Search, Delete, CopyDocument, VideoPlay } from '@element-plus/icons-vue'
import { useProcessStore } from '../stores/process'

const router = useRouter()
const store = useProcessStore()
const selectedId = ref(null)

const formatTime = (t) => {
  if (!t) return '-'
  const d = new Date(t)
  return d.toLocaleString('zh-CN')
}

const handleRowChange = (row) => {
  selectedId.value = row ? row.id : null
}

const viewMemory = (id) => router.push({ name: 'memory', params: { snapshotId: id } })
const viewScan = (id) => router.push({ name: 'scan', params: { snapshotId: id } })
const goCompare = () => router.push({ name: 'compare' })
const goMonitor = () => router.push({ name: 'monitor' })

const openMemoryViewer = () => selectedId.value && viewMemory(selectedId.value)
const openScanner = () => selectedId.value && viewScan(selectedId.value)

const deleteRow = async (id) => {
  try {
    await ElMessageBox.confirm('确定删除该快照？', '确认删除', { type: 'warning' })
    await store.deleteSnapshot(id)
    ElMessage.success('删除成功')
  } catch {}
}

const handleDelete = () => selectedId.value && deleteRow(selectedId.value)

onMounted(() => store.refreshSnapshots())
</script>

<style lang="scss" scoped>
.snapshot-manager {
  .toolbar {
    display: flex;
    gap: 12px;
    margin-bottom: 16px;
  }

  .size-text {
    font-family: Consolas, monospace;
    color: #fbbf24;
  }

  .time-text {
    color: #8b9bb4;
    font-size: 13px;
  }

  :deep(.el-table) {
    background: #16213e;
    --el-table-border-color: #0f3460;
    --el-table-header-bg-color: #0f3460;
    --el-table-tr-bg-color: transparent;
    --el-table-row-hover-bg-color: #1a2a4a;
    color: #e0e0e0;
  }
  :deep(.el-table th.el-table__cell) { color: #8b9bb4; }
  :deep(.el-table .el-table__row.current-row) { background: #1a2a4a !important; }
}
</style>
