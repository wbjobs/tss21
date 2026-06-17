<template>
  <div class="pattern-scanner">
    <el-page-header
      :content="`快照 #${snapshotId} - 内存模式扫描`"
      @back="$router.push({ name: 'memory', params: { snapshotId } })"
      style="margin-bottom: 16px;"
    />

    <el-alert
      v-if="privilegeInfo && !privilegeInfo.is_admin"
      type="warning"
      show-icon
      :title="'权限提示：当前未以管理员身份运行，部分受保护进程可能无法访问'"
      :description="privilegeInfo.suggested_action"
      style="margin-bottom: 16px;"
    />

    <el-card class="scan-card">
      <template #header>
        <div class="card-header">
          <span><el-icon><Search /></el-icon> 特征码扫描</span>
        </div>
      </template>

      <div class="scan-controls">
        <div class="pattern-section">
          <label class="control-label">特征码（十六进制，支持空格分隔）</label>
          <div class="pattern-input-row">
            <el-input
              v-model="pattern"
              placeholder="例如: 48 8B C4 48 89 58 ?? 或 FF25{8}"
              size="large"
              clearable
              style="flex: 1;"
              :disabled="scanning"
            >
              <template #prefix>
                <el-icon color="#e94560"><Key /></el-icon>
              </template>
            </el-input>
            <el-button
              type="primary"
              size="large"
              :icon="scanning ? VideoPause : VideoPlay"
              :loading="scanning"
              @click="doScan"
              style="margin-left: 12px;"
            >
              {{ scanning ? '扫描中...' : '开始扫描' }}
            </el-button>
          </div>
          <div class="pattern-tips">
            <el-tag type="info" size="small" effect="plain">
              <strong>通配符:</strong> ?? 表示任意字节 &nbsp;|&nbsp;
              <strong>示例:</strong> 4D 5A ?? ?? 00 00 (匹配 MZ 头)
            </el-tag>
          </div>
        </div>

        <div class="preset-section">
          <span class="control-label">常用特征码:</span>
          <el-tag
            v-for="p in presetPatterns"
            :key="p.name"
            type="primary"
            effect="light"
            class="preset-tag"
            @click="pattern = p.pattern"
            :disabled="scanning"
            style="cursor: pointer;"
          >
            {{ p.name }}
          </el-tag>
        </div>

        <div v-if="scanning && scanProgress" class="progress-section">
          <div class="progress-header">
            <span class="progress-label">
              扫描进度: {{ scanProgress.percent }}% ({{ scanProgress.current }}/{{ scanProgress.total }} 个区域)
            </span>
            <span class="progress-info">
              已扫描: {{ (scanProgress.bytes_scanned / 1048576).toFixed(2) }} MB | 匹配: {{ scanProgress.matches_found }} 处
            </span>
          </div>
          <el-progress
            :percentage="scanProgress.percent"
            :status="scanning ? 'success' : undefined"
            :stroke-width="14"
            :color="progressColor"
          />
          <div v-if="scanProgress.current_region" class="current-region">
            <el-icon color="#a78bfa"><Loading /></el-icon>
            正在扫描: {{ scanProgress.current_region }}
          </div>
        </div>

        <el-divider />

        <div v-if="scanStats.total_matches !== undefined" class="scan-stats">
          <el-row :gutter="24">
            <el-col :span="5">
              <el-statistic title="匹配次数" :value="scanStats.total_matches" value-style="color: #e94560;" />
            </el-col>
            <el-col :span="4">
              <el-statistic title="扫描区域" :value="scanStats.regions_scanned || 0" />
            </el-col>
            <el-col :span="5">
              <el-statistic title="扫描大小 (MB)" :value="scanStats.mb_scanned || 0" :precision="2" />
            </el-col>
            <el-col :span="4">
              <el-statistic title="耗时 (ms)" :value="scanStats.elapsed_ms || 0" />
            </el-col>
            <el-col :span="3">
              <el-statistic title="跳过NOA" :value="scanStats.regions_skipped_no_access || 0" value-style="color: #f59e0b;" />
            </el-col>
            <el-col :span="3">
              <el-statistic title="跳过GUARD" :value="scanStats.regions_skipped_guard || 0" value-style="color: #6366f1;" />
            </el-col>
          </el-row>
        </div>
      </div>
    </el-card>

    <el-card class="result-card" style="margin-top: 16px;">
      <template #header>
        <div class="card-header">
          <span><el-icon><List /></el-icon> 扫描结果 ({{ results.length }})</span>
          <div class="result-actions">
            <el-button size="small" :icon="Download" @click="exportResults" :disabled="results.length === 0">
              导出结果
            </el-button>
          </div>
        </div>
      </template>

      <el-empty v-if="results.length === 0 && !scanning" description="暂无匹配结果，请输入特征码后扫描" />

      <el-table
        v-else
        :data="results"
        height="480"
        highlight-current-row
        style="width: 100%;"
        v-loading="scanning"
      >
        <el-table-column type="index" label="#" width="70" />
        <el-table-column label="地址" width="200">
          <template #default="{ row }">
            <span class="result-addr">0x{{ row.address }}</span>
          </template>
        </el-table-column>
        <el-table-column label="区域类型" width="100">
          <template #default="{ row }">
            <el-tag :type="regionTypeTag(row.region_type)" effect="dark" size="small">
              {{ regionTypeName(row.region_type) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="所属模块" min-width="200" show-overflow-tooltip>
          <template #default="{ row }">
            {{ row.module_name || '-' }}
          </template>
        </el-table-column>
        <el-table-column label="偏移" width="140">
          <template #default="{ row }">
            <span class="offset-text">0x{{ row.offset_in_region }}</span>
          </template>
        </el-table-column>
        <el-table-column label="上下文预览">
          <template #default="{ row }">
            <span class="context-text">{{ row.context_hex }}</span>
          </template>
        </el-table-column>
        <el-table-column label="操作" width="140" fixed="right">
          <template #default="{ row }">
            <el-button size="small" type="primary" link @click="gotoMemory(row)">
              查看内存
            </el-button>
            <el-button size="small" type="success" link @click="copyAddress(row.address)">
              复制地址
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import {
  Search, Key, VideoPlay, VideoPause, Download, List, Loading
} from '@element-plus/icons-vue'
import { useProcessStore } from '../stores/process'
import { listen } from '@tauri-apps/api/event'

const route = useRoute()
const router = useRouter()
const store = useProcessStore()
const snapshotId = Number(route.params.snapshotId)

const pattern = ref('4D 5A ?? ?? 00 00 00 00')
const results = ref([])
const scanning = ref(false)
const scanProgress = ref(null)
const scanStats = ref({})
const privilegeInfo = ref(null)
let unlistenFn = null

const progressColor = computed(() => {
  if (!scanProgress.value) return '#409eff'
  if (scanProgress.value.percent >= 100) return '#67c23a'
  if (scanProgress.value.percent >= 50) return '#e6a23c'
  return '#409eff'
})

const presetPatterns = [
  { name: 'MZ Header (PE文件头)', pattern: '4D 5A ?? ?? 00 00 00 00' },
  { name: 'PE Signature', pattern: '50 45 00 00' },
  { name: 'MessageBoxA x64', pattern: '48 8B C4 48 89 58 ?? 48 89 70 ?? 48 89 78 ??' },
  { name: 'CreateProcessW', pattern: '4C 8B DC 57 48 81 EC C0 00 00 00' },
  { name: '字符串 "Hello"', pattern: '48 65 6C 6C 6F' },
  { name: '全零区域 (8字节)', pattern: '00 00 00 00 00 00 00 00' },
  { name: '空指令填充 NOPs', pattern: '90 90 90 90 90' },
  { name: '返回指令 RETs', pattern: 'C3 C3 C3 C3' }
]

const regionTypeTag = (t) => ({ image: 'primary', heap: 'danger', stack: 'warning', private: 'info' }[t] || 'info')
const regionTypeName = (t) => ({ image: '映像', heap: '堆', stack: '栈', private: '私有', other: '其他' }[t] || t)

const copyAddress = (addr) => {
  navigator.clipboard.writeText('0x' + addr)
  ElMessage.success('地址已复制: 0x' + addr)
}

const doScan = async () => {
  if (!pattern.value.trim()) {
    ElMessage.warning('请输入特征码')
    return
  }
  if (scanning.value) return

  results.value = []
  scanProgress.value = null
  scanning.value = true

  try {
    const res = await store.scanPattern(snapshotId, pattern.value.trim())
    if (res && res.matches) {
      results.value = res.matches
      scanStats.value = {
        total_matches: res.total_matches || res.matches.length,
        regions_scanned: res.regions_scanned,
        mb_scanned: res.bytes_scanned ? res.bytes_scanned / 1048576 : 0,
        elapsed_ms: res.elapsed_ms,
        regions_skipped_no_access: res.regions_skipped_no_access || 0,
        regions_skipped_guard: res.regions_skipped_guard || 0,
      }
      ElMessage.success(`扫描完成，找到 ${results.value.length} 个匹配`)
    } else {
      results.value = []
      ElMessage.info('未找到匹配结果')
    }
  } catch (e) {
    ElMessage.error('扫描失败: ' + e)
  } finally {
    scanning.value = false
  }
}

const gotoMemory = (row) => {
  store.readMemory(snapshotId, row.address, 256).then(() => {
    if (store.memoryData) {
      store.memoryData.highlights = [0]
      store.memoryData.highlight_length = pattern.value.trim().split(/\s+/).filter(s => s && s !== '??').length
    }
  })
  router.push({ name: 'memory', params: { snapshotId } })
}

const exportResults = () => {
  const csv = [
    ['序号', '地址', '区域类型', '模块', '区域内偏移', '上下文'].join(','),
    ...results.value.map((r, i) => [
      i + 1,
      '0x' + r.address,
      regionTypeName(r.region_type),
      (r.module_name || '').replace(/,/g, ';'),
      '0x' + r.offset_in_region,
      r.context_hex || ''
    ].join(','))
  ].join('\n')
  const blob = new Blob([csv], { type: 'text/csv' })
  const a = document.createElement('a')
  a.href = URL.createObjectURL(blob)
  a.download = `scan_results_${snapshotId}_${Date.now()}.csv`
  a.click()
  ElMessage.success('结果已导出')
}

const checkPrivilege = async () => {
  try {
    const res = await store.checkPrivilege(snapshotId)
    privilegeInfo.value = res
  } catch (e) {
    console.error('Failed to check privilege:', e)
  }
}

onMounted(async () => {
  if (!store.currentSnapshot || store.currentSnapshot.id !== snapshotId) {
    await store.loadSnapshot(snapshotId)
  }
  await checkPrivilege()

  unlistenFn = await listen('scan-progress', (event) => {
    scanProgress.value = event.payload
  })
})

onBeforeUnmount(() => {
  if (unlistenFn) {
    unlistenFn()
  }
})
</script>

<style lang="scss" scoped>
.pattern-scanner {
  :deep(.el-page-header__left) { color: #e94560; cursor: pointer; }
  :deep(.el-page-header__title) { color: #fff; font-size: 18px; }

  .scan-card, .result-card {
    background: #16213e; border: 1px solid #0f3460;
    :deep(.el-card__header) { background: #0f3460; border-bottom: 1px solid #0f3460; }
  }

  .card-header {
    display: flex; justify-content: space-between; align-items: center;
    color: #fff; font-weight: 600;
  }

  .control-label {
    display: block;
    color: #8b9bb4;
    font-size: 13px;
    margin-bottom: 8px;
  }

  .pattern-input-row { display: flex; align-items: center; }

  .pattern-tips { margin-top: 10px; }

  .preset-section { margin-top: 16px; display: flex; align-items: center; flex-wrap: wrap; gap: 8px; }
  .preset-tag { margin-right: 0 !important; }

  .progress-section {
    margin-top: 16px;
    padding: 16px;
    background: #0f1a33;
    border-radius: 8px;
    border: 1px solid #1a2a4a;
  }

  .progress-header {
    display: flex;
    justify-content: space-between;
    margin-bottom: 10px;
    font-size: 13px;
  }

  .progress-label {
    color: #4ade80;
    font-weight: 600;
  }

  .progress-info {
    color: #8b9bb4;
  }

  .current-region {
    margin-top: 10px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: #a78bfa;
    font-family: Consolas, monospace;
  }

  .scan-stats { padding: 8px 4px; }

  .result-addr { font-family: Consolas, monospace; color: #4ade80; font-weight: 600; }
  .offset-text { font-family: Consolas, monospace; color: #a78bfa; }
  .context-text { font-family: Consolas, monospace; color: #60a5fa; font-size: 12px; }

  :deep(.el-table) {
    background: #16213e;
    --el-table-border-color: #0f3460;
    --el-table-header-bg-color: #0d1a33;
    --el-table-tr-bg-color: transparent;
    --el-table-row-hover-bg-color: #1a2a4a;
    color: #e0e0e0;
  }
  :deep(.el-table th.el-table__cell) { color: #8b9bb4; }
  :deep(.el-table .el-table__row.current-row) { background: #1a2a4a !important; }
}
</style>
