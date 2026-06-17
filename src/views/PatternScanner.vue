<template>
  <div class="pattern-scanner">
    <el-page-header
      :content="`快照 #${snapshotId} - 内存模式扫描`"
      @back="$router.push({ name: 'memory', params: { snapshotId } })"
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
            >
              <template #prefix>
                <el-icon color="#e94560"><Key /></el-icon>
              </template>
            </el-input>
            <el-button
              type="primary"
              size="large"
              :icon="VideoPlay"
              :loading="store.loading"
              @click="doScan"
              style="margin-left: 12px;"
            >
              开始扫描
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
            style="cursor: pointer;"
          >
            {{ p.name }}
          </el-tag>
        </div>

        <el-divider />

        <div v-if="scanStats.total_matches !== undefined" class="scan-stats">
          <el-row :gutter="24">
            <el-col :span="6">
              <el-statistic title="匹配次数" :value="scanStats.total_matches" value-style="color: #e94560;" />
            </el-col>
            <el-col :span="6">
              <el-statistic title="扫描区域" :value="scanStats.regions_scanned || 0" />
            </el-col>
            <el-col :span="6">
              <el-statistic title="扫描大小 (MB)" :value="scanStats.mb_scanned || 0" :precision="2" />
            </el-col>
            <el-col :span="6">
              <el-statistic title="耗时 (ms)" :value="scanStats.elapsed_ms || 0" />
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

      <el-empty v-if="results.length === 0 && !store.loading" description="暂无匹配结果，请输入特征码后扫描" />

      <el-table
        v-else
        :data="results"
        height="480"
        highlight-current-row
        style="width: 100%;"
        v-loading="store.loading"
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
import { ref, onMounted, markRaw } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import {
  Search, Key, VideoPlay, Download, List
} from '@element-plus/icons-vue'
import { useProcessStore } from '../stores/process'

const route = useRoute()
const router = useRouter()
const store = useProcessStore()
const snapshotId = Number(route.params.snapshotId)

const pattern = ref('4D 5A ?? ?? 00 00 00 00')
const results = ref([])
const scanStats = ref({})

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
  results.value = []
  const res = await store.scanPattern(snapshotId, pattern.value.trim())
  if (res && res.matches) {
    results.value = res.matches
    scanStats.value = {
      total_matches: res.total_matches || res.matches.length,
      regions_scanned: res.regions_scanned,
      mb_scanned: res.bytes_scanned ? res.bytes_scanned / 1048576 : 0,
      elapsed_ms: res.elapsed_ms
    }
    ElMessage.success(`扫描完成，找到 ${results.value.length} 个匹配`)
  } else {
    results.value = []
    ElMessage.info('未找到匹配结果')
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

onMounted(async () => {
  if (!store.currentSnapshot || store.currentSnapshot.id !== snapshotId) {
    await store.loadSnapshot(snapshotId)
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
