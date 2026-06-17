<template>
  <div class="snapshot-compare-container">
    <el-card class="selection-card">
      <template #header>
        <div class="card-header">
          <el-icon size="18" color="#e94560"><CopyDocument /></el-icon>
          <span>选择快照进行对比（必须是同一进程的不同时间点）</span>
        </div>
      </template>

      <el-row :gutter="24">
        <el-col :span="10">
          <div class="snapshot-select-box">
            <div class="label-box">
              <el-tag type="info" effect="dark" size="large">快照 A（基准）</el-tag>
            </div>
            <el-select
              v-model="snapshotAId"
              placeholder="选择快照 A"
              style="width: 100%; margin-top: 8px;"
              :loading="store.loading"
              filterable
            >
              <el-option
                v-for="s in filteredSnapshots"
                :key="s.id"
                :label="`#${s.id} - ${s.process_name} (PID:${s.pid}) - ${formatTime(s.created_at)}`"
                :value="s.id"
              >
                <span style="float: left">{{ s.process_name }} (PID: {{ s.pid }})</span>
                <span style="float: right; color: #8492a6; font-size: 12px">
                  #{{ s.id }} · {{ s.total_size_mb.toFixed(1) }}MB
                </span>
              </el-option>
            </el-select>
            <div v-if="snapshotA" class="snapshot-info">
              <el-descriptions :column="1" size="small">
                <el-descriptions-item label="进程">{{ snapshotA.process_name }}</el-descriptions-item>
                <el-descriptions-item label="大小">{{ snapshotA.total_size_mb.toFixed(2) }} MB</el-descriptions-item>
                <el-descriptions-item label="区域数">{{ snapshotA.region_count }}</el-descriptions-item>
                <el-descriptions-item label="创建时间">{{ formatTime(snapshotA.created_at) }}</el-descriptions-item>
              </el-descriptions>
            </div>
          </div>
        </el-col>

        <el-col :span="4" class="vs-column">
          <el-icon size="40" color="#e94560"><ArrowRightBold /></el-icon>
          <div class="vs-text">VS</div>
        </el-col>

        <el-col :span="10">
          <div class="snapshot-select-box">
            <div class="label-box">
              <el-tag type="warning" effect="dark" size="large">快照 B（对比目标）</el-tag>
            </div>
            <el-select
              v-model="snapshotBId"
              placeholder="选择快照 B"
              style="width: 100%; margin-top: 8px;"
              :loading="store.loading"
              filterable
            >
              <el-option
                v-for="s in compatibleSnapshots"
                :key="s.id"
                :label="`#${s.id} - ${s.process_name} - ${formatTime(s.created_at)}`"
                :value="s.id"
                :disabled="s.id === snapshotAId"
              >
                <span style="float: left">{{ s.process_name }} (PID: {{ s.pid }})</span>
                <span style="float: right; color: #8492a6; font-size: 12px">
                  #{{ s.id }} · {{ s.total_size_mb.toFixed(1) }}MB
                </span>
              </el-option>
            </el-select>
            <div v-if="snapshotB" class="snapshot-info">
              <el-descriptions :column="1" size="small">
                <el-descriptions-item label="进程">{{ snapshotB.process_name }}</el-descriptions-item>
                <el-descriptions-item label="大小">{{ snapshotB.total_size_mb.toFixed(2) }} MB</el-descriptions-item>
                <el-descriptions-item label="区域数">{{ snapshotB.region_count }}</el-descriptions-item>
                <el-descriptions-item label="创建时间">{{ formatTime(snapshotB.created_at) }}</el-descriptions-item>
              </el-descriptions>
            </div>
          </div>
        </el-col>
      </el-row>

      <div class="action-row">
        <el-button
          type="primary"
          :icon="RefreshRight"
          size="large"
          :disabled="!canCompare"
          @click="doCompare"
          :loading="comparing"
        >
          开始对比
        </el-button>
        <el-button size="large" @click="clearSelection">
          清除选择
        </el-button>
        <el-button
          size="large"
          :icon="FolderOpened"
          @click="$router.push({ name: 'snapshots' })"
        >
          去管理快照
        </el-button>
      </div>
    </el-card>

    <el-card v-if="diffResult" class="diff-card">
      <template #header>
        <div class="card-header">
          <el-icon size="18" color="#e94560"><DataLine /></el-icon>
          <span>对比结果：{{ diffResult.process_name }}</span>
          <el-tag type="info" effect="dark" style="margin-left:12px">
            耗时 {{ diffResult.elapsed_ms }}ms
          </el-tag>
        </div>
      </template>

      <el-row :gutter="16" class="stats-row">
        <el-col :span="4">
          <el-statistic title="对比区域数" :value="diffResult.regions_compared" />
        </el-col>
        <el-col :span="4">
          <el-statistic title="修改区域" :value="diffResult.regions_modified">
            <template #suffix>
              <el-tag type="warning" size="small" effect="plain">MODIFIED</el-tag>
            </template>
          </el-statistic>
        </el-col>
        <el-col :span="4">
          <el-statistic title="新增区域" :value="diffResult.regions_added">
            <template #suffix>
              <el-tag type="success" size="small" effect="plain">ADDED</el-tag>
            </template>
          </el-statistic>
        </el-col>
        <el-col :span="4">
          <el-statistic title="移除区域" :value="diffResult.regions_removed">
            <template #suffix>
              <el-tag type="danger" size="small" effect="plain">REMOVED</el-tag>
            </template>
          </el-statistic>
        </el-col>
        <el-col :span="4">
          <el-statistic title="变化字节数" :value="diffResult.total_changed_bytes">
            <template #suffix>bytes</template>
          </el-statistic>
        </el-col>
        <el-col :span="4">
          <el-statistic title="含变化的区域" :value="diffResult.diff_regions.length" />
        </el-col>
      </el-row>

      <el-divider />

      <div class="diff-regions-section">
        <div class="toolbar-row">
          <div class="filter-section">
            <el-radio-group v-model="changeTypeFilter" size="default">
              <el-radio-button value="all">全部</el-radio-button>
              <el-radio-button value="modified">修改</el-radio-button>
              <el-radio-button value="added">新增</el-radio-button>
              <el-radio-button value="removed">移除</el-radio-button>
            </el-radio-group>
            <el-input
              v-model="searchText"
              placeholder="搜索地址/模块名..."
              clearable
              :prefix-icon="Search"
              style="width: 240px; margin-left: 12px"
            />
          </div>
          <div class="count-info">
            显示 {{ filteredRegions.length }} / {{ diffResult.diff_regions.length }}
          </div>
        </div>

        <div v-for="(region, idx) in filteredRegions" :key="idx" class="diff-region-item">
          <div class="region-header">
            <div class="region-title">
              <el-tag
                :type="changeTagType(region.change_type)"
                effect="dark"
                size="default"
              >
                {{ changeTypeLabel(region.change_type) }}
              </el-tag>
              <span class="region-address">
                <el-icon><Location /></el-icon>
                0x{{ region.base_address }}
              </span>
              <span class="region-modname" v-if="region.module_name">
                <el-icon><Files /></el-icon>
                {{ region.module_name }}
              </span>
              <el-tag size="small" type="info" effect="plain">
                {{ regionLabel(region.region_type) }}
              </el-tag>
            </div>
            <div class="region-stats">
              <span class="stat-item">
                <el-icon><Pointer /></el-icon>
                {{ region.change_count }} 处变化
              </span>
              <el-progress
                :percentage="Math.min(region.change_percent, 100)"
                :stroke-width="8"
                :color="changeColor(region.change_type)"
                :show-text="false"
                style="width: 80px; display: inline-block"
              />
              <span class="stat-item">{{ (region.change_percent).toFixed(2) }}%</span>
              <el-button
                link
                type="primary"
                size="small"
                @click="toggleExpand(idx)"
              >
                {{ expanded[idx] ? '收起' : '展开字节' }}
                <el-icon>{{ expanded[idx] ? ArrowUp : ArrowDown }}</el-icon>
              </el-button>
            </div>
          </div>

          <div v-if="expanded[idx]" class="region-bytes">
            <div class="bytes-preview-header">
              偏移 | 旧值 → 新值 | 绝对地址
            </div>
            <div class="bytes-list">
              <div
                v-for="(b, bi) in previewBytes(region)"
                :key="bi"
                class="byte-item"
              >
                <span class="offset">+{{ formatHex(b.offset_in_region, 4) }}</span>
                <span class="old-val" :class="{ dim: b.old_value == null }">
                  {{ b.old_value != null ? formatByte(b.old_value) : '--' }}
                </span>
                <span class="arrow">→</span>
                <span class="new-val" :class="{ dim: b.new_value == null }">
                  {{ b.new_value != null ? formatByte(b.new_value) : '--' }}
                </span>
                <span class="abs-addr">0x{{ b.absolute_address }}</span>
              </div>
              <div
                v-if="region.changed_bytes.length > 200"
                class="more-hint"
              >
                仅显示前 200 条，共 {{ region.changed_bytes.length }} 处变化
              </div>
            </div>
          </div>
        </div>

        <el-empty
          v-if="filteredRegions.length === 0"
          description="没有符合条件的变化区域"
          :image-size="100"
        />
      </div>
    </el-card>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, reactive } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import {
  CopyDocument, ArrowRightBold, RefreshRight, FolderOpened, DataLine,
  Search, Location, Files, Pointer, ArrowUp, ArrowDown
} from '@element-plus/icons-vue'
import { useProcessStore } from '../stores/process'

