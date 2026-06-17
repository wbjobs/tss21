import { createRouter, createWebHashHistory } from 'vue-router'
import ProcessList from '../views/ProcessList.vue'
import SnapshotManager from '../views/SnapshotManager.vue'
import MemoryViewer from '../views/MemoryViewer.vue'
import PatternScanner from '../views/PatternScanner.vue'

const routes = [
  { path: '/', redirect: '/processes' },
  { path: '/processes', component: ProcessList, name: 'processes' },
  { path: '/snapshots', component: SnapshotManager, name: 'snapshots' },
  { path: '/memory/:snapshotId', component: MemoryViewer, name: 'memory', props: true },
  { path: '/scan/:snapshotId', component: PatternScanner, name: 'scan', props: true }
]

const router = createRouter({
  history: createWebHashHistory(),
  routes
})

export default router
