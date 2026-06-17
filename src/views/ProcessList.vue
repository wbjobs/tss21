<template>
  <div class="process-list-container">
    <el-alert
      v-if="privilegeInfo && !privilegeInfo.is_admin"
      type="warning"
      show-icon
      :title="'权限提示：当前未以管理员身份运行'"
      :description="privilegeInfo.suggested_action"
      style="margin-bottom: 16px;"
    />

    <div class="toolbar">
      <div class="filter-section">
        <el-input
          v-model="filterText"
          placeholder="搜索进程名或PID..."
          clearable
          style="width: 320px;"
          :prefix-icon="Search"
        />
      </div>
      <div class="actions">
        <el-button type="primary" :icon="Refresh" @click="loadProcesses" :loading="store.loading">
          刷新进程
        </el-button>
        <el-button
          type="danger"
          :icon="Camera"
          :disabled="!selectedPid"
          @click="handleCreateSnapshot"
          :loading="creatingSnapshot"
        >
          创建内存快照
        </el-button>
      </div>
    </div>

    <el-card class="stats-card">
      <el-row :gutter="20">
        <el-col :span="6">
          <el-statistic title="进程总数" :value="store.processes.length" />
        </el-col>
        <el-col :span="6">
          <el-statistic title="选中进程" :value="selectedProcessName" />
        </el-col>
        <el-col :span="6">
          <el-statistic title="系统总内存 (MB)" :value="systemMemory" />
        </el-col>
        <el-col :span="6">
          <el-statistic title="已创建快照" :value="store.snapshots.length" />
        </el-col>
      </el-row>
    </el-card>

    <el-table
      :data="filteredProcesses"
      height="520"
      highlight-current-row
      stripe
      @current-change="handleRowChange"
      @row-dblclick="handleRowDoubleClick"
      style="width: 100%; margin-top: 16px;"
      v-loading="store.loading"
    >
      <el-table-column type="index" label="#" width="60" />
      <el-table-column prop="pid" label="PID" width="100" sortable>
        <template #default="{ row }">
          <span class="pid-badge">{{ row.pid }}</span>
        </template>
      </el-table-column>
      <el-table-column prop="name" label="进程名" min-width="180" sortable>
        <template #default="{ row }">
          <div class="process-name-cell">
            <el-icon size="16" color="#60a5fa"><Monitor /></el-icon>
            <span>{{ row.name }}</span>
          </div>
        </template>
      </el-table-column>
      <el-table-column prop="memory_usage_mb" label="内存 (MB)" width="120" sortable>
        <template #default="{ row }">
          <el-tag :type="getMemoryTagType(row.memory_usage_mb)" size="small">
            {{ (row.memory_usage_mb || 0).toFixed(1) }}
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column prop="thread_count" label="线程数" width="100" sortable />
      <el-table-column label="权限状态" width="120">
        <template #default="{ row }">
          <el-tag
            v-if="isProtectedProcess(row.name)"
            type="danger"
            size="small"
            effect="dark"
          >
            受保护
          </el-tag>
          <el-tag
            v-else
            type="success"
            size="small"
            effect="light"
          >
            可访问
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column prop="path" label="可执行文件路径" min-width="260" show-overflow-tooltip />
    </el-table>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, markRaw } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Search, Refresh, Camera, Monitor } from '@element-plus/icons-vue'
import { useProcessStore } from '../stores/process'

const router = useRouter()
const store = useProcessStore()

const filterText = ref('')
const selectedPid = ref(null)
const creatingSnapshot = ref(false)
const systemMemory = ref(0)
const privilegeInfo = ref(null)

const PROTECTED_PROCESSES = [
  'csrss.exe', 'smss.exe', 'wininit.exe', 'winlogon.exe',
  'services.exe', 'lsass.exe', 'lsm.exe', 'svchost.exe',
  'system', 'registry', 'memcompress.exe', 'fontdrvhost.exe',
  'dwm.exe', 'igfxcuiservice.exe', 'nvcontainer.exe'
]

const isProtectedProcess = (name) => {
  if (!name) return false
  return PROTECTED_PROCESSES.includes(name.toLowerCase())
}

const filteredProcesses = computed(() => {
  if (!filterText.value) return store.processes
  const f = filterText.value.toLowerCase()
  return store.processes.filter(p =>
    String(p.pid).includes(f) || (p.name || '').toLowerCase().includes(f)
  )
})

const selectedProcessName = computed(() => {
  if (!selectedPid.value) return '无'
  const p = store.processes.find(x => x.pid === selectedPid.value)
  return p ? `${p.name} (${p.pid})` : '无'
})

const getMemoryTagType = (mb) => {
  if (mb > 1024) return 'danger'
  if (mb > 256) return 'warning'
  return 'success'
}

const loadProcesses = async () => {
  await store.refreshProcesses()
  const total = store.processes.reduce((s, p) => s + (p.memory_usage_mb || 0), 0)
  systemMemory.value = Math.round(total)
}

const handleRowChange = (row) => {
  selectedPid.value = row ? row.pid : null
  store.selectedProcess = row
}

const handleRowDoubleClick = (row) => {
  selectedPid.value = row.pid
  handleCreateSnapshot()
}

const handleCreateSnapshot = async () => {
  if (!selectedPid.value) return

  const proc = store.processes.find(p => p.pid === selectedPid.value)
  if (proc && isProtectedProcess(proc.name)) {
    ElMessage.error(`进程 ${proc.name} 是受保护的系统进程，无法读取内存。请选择普通用户进程。`)
    return
  }

  if (!privilegeInfo.value || !privilegeInfo.value.is_admin) {
    const confirm = await ElMessageBox.confirm(
      '检测到当前未以管理员身份运行，可能无法访问部分受保护进程。是否继续？建议以管理员身份重启程序后重试。',
      '权限警告',
      {
        confirmButtonText: '继续尝试',
        cancelButtonText: '取消',
        type: 'warning',
        distinguishCancelAndClose: true
      }
    ).catch(() => 'cancel')
    if (confirm === 'cancel') return
  }

  try {
    await ElMessageBox.confirm(
      `确定为进程 PID ${selectedPid.value} 创建内存快照？这可能需要一些时间。`,
      '创建快照确认',
      { confirmButtonText: '确认', cancelButtonText: '取消', type: 'warning' }
    )
  } catch {
    return
  }
  creatingSnapshot.value = true
  try {
    const snap = await store.createSnapshot(selectedPid.value)
    ElMessage.success(`快照创建成功！ID: ${snap.id}`)
    router.push({ name: 'memory', params: { snapshotId: snap.id } })
  } catch (e) {
    ElMessage.error(`快照创建失败: ${e}`)
  } finally {
    creatingSnapshot.value = false
  }
}

const checkPrivilege = async () => {
  try {
    privilegeInfo.value = await store.checkPrivilege()
  } catch (e) {
    console.error('Failed to check privilege:', e)
  }
}

onMounted(async () => {
  await checkPrivilege()
  await store.refreshSnapshots()
  await loadProcesses()
})
</script>

<style lang="scss" scoped>
.process-list-container {
  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
    .filter-section, .actions {
      display: flex;
      gap: 12px;
    }
  }

  .stats-card {
    background: #16213e;
    border: 1px solid #0f3460;
    :deep(.el-card__body) { padding: 20px; }
  }

  .pid-badge {
    font-family: Consolas, monospace;
    font-weight: 600;
    color: #4ade80;
  }

  .process-name-cell {
    display: flex;
    align-items: center;
    gap: 8px;
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