const router = useRouter()
const store = useProcessStore()

const snapshotAId = ref(null)
const snapshotBId = ref(null)
const comparing = ref(false)
const diffResult = ref(null)
const changeTypeFilter = ref('all')
const searchText = ref('')
const expanded = reactive({})

const snapshotA = computed(() => store.snapshots.find(s => s.id === snapshotAId.value) || null)
const snapshotB = computed(() => store.snapshots.find(s => s.id === snapshotBId.value) || null)

const filteredSnapshots = computed(() => {
  const list = [...store.snapshots].sort((a, b) => b.created_at - a.created_at)
  return list
})

const compatibleSnapshots = computed(() => {
  if (!snapshotAId.value) return filteredSnapshots.value
  const a = snapshotA.value
  if (!a) return filteredSnapshots.value
  return filteredSnapshots.value.filter(s => s.pid === a.pid)
})

const canCompare = computed(() =>
  snapshotAId.value && snapshotBId.value &&
  snapshotAId.value !== snapshotBId.value
)

const filteredRegions = computed(() => {
  if (!diffResult.value) return []
  let list = diffResult.value.diff_regions
  if (changeTypeFilter.value !== 'all') {
    list = list.filter(r => r.change_type === changeTypeFilter.value)
  }
  if (searchText.value) {
    const q = searchText.value.toLowerCase()
    list = list.filter(r =>
      r.base_address.toLowerCase().includes(q) ||
      r.module_name.toLowerCase().includes(q)
    )
  }
  return list
})

