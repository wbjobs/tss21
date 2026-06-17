<template>
  <div class="memory-viewer">
    <el-page-header
      :content="`快照 #${snapshotId} - 内存布局`"
      @back="$router.push({ name: 'snapshots' })"
      style="margin-bottom: 16px;"
    />

    <el-card v-if="store.currentSnapshot" class="snapshot-info-card">
      <el-row :gutter="24">
        <el-col :span="4">
          <div class="info-item">
            <span class="label">进程名</span>
            <span class="value">{{ store.currentSnapshot.process_name }}</span>
          </div>
        </el-col>
        <el-col :span="4">
          <div class="info-item">
            <span class="label">PID</span>
            <span class="value">{{ store.currentSnapshot.pid }}</span>
          </div>
        </el-col>
        <el-col :span="4">
          <div class="info-item">
            <span class="label">大小</span>
            <span class="value">{{ (store.currentSnapshot.total_size_mb || 0).toFixed(2) }} MB</span>
          </div>
        </el-col>
        <el-col :span="4">
          <div class="info-item">
            <span class="label">区域数</span>
            <span class="value">{{ store.currentSnapshot.region_count }}</span>
          </div>
        </el-col>
        <el-col :span="8">
          <div class="info-item">
            <span class="label">创建时间</span>
            <span class="value">{{ new Date(store.currentSnapshot.created_at).toLocaleString('zh-CN') }}</span>
          </div>
        </el-col>
      </el-row>
    </el-card>

    <el-row :gutter="16" style="margin-top: 16px;">
      <el-col :span="9">
        <el-card class="regions-card">
          <template #header>
            <div class="card-header">
              <span>内存区域</span>
              <div class="region-tabs">
                <el-radio-group v-model="regionFilter" size="small" @change="filterRegions">
                  <el-radio-button value="all">全部</el-radio-button>
                  <el-radio-button value="image">映像</el-radio-button>
                  <el-radio-button value="heap">堆</el-radio-button>
                  <el-radio-button value="stack">栈</el-radio-button>
                  <el-radio-button value="private">私有</el-radio-button>
                </el-radio-group>
              </div>
            </div>
          </template>

          <el-table
            :data="filteredRegions"
            height="560"
            highlight-current-row
            @current-change="selectRegion"
            style="width: 100%;"
          >
            <el-table-column label="类型" width="80">
              <template #default="{ row }">
                <el-tag :type="getRegionTagType(row.type)" effect="dark" size="small">
                  {{ getRegionLabel(row.type) }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column label="基址" width="150">
              <template #default="{ row }">
                <span class="addr-text">0x{{ row.base_address }}</span>
              </template>
            </el-table-column>
            <el-table-column label="大小" width="100">
              <template #default="{ row }">
                <span>{{ formatSize(row.region_size) }}</span>
              </template>
            </el-table-column>
            <el-table-column label="保护">
              <template #default="{ row }">
                <span class="protect-text">{{ row.protection }}</span>
              </template>
            </el-table-column>
            <el-table-column label="模块/详情" min-width="160" show-overflow-tooltip>
              <template #default="{ row }">
                <span>{{ row.module_name || row.details || '-' }}</span>
              </template>
            </el-table-column>
          </el-table>
        </el-card>
      </el-col>

      <el-col :span="15">
        <el-card class="hex-card">
          <template #header>
            <div class="card-header">
              <span>十六进制查看器</span>
              <div class="hex-controls">
                <el-input
                  v-model="gotoAddress"
                  placeholder="跳转地址 (如 7FF6...)"
                  size="small"
                  style="width: 220px; margin-right: 8px;"
                />
                <el-input-number
                  v-model="readLength"
                  :min="64"
                  :max="65536"
                  :step="64"
                  size="small"
                  style="width: 140px; margin-right: 8px;"
                />
                <el-button size="small" type="primary" @click="readAtGoAddress" :icon="Search">
                  读取
                </el-button>
                <el-button
                  size="small"
                  type="warning"
                  style="margin-left: 8px;"
                  :icon="MagicStick"
                  @click="$router.push({ name: 'scan', params: { snapshotId } })"
                >
                  模式扫描
                </el-button>
              </div>
            </div>
          </template>

          <div v-if="!store.memoryData" class="hex-placeholder">
            <el-empty description="请选择一个内存区域或输入地址查看" :image-size="100" />
          </div>

          <div v-else class="hex-viewer-wrapper">
            <div class="hex-viewer">
              <div
                v-for="(line, idx) in hexLines"
                :key="idx"
                class="hex-row"
              >
                <span class="hex-address">{{ line.address }}</span>
                <span class="hex-bytes">
                  <span
                    v-for="(byte, bIdx) in line.bytes"
                    :key="bIdx"
                    :class="{ highlight: isHighlighted(line.startOffset + bIdx) }"
                  >{{ byte }}</span>
                </span>
                <span class="hex-ascii">
                  <span
                    v-for="(ch, cIdx) in line.ascii"
                    :key="cIdx"
                    :class="{ 'non-printable': ch === '.' }"
                  >{{ ch }}</span>
                </span>
              </div>
            </div>

            <div v-if="store.memoryData.highlights && store.memoryData.highlights.length > 0" class="highlights-info">
              <el-alert
                type="success"
                :closable="false"
                show-icon
                :title="`匹配高亮 ${store.memoryData.highlights.length} 处结果`"
              />
            </div>
          </div>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, markRaw } from 'vue'
