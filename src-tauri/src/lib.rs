#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::{State, Window};
use serde::{Deserialize, Serialize};

pub mod models;
pub mod win32;
pub mod db;
pub mod snapshot;

use snapshot::SnapshotManager;
use models::*;

struct AppState {
    manager: Arc<SnapshotManager>,
}

#[derive(Serialize, Deserialize)]
struct ListProcessesResult(Vec<ProcessInfo>);

#[tauri::command]
fn list_processes(state: State<'_, AppState>) -> Result<Vec<ProcessInfo>, String> {
    let _ = &state;
    win32::list_processes().map_err(|e| e.to_string())
}

#[tauri::command]
fn check_privilege(pid: Option<u32>, state: State<'_, AppState>) -> PrivilegeCheckResult {
    state.manager.check_privilege(pid)
}

#[tauri::command]
fn create_snapshot(pid: u32, state: State<'_, AppState>) -> Result<SnapshotInfo, String> {
    state.manager.create_snapshot(pid).map_err(|e| format!("{}", e))
}

#[tauri::command]
fn list_snapshots(state: State<'_, AppState>) -> Result<Vec<SnapshotInfo>, String> {
    state.manager.list_snapshots().map_err(|e| format!("获取快照列表失败: {}", e))
}

#[tauri::command]
fn get_snapshot(id: i64, state: State<'_, AppState>) -> Result<Option<SnapshotInfo>, String> {
    state.manager.get_snapshot(id).map_err(|e| format!("获取快照失败: {}", e))
}

#[tauri::command]
fn delete_snapshot(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.manager.delete_snapshot(id).map_err(|e| format!("删除快照失败: {}", e))
}

#[tauri::command]
fn get_memory_regions(snapshot_id: i64, state: State<'_, AppState>) -> Result<Vec<MemoryRegion>, String> {
    state.manager.get_regions(snapshot_id).map_err(|e| format!("获取内存区域失败: {}", e))
}

#[tauri::command]
fn read_memory_region(
    snapshot_id: i64,
    address: String,
    length: usize,
    state: State<'_, AppState>,
) -> Result<MemoryReadResult, String> {
    state.manager.read_memory_region(snapshot_id, &address, length)
        .map_err(|e| format!("读取内存失败: {}", e))
}

#[tauri::command]
fn scan_memory_pattern(
    snapshot_id: i64,
    pattern: String,
    window: Window,
    state: State<'_, AppState>,
) -> Result<ScanResult, String> {
    let window_arc = std::sync::Arc::new(window);
    let result = state.manager.scan_pattern(
        snapshot_id,
        &pattern,
        Some(move |progress: ScanProgress| {
            let _ = window_arc.emit("scan-progress", &progress);
        }),
    );
    result.map_err(|e| format!("扫描失败: {}", e))
}

#[tauri::command]
fn compare_snapshots(
    snapshot_a_id: i64,
    snapshot_b_id: i64,
    state: State<'_, AppState>,
) -> Result<DiffResult, String> {
    state.manager.compare_snapshots(snapshot_a_id, snapshot_b_id)
        .map_err(|e| format!("对比快照失败: {}", e))
}

#[tauri::command]
fn start_monitor(
    pid: u32,
    interval_ms: Option<u64>,
    window: Window,
    state: State<'_, AppState>,
) -> Result<MonitorStatus, String> {
    let window_arc = std::sync::Arc::new(window);
    let interval = interval_ms.unwrap_or(1000);
    state.manager.start_monitor(
        pid,
        interval,
        Some(move |evt: MonitorCycleEvent| {
            let _ = window_arc.emit("monitor-cycle", &evt);
        }),
    ).map_err(|e| format!("启动监控失败: {}", e))
}

#[tauri::command]
fn stop_monitor(state: State<'_, AppState>) -> MonitorStatus {
    state.manager.stop_monitor()
}

#[tauri::command]
fn get_monitor_status(state: State<'_, AppState>) -> MonitorStatus {
    state.manager.get_monitor_status()
}

#[tauri::command]
fn get_monitor_logs(limit: Option<usize>, state: State<'_, AppState>) -> Vec<MonitorLogEntry> {
    state.manager.get_monitor_logs(limit)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let manager = SnapshotManager::new()
        .expect("初始化快照管理器失败");
    let app_state = AppState {
        manager: Arc::new(manager),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            list_processes,
            check_privilege,
            create_snapshot,
            list_snapshots,
            get_snapshot,
            delete_snapshot,
            get_memory_regions,
            read_memory_region,
            scan_memory_pattern,
            compare_snapshots,
            start_monitor,
            stop_monitor,
            get_monitor_status,
            get_monitor_logs,
        ])
        .run(tauri::generate_context!())
        .expect("启动应用失败");
}
