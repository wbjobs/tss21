<template>
  <div class="memory-monitor-container">
    <el-card class="config-card">
      <template #header>
        <div class="card-header">
          <el-icon size="18" :color="monitorStatus.is_running ? '#67c23a' : '#e94560'"><VideoPlay /></el-icon>
          <span>内存变化监控</span>
          <el-tag
            v-if="monitorStatus.is_running"
            type="success"
            effect="dark"
            size="small"
            style="margin-left: 12px"
          >
            <el-icon><Bell /></el-icon>
            监控中
          </el-tag>
          <el-tag
            v-else
            type="info"
            effect="light"
            size="small"
            style="margin-left: 12px"
          >
            空闲
          </el-tag>
        </div>
      </template>

      <el-row :gutter="20">
        <el-col :span="10">
          <div class="form-item">
            <label class="form-label">目标进程</label>
            <el-select
              v-model="selectedPid"
              placeholder="选择进程..."
              filterable
              style="width: 100%"
              :disabled="monitorStatus.is_running"
              :loading="loadingProcesses"
            >
              <el-option
                v-for="p in filteredProcesses"
                :key="p.pid"
                :label="`${p.name} (PID: ${p.pid}) - ${p.memory_usage_mb.toFixed(1)} MB`"
                :value="p.pid"
                :disabled="isProtected(p.name)"
              >
                <div class="proc-option">
                  <span class="proc-name">{{ p.name }}</span>
                  <span class="proc-pid">PID: {{ p.pid }}</span>
                  <span class="proc-mem">{{ p.memory_usage_mb.toFixed(1) }}MB</span>
                  <el-tag
                    v-if="isProtected(p.name)"
                    type="danger"
                    size="small"
                    effect="light"
                  >
                    受保护
                  </el-tag>
                </div>
              </el-option>
            </el-select>
          </div>
        </el-col>

        <el-col :span="6">
          <div class="form-item">
            <label class="form-label">轮询间隔 (ms)</label>
            <el-input-number
              v-model="intervalMs"
              :min="200"
              :max="30000"
              :step="100"
              :disabled="monitorStatus.is_running"
              controls-position="right"
              style="width: 100%"
            />
          </div>
        </el-col>

        <el-col :span="8">
          <div class="form-item">
            <label class="form-label">操作</label>
            <div class="button-row">
              <el-button
                v-if="!monitorStatus.is_running"
                type="primary"
                :icon="VideoPlay"
                size="default"
                :disabled="!selectedPid"
                @click="startMonitor"
              >
                启动监控
              </el-button>
              <el-button
                v-else
                type="danger"
                :icon="VideoPause"
                size="default"
                @click="stopMonitor"
              >
                停止监控
              </el-button>
              <el-button
                :icon="Refresh"
                size="default"
                :disabled="monitorStatus.is_running"
                @click="refreshProcesses"
              >
                刷新
              </el-button>
            </div>
          </div>
        </el-col>
      </el-row>

      <el-row v-if="monitorStatus.is_running || monitorStatus.pid" :gutter="16" class="status-row">
        <el-col :span="3">
          <el-statistic
            title="当前轮次"
            :value="displayStatus.current_cycle"
            :loading="!displayStatus.current_cycle"
          />
        </el-col>
        <el-col :span="3">
          <el-statistic
            title="总变化数"
            :value="displayStatus.total_changes"
          >
            <template #suffix>bytes</template>
          </el-statistic>
        </el-col>
        <el-col :span="3">
          <el-statistic
            title="日志条目"
            :value="displayStatus.log_entry_count"
          />
        </el-col>
        <el-col :span="3">
          <el-statistic
            title="轮询间隔"
            :value="displayStatus.interval_ms"
          >
            <template #suffix>ms</template>
          </el-statistic>
        </el-col>
        <el-col :span="3">
          <el-statistic
            title="监控 PID"
            :value="displayStatus.pid || '-'"
          />
        </el-col>
        <el-col :span="9">
          <div class="cycle-info-box">
            <div class="label">上轮变化</div>
            <el-tag
              :type="lastCycle.changes_found > 0 ? 'warning' : 'success'"
              effect="dark"
              size="large"
            >
              {{ lastCycle.changes_found }} 处
            </el-tag>
            <div class="label" style="margin-left: 16px">耗时</div>
            <el-tag type="info" effect="dark" size="large">
              {{ lastCycle.duration_ms }} ms
            </el-tag>
            <div class="label" style="margin-left: 16px">累计</div>
            <el-tag type="primary" effect="dark" size="large">
              {{ lastCycle.total_changes_so_far }}
            </el-tag>
          </div>
        </el-col>
      </el-row>

      <el-alert
        v-if="displayStatus.last_error"
        :title="displayStatus.last_error"
        type="error"
        show-icon
        :closable="false"
        style="margin-top: 12px"
      />
    </el-card>

    <el-card class="logs-card">
      <template #header>
        <div class="card-header" style="justify-content: space-between;">
          <div style="display: flex; align-items: center; gap: 8px;">
            <el-icon size="18" color="#e94560"><Document /></el-icon>
            <span>变化日志</span>
            <el-badge v-if="allLogs.length" :value="allLogs.length" class="log-badge" />
          </div>
          <div class="logs-toolbar">
            <el-radio-group v-model="changeFilter" size="default" style="margin-right: 12px">
              <el-radio-button value="all">全部</el-radio-button>
              <el-radio-button value="modified">修改</el-radio-button>
              <el-radio-button value="added">新增</el-radio-button>
              <el-radio-button value="removed">移除</el-radio-button>
            </el-radio-group>
            <el-input
              v-model="searchText"
              placeholder="搜索地址/模块..."
              clearable
              :prefix-icon="Search"
              style="width: 220px; margin-right: 12px"
            />
            <el-button
              :icon="Download"
              size="default"
              :disabled="!allLogs.length"
              @click="exportLogs"
            >
              导出
            </el-button>
            <el-button
              :icon="Delete"
              size="default"
              :disabled="!allLogs.length"
              type="danger"
              plain
              @click="clearLogs"
            >
              清空
            </el-button>
          </div>
        </div>
      </template>

      <div class="logs-table-wrapper">
        <el-table
          :data="displayLogs"
          height="500"
          stripe
          highlight-current-row
          size="small"
        >
          <el-table-column label="轮次" width="70" align="center">
            <template #default="{ row }">
              <el-tag size="small" type="info" effect="plain">#{{ row.cycle_index }}</el-tag>
            </template>
          </el-table-column>
          <el-table-column label="时间" width="170">
            <template #default="{ row }">
              {{ formatTime(row.timestamp) }}
            </template>
          </el-table-column>
          <el-table-column label="变化类型" width="90" align="center">
            <template #default="{ row }">
              <el-tag
                :type="changeTagType(row.change_type)"
                effect="dark"
                size="small"
              >
                {{ changeTypeLabel(row.change_type) }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column label="区域类型" width="80" align="center">
            <template #default="{ row }">
              {{ regionLabel(row.region_type) }}
            </template>
          </el-table-column>
          <el-table-column label="模块" min-width="150" show-overflow-tooltip>
            <template #default="{ row }">
              <span style="font-family: Consolas, monospace;">
                {{ row.module_name || '-' }}
              </span>
            </template>
          </el-table-column>
          <el-table-column label="区域基址" width="140">
            <template #default="{ row }">
              <span class="mono-text">0x{{ row.base_address }}</span>
            </template>
          </el-table-column>
          <el-table-column label="偏移" width="110">
            <template #default="{ row }">
              <span class="mono-text text-offset">+{{ formatHex(row.offset_in_region, 4) }}</span>
            </template>
          </el-table-column>
          <el-table-column label="绝对地址" width="160">
            <template #default="{ row }">
              <span class="mono-text text-addr">0x{{ row.absolute_address }}</span>
            </template>
          </el-table-column>
          <el-table-column label="旧值 → 新值" width="130">
            <template #default="{ row }">
              <span
                class="byte-old"
                :class="{ dim: row.old_value == null }"
              >{{ row.old_value != null ? formatByte(row.old_value) : '--' }}</span>
              <span class="byte-arrow">→</span>
              <span
                class="byte-new"
                :class="{ dim: row.new_value == null }"
              >{{ row.new_value != null ? formatByte(row.new_value) : '--' }}</span>
            </template>
          </el-table-column>
        </el-table>

        <el-empty
          v-if="displayLogs.length === 0"
          :description="monitorStatus.is_running ? '等待变化发生...' : '暂无日志，启动监控后会自动记录变化'"
          :image-size="120"
        >
          <template #image>
            <el-icon size="64" color="#cbd5e1"><Monitor /></el-icon>
          </template>
        </el-empty>
      </div>
    </el-card>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onBeforeUnmount, reactive, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import {
  VideoPlay, VideoPause, Refresh, Bell, Document,
  Search, Download, Delete, Monitor
} from '@element-plus/icons-vue'
import { useProcessStore } from '../stores/process'
import { listen } from '@tauri-apps/api/event'

const router = useRouter()
const store = useProcessStore()

const selectedPid = ref(null)
const intervalMs = ref(1000)
const loadingProcesses = ref(false)
const changeFilter = ref('all')
const searchText = ref('')
const allLogs = ref([])
const displayStatus = reactive({
  is_running: false,
  pid: null,
  process_name: null,
  started_at: null,
  current_cycle: 0,
  interval_ms: 1000,
  total_changes: 0,
  log_entry_count: 0,
  last_error: null,
})
const monitorStatus = reactive({
  is_running: false,
  pid: null,
})
const lastCycle = reactive({
  cycle_index: 0,
  duration_ms: 0,
  changes_found: 0,
  total_changes_so_far: 0,
})

let unlistenFn = null
let pollTimer = null

const PROTECTED = ['csrss.exe','smss.exe','wininit.exe','winlogon.exe','services.exe','lsass.exe','lsm.exe','svchost.exe','system','registry','memcompress.exe','fontdrvhost.exe','dwm.exe']

const filteredProcesses = computed(() => {
  const list = [...store.processes].sort((a, b) => {
    const aProt = isProtected(a.name) ? 1 : 0
    const bProt = isProtected(b.name) ? 1 : 0
    if (aProt !== bProt) return aProt - bProt
    return b.memory_usage_mb - a.memory_usage_mb
  })
  return list
})

const displayLogs = computed(() => {
  let list = [...allLogs.value].reverse()
  if (changeFilter.value !== 'all') {
    list = list.filter(l => l.change_type === changeFilter.value)
  }
  if (searchText.value) {
    const q = searchText.value.toLowerCase()
    list = list.filter(l =>
      l.base_address.toLowerCase().includes(q) ||
      l.absolute_address.toLowerCase().includes(q) ||
      l.module_name.toLowerCase().includes(q)
    )
  }
  return list.slice(0, 2000)
})

const isProtected = (n) => PROTECTED.includes((n || '').toLowerCase())

const formatTime = (ts) => {
  const d = new Date(Number(ts))
  return d.toLocaleTimeString('zh-CN', { hour12: false }) + '.' +
    String(d.getMilliseconds()).padStart(3, '0')
}

const formatHex = (n, len) => {
  let s = Number(n).toString(16).toUpperCase()
  while (s.length < len) s = '0' + s
  return s
}

const formatByte = (b) => {
  let s = Number(b).toString(16).toUpperCase()
  return s.length < 2 ? '0' + s : s
}

const changeTypeLabel = (t) => {
  switch (t) {
    case 'modified': return '修改'
    case 'added': return '新增'
    case 'removed': return '移除'
    default: return t
  }
}

const changeTagType = (t) => {
  switch (t) {
    case 'modified': return 'warning'
    case 'added': return 'success'
    case 'removed': return 'danger'
    default: return 'info'
  }
}

const regionLabel = (t) => {
  const map = { image: '映像', heap: '堆', stack: '栈', private: '私有', mapped: '映射', other: '其他' }
  return map[t] || t
}

const refreshProcesses = async () => {
  loadingProcesses.value = true
  try {
    await store.refreshProcesses()
  } finally {
    loadingProcesses.value = false
  }
}

const applyStatus = (s) => {
  if (!s) return
  monitorStatus.is_running = s.is_running
  monitorStatus.pid = s.pid
  Object.assign(displayStatus, {
    is_running: s.is_running,
    pid: s.pid,
    process_name: s.process_name,
    started_at: s.started_at,
    current_cycle: s.current_cycle,
    interval_ms: s.interval_ms,
    total_changes: s.total_changes,
    log_entry_count: s.log_entry_count,
    last_error: s.last_error,
  })
}

const startMonitor = async () => {
  if (!selectedPid.value) return
  const p = store.processes.find(x => x.pid === selectedPid.value)
  if (p && isProtected(p.name)) {
    ElMessage.error('系统保护进程无法监控')
    return
  }
  try {
    const st = await store.startMonitor(selectedPid.value, intervalMs.value)
    applyStatus(st)
    ElMessage.success(`已启动监控 ${p?.name || ''} (PID: ${selectedPid.value})`)

    if (st.is_running) {
      allLogs.value = await store.getMonitorLogs()
      startPolling()
    }
  } catch (e) {
    ElMessage.error(`启动失败: ${e}`)
  }
}

const stopMonitor = async () => {
  try {
    await ElMessageBox.confirm(
      '确定要停止监控吗？停止后不再接收新的变化。',
      '停止监控',
      { confirmButtonText: '停止', cancelButtonText: '取消', type: 'warning' }
    )
  } catch { return }

  try {
    const st = await store.stopMonitor()
    applyStatus(st)
    stopPolling()
    ElMessage.info('已停止监控')
    const remaining = await store.getMonitorLogs()
    allLogs.value = remaining
  } catch (e) {
    ElMessage.error(`停止失败: ${e}`)
  }
}

const startPolling = () => {
  if (pollTimer) return
  pollTimer = setInterval(async () => {
    try {
      const s = await store.getMonitorStatus()
      applyStatus(s)
      const logs = await store.getMonitorLogs(10000)
      if (logs.length > allLogs.value.length) {
        allLogs.value = logs
      }
    } catch (e) {
      console.error(e)
    }
  }, 500)
}

const stopPolling = () => {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

const clearLogs = async () => {
  try {
    await ElMessageBox.confirm('确定要清空所有日志？', '清空', { type: 'warning' })
  } catch { return }
  allLogs.value = []
}

const exportLogs = () => {
  const rows = allLogs.value.map(l => [
    l.cycle_index,
    formatTime(l.timestamp),
    changeTypeLabel(l.change_type),
    regionLabel(l.region_type),
    l.module_name,
    '0x' + l.base_address,
    '+' + formatHex(l.offset_in_region, 4),
    '0x' + l.absolute_address,
    l.old_value != null ? formatByte(l.old_value) : '--',
    l.new_value != null ? formatByte(l.new_value) : '--',
  ].join('\t'))

  const header = ['轮次', '时间', '类型', '区域', '模块', '基址', '偏移', '绝对地址', '旧', '新'].join('\t')
  const content = [header, ...rows].join('\n')
  const blob = new Blob([content], { type: 'text/tab-separated-values;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `monitor_logs_${Date.now()}.tsv`
  a.click()
  URL.revokeObjectURL(url)
  ElMessage.success(`已导出 ${allLogs.value.length} 条日志`)
}

onMounted(async () => {
  await refreshProcesses()
  const s = await store.getMonitorStatus()
  applyStatus(s)
  if (s.is_running) {
    allLogs.value = await store.getMonitorLogs()
    startPolling()
  }

  try {
    unlistenFn = await listen('monitor-cycle', (event) => {
      const e = event.payload
      Object.assign(lastCycle, {
        cycle_index: e.cycle_index,
        duration_ms: e.duration_ms,
        changes_found: e.changes_found,
        total_changes_so_far: e.total_changes_so_far,
      })
      if (e.changes_found > 0) {
        displayStatus.current_cycle = e.cycle_index
        displayStatus.total_changes = e.total_changes_so_far
      }
    })
  } catch (e) {
    console.error('listen monitor-cycle failed:', e)
  }
})

onBeforeUnmount(() => {
  if (unlistenFn) {
    try { unlistenFn() } catch (e) {}
    unlistenFn = null
  }
  stopPolling()
})
</script>

<style lang="scss" scoped>
.memory-monitor-container {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.card-header {
  display: flex;
  align-items: center;
  font-weight: 600;
  font-size: 15px;
}

.form-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-label {
  font-size: 13px;
  color: #475569;
  font-weight: 500;
}

.button-row {
  display: flex;
  gap: 8px;
}

.proc-option {
  display: flex;
  align-items: center;
  gap: 12px;

  .proc-name {
    font-weight: 600;
    color: #0f172a;
    min-width: 160px;
  }

  .proc-pid {
    color: #64748b;
    font-size: 12px;
    min-width: 80px;
  }

  .proc-mem {
    color: #3b82f6;
    font-size: 12px;
    margin-right: auto;
  }
}

.status-row {
  margin-top: 16px;
  padding-top: 16px;
  border-top: 1px dashed #e5e7eb;
}

.cycle-info-box {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  background: #f8fafc;
  border-radius: 8px;
  height: 100%;
  flex-wrap: wrap;

  .label {
    font-size: 12px;
    color: #64748b;
  }
}

.log-badge {
  margin-left: 8px;
}

.logs-toolbar {
  display: flex;
  align-items: center;
}

.logs-table-wrapper {
  position: relative;
}

.mono-text {
  font-family: 'Consolas', 'Monaco', monospace;
}

.text-offset {
  color: #64748b;
}

.text-addr {
  color: #2563eb;
  font-weight: 500;
}

.byte-old {
  color: #dc2626;
  font-family: Consolas, monospace;
  font-weight: 600;

  &.dim {
    color: #94a3b8;
    font-weight: 400;
  }
}

.byte-arrow {
  color: #64748b;
  margin: 0 4px;
}

.byte-new {
  color: #16a34a;
  font-family: Consolas, monospace;
  font-weight: 600;

  &.dim {
    color: #94a3b8;
    font-weight: 400;
  }
}
</style>