const toggleExpand = (idx) => {
  expanded[idx] = !expanded[idx]
}

const previewBytes = (region) => {
  return region.changed_bytes.slice(0, 200)
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

const formatTime = (ts) => {
  const d = new Date(Number(ts))
  return d.toLocaleString('zh-CN', { hour12: false })
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

const changeColor = (t) => {
  switch (t) {
    case 'modified': return '#e6a23c'
    case 'added': return '#67c23a'
    case 'removed': return '#f56c6c'
    default: return '#409eff'
  }
}

const regionLabel = (t) => {
  const map = { image: '映像', heap: '堆', stack: '栈', private: '私有', mapped: '映射', other: '其他' }
  return map[t] || t
}

const doCompare = async () => {
  if (!canCompare.value) return
  comparing.value = true
  diffResult.value = null
  try {
    diffResult.value = await store.compareSnapshots(snapshotAId.value, snapshotBId.value)
    ElMessage.success('对比完成！')
  } catch (e) {
    ElMessage.error(`对比失败: ${e}`)
  } finally {
    comparing.value = false
  }
}

const clearSelection = () => {
  snapshotAId.value = null
  snapshotBId.value = null
  diffResult.value = null
  Object.keys(expanded).forEach(k => delete expanded[k])
}

onMounted(async () => {
  await store.refreshSnapshots()
})
</script>

<style lang="scss" scoped>
.snapshot-compare-container {
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

.snapshot-select-box {
  padding: 12px;
  border-radius: 8px;
  background: #f8fafc;
  border: 1px solid #e5e7eb;
}

.label-box {
  display: flex;
  align-items: center;
  justify-content: center;
}

.snapshot-info {
  margin-top: 12px;
  padding: 12px;
  background: #fff;
  border-radius: 6px;
  border: 1px dashed #e5e7eb;
}

.vs-column {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 40px 0;
}

.vs-text {
  font-size: 24px;
  font-weight: 800;
  color: #e94560;
  letter-spacing: 4px;
}

.action-row {
  margin-top: 20px;
  display: flex;
  justify-content: center;
  gap: 12px;
}

.stats-row {
  padding: 8px 0;
}

.diff-regions-section {
  margin-top: 12px;
}

.toolbar-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.filter-section {
  display: flex;
  align-items: center;
}

.count-info {
  color: #64748b;
  font-size: 13px;
}

.diff-region-item {
  margin-bottom: 12px;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  background: #fff;
  overflow: hidden;
}

.region-header {
  padding: 12px 16px;
  background: #f8fafc;
  border-bottom: 1px solid #e5e7eb;
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 12px;
}

.region-title {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.region-address {
  font-family: 'Consolas', 'Monaco', monospace;
  color: #1e293b;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 4px;
}

.region-modname {
  color: #475569;
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 13px;
}

.region-stats {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
  color: #475569;
}

.stat-item {
  display: flex;
  align-items: center;
  gap: 4px;
  color: #334155;
  font-weight: 500;
}

.region-bytes {
  padding: 12px 16px;
  background: #0f172a;
}

.bytes-preview-header {
  color: #94a3b8;
  font-size: 12px;
  padding: 4px 8px;
  font-family: 'Consolas', monospace;
  letter-spacing: 1px;
  margin-bottom: 8px;
}

.bytes-list {
  max-height: 400px;
  overflow: auto;
  background: #1e293b;
  border-radius: 6px;
  padding: 8px;
}

.byte-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 3px 8px;
  font-family: 'Consolas', 'Monaco', monospace;
  font-size: 13px;
  border-radius: 4px;

  &:hover {
    background: #334155;
  }
}

.byte-item .offset {
  color: #94a3b8;
  min-width: 60px;
}

.byte-item .old-val {
  color: #f87171;
  min-width: 28px;
  text-align: center;

  &.dim {
    color: #475569;
  }
}

.byte-item .arrow {
  color: #64748b;
}

.byte-item .new-val {
  color: #4ade80;
  min-width: 28px;
  text-align: center;

  &.dim {
    color: #475569;
  }
}

.byte-item .abs-addr {
  color: #60a5fa;
  margin-left: auto;
}

.more-hint {
  padding: 8px;
  text-align: center;
  color: #94a3b8;
  font-size: 12px;
}
</style>
