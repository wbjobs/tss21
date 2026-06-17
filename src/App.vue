<template>
  <el-container class="app-container">
    <el-header class="app-header">
      <div class="header-left">
        <el-icon size="24" color="#e94560"><Cpu /></el-icon>
        <h1 class="app-title">MemSnap Analyzer</h1>
        <span class="subtitle">进程内存快照分析器</span>
      </div>
      <el-menu
        mode="horizontal"
        :default-active="activeMenu"
        class="header-menu"
        background-color="transparent"
        text-color="#e0e0e0"
        active-text-color="#e94560"
        @select="onMenuSelect"
      >
        <el-menu-item index="processes">
          <el-icon><List /></el-icon>
          <span>进程列表</span>
        </el-menu-item>
        <el-menu-item index="snapshots">
          <el-icon><FolderOpened /></el-icon>
          <span>快照管理</span>
        </el-menu-item>
        <el-menu-item index="compare">
          <el-icon><CopyDocument /></el-icon>
          <span>快照对比</span>
        </el-menu-item>
        <el-menu-item index="monitor">
          <el-icon><VideoPlay /></el-icon>
          <span>内存监控</span>
        </el-menu-item>
      </el-menu>
    </el-header>

    <el-main class="app-main">
      <router-view />
    </el-main>
  </el-container>
</template>

<script setup>
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useProcessStore } from './stores/process'
import { Cpu, List, FolderOpened, CopyDocument, VideoPlay } from '@element-plus/icons-vue'

const route = useRoute()
const router = useRouter()
const store = useProcessStore()

const activeMenu = computed(() => {
  const name = route.name
  if (name === 'memory' || name === 'scan') return 'snapshots'
  return name || 'processes'
})

const onMenuSelect = (index) => {
  router.push({ name: index })
}
</script>

<style lang="scss" scoped>
.app-container {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: #1a1a2e;
}

.app-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: #16213e;
  border-bottom: 1px solid #0f3460;
  padding: 0 24px;
  height: 64px !important;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.app-title {
  font-size: 20px;
  font-weight: 700;
  color: #fff;
  margin: 0;
}

.subtitle {
  font-size: 12px;
  color: #8b9bb4;
}

.header-menu {
  border: none;
  flex: 1;
  margin-left: 48px;
}

.app-main {
  flex: 1;
  padding: 20px;
  overflow: auto;
}
</style>