import { useRoute } from 'vue-router'
import { ElMessage } from 'element-plus'
import { Search, MagicStick } from '@element-plus/icons-vue'
import { useProcessStore } from '../stores/process'

const route = useRoute()
const store = useProcessStore()
const snapshotId = Number(route.params.snapshotId)

const regionFilter = ref('all')
const filteredRegionList = ref([])
const gotoAddress = ref('')
const readLength = ref(4096)

const filteredRegions = computed(() => filteredRegionList.value)

const hexLines = computed(() => {
  if (!store.memoryData) return []
  const { data, base_address } = store.memoryData
  const bytes = typeof data === 'string' ? Array.from(data).map(c => c.charCodeAt(0)) : data
  const lines = []
  for (let i = 0; i < bytes.length; i += 16) {
    const rowBytes = bytes.slice(i, i + 16)
    const hexStrs = rowBytes.map(b => b.toString(16).padStart(2, '0').toUpperCase())
    while (hexStrs.length < 16) hexStrs.push('  ')
    const ascii = rowBytes.map(b => (b >= 32 && b < 127) ? String.fromCharCode(b) : '.')
    while (ascii.length < 16) ascii.push(' ')
    const addrNum = BigInt('0x' + (base_address || '0')) + BigInt(i)
    const addr = '0x' + addrNum.toString(16).toUpperCase().padStart(16, '0')
    lines.push({
      address: addr,
      bytes: hexStrs,
      ascii: ascii,
      startOffset: i
    })
  }
  return lines
})

const getRegionTagType = (type) => {
  const map = { image: 'primary', heap: 'danger', stack: 'warning', private: 'info' }
  return map[type] || 'info'
}
const getRegionLabel = (type) => {
  const map = { image: '映像', heap: '堆', stack: '栈', private: '私有', other: '其他' }
  return map[type] || type
}
const formatSize = (bytes) => {
  if (bytes >= 1048576) return (bytes / 1048576).toFixed(2) + ' MB'
  if (bytes >= 1024) return (bytes / 1024).toFixed(2) + ' KB'
  return bytes + ' B'
}

const filterRegions = () => {
  if (regionFilter.value === 'all') {
    filteredRegionList.value = [...store.memoryRegions]
  } else {
    filteredRegionList.value = store.memoryRegions.filter(r => r.type === regionFilter.value)
  }
}

const selectRegion = async (row) => {
  if (!row) return
  gotoAddress.value = row.base_address
  const len = Math.min(Number(row.region_size) || 4096, readLength.value)
  await store.readMemory(snapshotId, row.base_address, len)
}

const readAtGoAddress = async () => {
  if (!gotoAddress.value) return
  let addr = gotoAddress.value.trim()
  if (addr.startsWith('0x') || addr.startsWith('0X')) addr = addr.slice(2)
  try {
    await store.readMemory(snapshotId, addr, readLength.value)
  } catch (e) {
    ElMessage.error('读取失败: ' + e)
  }
}

const isHighlighted = (offset) => {
  if (!store.memoryData?.highlights) return false
  const hl = store.memoryData.highlights
  const len = store.memoryData.highlight_length || 0
  for (const start of hl) {
    if (offset >= start && offset < start + len) return true
  }
  return false
}

onMounted(async () => {
  await store.loadSnapshot(snapshotId)
  filterRegions()
})
</script>

<style lang="scss" scoped>
.memory-viewer {
  :deep(.el-page-header__left) { color: #e94560; cursor: pointer; }
  :deep(.el-page-header__title) { color: #fff; font-size: 18px; }

  .snapshot-info-card {
    background: #16213e; border: 1px solid #0f3460;
    .info-item { display: flex; flex-direction: column; gap: 4px; }
    .label { font-size: 12px; color: #8b9bb4; }
    .value { font-size: 15px; color: #fff; font-weight: 600; }
  }

  .regions-card, .hex-card {
    background: #16213e; border: 1px solid #0f3460;
    :deep(.el-card__header) { background: #0f3460; border-bottom: 1px solid #0f3460; }
  }
  .card-header {
    display: flex; justify-content: space-between; align-items: center;
    color: #fff; font-weight: 600;
  }

  .addr-text { font-family: Consolas, monospace; color: #4ade80; font-size: 12px; }
  .protect-text { font-family: Consolas, monospace; color: #a78bfa; font-size: 12px; }

  :deep(.el-table) {
    background: #16213e;
    --el-table-border-color: #0f3460;
    --el-table-header-bg-color: #0d1a33;
    --el-table-tr-bg-color: transparent;
    --el-table-row-hover-bg-color: #1a2a4a;
    color: #e0e0e0; font-size: 13px;
  }
  :deep(.el-table th.el-table__cell) { color: #8b9bb4; }
  :deep(.el-table .el-table__row.current-row) { background: #1a2a4a !important; }

  .hex-viewer-wrapper {
    max-height: 560px;
    overflow: auto;
    background: #0d0d1a;
    border-radius: 6px;
    padding: 4px;
  }
  .hex-placeholder { padding: 40px 0; }

  .highlights-info { margin-top: 12px; }
}
</style>
