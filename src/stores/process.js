import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/tauri'

export const useProcessStore = defineStore('process', {
  state: () => ({
    processes: [],
    selectedProcess: null,
    snapshots: [],
    currentSnapshot: null,
    memoryRegions: [],
    currentRegion: null,
    memoryData: null,
    scanResults: [],
    loading: false
  }),

  actions: {
    async refreshProcesses() {
      this.loading = true
      try {
        this.processes = await invoke('list_processes')
      } catch (e) {
        console.error('Failed to list processes:', e)
        this.processes = []
      } finally {
        this.loading = false
      }
    },

    async createSnapshot(pid) {
      this.loading = true
      try {
        const result = await invoke('create_snapshot', { pid })
        if (this.currentSnapshot === null) {
          this.currentSnapshot = result
        }
        await this.refreshSnapshots()
        return result
      } finally {
        this.loading = false
      }
    },

    async refreshSnapshots() {
      try {
        this.snapshots = await invoke('list_snapshots')
      } catch (e) {
        console.error('Failed to list snapshots:', e)
        this.snapshots = []
      }
    },

    async loadSnapshot(id) {
      this.loading = true
      try {
        this.currentSnapshot = await invoke('get_snapshot', { id })
        this.memoryRegions = await invoke('get_memory_regions', { snapshotId: id })
        return this.currentSnapshot
      } finally {
        this.loading = false
      }
    },

    async deleteSnapshot(id) {
      try {
        await invoke('delete_snapshot', { id })
        await this.refreshSnapshots()
        if (this.currentSnapshot?.id === id) {
          this.currentSnapshot = null
          this.memoryRegions = []
          this.memoryData = null
        }
      } catch (e) {
        console.error('Failed to delete snapshot:', e)
      }
    },

    async readMemory(snapshotId, address, length) {
      try {
        this.memoryData = await invoke('read_memory_region', { snapshotId, address, length })
        return this.memoryData
      } catch (e) {
        console.error('Failed to read memory:', e)
        return null
      }
    },

    async scanPattern(snapshotId, pattern) {
      this.loading = true
      try {
        this.scanResults = await invoke('scan_memory_pattern', { snapshotId, pattern })
        return this.scanResults
      } catch (e) {
        console.error('Failed to scan pattern:', e)
        this.scanResults = []
        return []
      } finally {
        this.loading = false
      }
    },

    async checkPrivilege(pid) {
      try {
        return await invoke('check_privilege', { pid })
      } catch (e) {
        console.error('Failed to check privilege:', e)
        return null
      }
    }
  }
})
