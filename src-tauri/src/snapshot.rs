use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use chrono::Utc;
use parking_lot::Mutex;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::db::Database;
use crate::win32;
use crate::models::{
    SnapshotInfo, MemoryRegion, MemoryReadResult, RegionType, ScanProgress,
    DiffResult, DiffRegion, DiffByte, DiffChangeType,
    MonitorLogEntry, MonitorStatus, MonitorCycleEvent,
};

pub type ProgressCallback = dyn Fn(ScanProgress) + Send + Sync;

pub type MonitorCycleCallback = dyn Fn(MonitorCycleEvent) + Send + Sync;

struct MonitorShared {
    running: Arc<AtomicBool>,
    pid: Mutex<Option<u32>>,
    process_name: Mutex<Option<String>>,
    started_at: Mutex<Option<i64>>,
    current_cycle: Mutex<usize>,
    interval_ms: Mutex<u64>,
    total_changes: Mutex<usize>,
    logs: Mutex<Vec<MonitorLogEntry>>,
    last_error: Mutex<Option<String>>,
}

struct MonitorState {
    shared: Arc<MonitorShared>,
    handle: Option<JoinHandle<()>>,
}

pub struct SnapshotManager {
    pub db: Database,
    pub data_dir: PathBuf,
    monitor: Mutex<MonitorState>,
}

impl SnapshotManager {
    pub fn new() -> Result<Self> {
        let db = Database::new()?;
        let data_dir = if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata).join("MemSnapAnalyzer").join("data")
        } else {
            let exe = std::env::current_exe()?;
            exe.parent().unwrap_or(&exe).to_path_buf().join("data")
        };
        fs::create_dir_all(&data_dir)?;
        let shared = Arc::new(MonitorShared {
            running: Arc::new(AtomicBool::new(false)),
            pid: Mutex::new(None),
            process_name: Mutex::new(None),
            started_at: Mutex::new(None),
            current_cycle: Mutex::new(0),
            interval_ms: Mutex::new(1000),
            total_changes: Mutex::new(0),
            logs: Mutex::new(Vec::new()),
            last_error: Mutex::new(None),
        });
        Ok(Self {
            db,
            data_dir,
            monitor: Mutex::new(MonitorState { shared, handle: None }),
        })
    }

    pub fn create_snapshot(&self, pid: u32) -> Result<SnapshotInfo> {
        let now = Utc::now().timestamp_millis();

        let h = win32::open_process(pid)
            .map_err(|e| anyhow!(e))?;

        let process_info = win32::list_processes()?
            .into_iter()
            .find(|p| p.pid == pid)
            .ok_or_else(|| anyhow!("找不到进程 PID={}", pid))?;

        let snapshot_id = self.db.create_snapshot(
            pid,
            &process_info.name,
            &process_info.path,
            now,
        )?;

        let regions = win32::get_memory_regions(h.0, snapshot_id)
            .map_err(|e| anyhow!("获取内存区域失败: {}", e))?;

        let data_file = format!("snapshot_{}.bin", snapshot_id);
        let data_path = self.data_dir.join(&data_file);
        let mut data_file_handle = File::create(&data_path)
            .with_context(|| format!("创建数据文件失败: {}", data_path.display()))?;

        let mut total_bytes: i64 = 0;
        let mut region_count: i64 = 0;
        let mut file_offset: i64 = 0;
        let mut failed_regions = 0i64;
        let mut skipped_no_access = 0i64;

        for region in &regions {
            if !region.is_readable {
                skipped_no_access += 1;
                self.db.insert_region(region, None, 0, 0)?;
                region_count += 1;
                continue;
            }

            let base_addr = u64::from_str_radix(&region.base_address, 16).unwrap_or(0);
            let rsize = region.region_size as usize;
            let (stored_data_path, stored_offset, stored_length) = if region.state == "Commit" && region.is_readable {
                match win32::try_read_memory_safe(h.0, base_addr, rsize.min(128 * 1024 * 1024)) {
                    Ok(data) if !data.is_empty() => {
                        data_file_handle.write_all(&data)?;
                        let len = data.len() as i64;
                        let off = file_offset;
                        file_offset += len;
                        total_bytes += len;
                        (Some(data_file.clone()), off, len)
                    }
                    Err(_) => {
                        failed_regions += 1;
                        (None, 0, 0)
                    }
                    _ => (None, 0, 0),
                }
            } else {
                (None, 0, 0)
            };

            self.db.insert_region(
                region,
                stored_data_path.as_deref(),
                stored_offset,
                stored_length,
            )?;
            region_count += 1;
        }

        drop(data_file_handle);

        self.db.update_snapshot_stats(snapshot_id, total_bytes, region_count)?;

        let snap = self.db.get_snapshot(snapshot_id)?
            .ok_or_else(|| anyhow!("快照创建后无法读取"))?;

        Ok(snap)
    }

    pub fn check_privilege(&self, pid: Option<u32>) -> crate::models::PrivilegeCheckResult {
        let is_admin = win32::is_admin();
        let (can_open, suggested) = match pid {
            Some(pid) => {
                let (ok, msg) = win32::probe_process_access(pid);
                (ok, if !ok && !is_admin {
                    "请以管理员身份运行此程序以获取完整访问权限".to_string()
                } else if !ok {
                    msg
                } else {
                    "权限充足".to_string()
                })
            }
            None => {
                (false, if !is_admin {
                    "请以管理员身份运行以访问系统进程".to_string()
                } else {
                    "已以管理员权限运行".to_string()
                })
            }
        };
        crate::models::PrivilegeCheckResult {
            is_admin,
            can_open_process: can_open,
            suggested_action: suggested,
        }
    }

    pub fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>> {
        Ok(self.db.list_snapshots()?)
    }

    pub fn get_snapshot(&self, id: i64) -> Result<Option<SnapshotInfo>> {
        Ok(self.db.get_snapshot(id)?)
    }

    pub fn delete_snapshot(&self, id: i64) -> Result<()> {
        Ok(self.db.delete_snapshot(id, &self.data_dir)?)
    }

    pub fn get_regions(&self, snapshot_id: i64) -> Result<Vec<MemoryRegion>> {
        Ok(self.db.get_regions(snapshot_id)?)
    }

    pub fn read_memory_region(
        &self,
        snapshot_id: i64,
        address_hex: &str,
        length: usize,
    ) -> Result<MemoryReadResult> {
        let address = parse_address(address_hex)?;
        if length == 0 {
            return Err(anyhow!("读取长度不能为0"));
        }
        if length > 16 * 1024 * 1024 {
            return Err(anyhow!("单次读取不能超过16MB"));
        }

        let result = self.db.find_region_containing(snapshot_id, address)?
            .ok_or_else(|| anyhow!("地址 0x{:X} 不在任何已存储区域中", address))?;

        let (_id, dp, d_off, d_len) = self.db.get_region_data_info(result.id)?;
        let region_base = u64::from_str_radix(&result.base_address, 16).unwrap_or(0);

        let offset_in_region = (address - region_base) as i64;
        let data_path = dp.ok_or_else(|| anyhow!("该内存区域未存储快照数据（可能不可读）"))?;
        let full_path = self.data_dir.join(&data_path);

        if offset_in_region >= d_len {
            return Err(anyhow!("偏移超出已存储数据范围"));
        }

        let available = (d_len - offset_in_region) as usize;
        let actual_len = length.min(available);
        if actual_len == 0 {
            return Err(anyhow!("无可读取的数据"));
        }

        let mut f = File::open(&full_path)
            .with_context(|| format!("打开数据文件失败: {}", full_path.display()))?;
        f.seek(SeekFrom::Start((d_off + offset_in_region) as u64))?;

        let mut buf = vec![0u8; actual_len];
        let mut bytes_read_total = 0usize;
        while bytes_read_total < actual_len {
            let n = f.read(&mut buf[bytes_read_total..])?;
            if n == 0 { break; }
            bytes_read_total += n;
        }
        buf.truncate(bytes_read_total);

        Ok(MemoryReadResult {
            base_address: format!("{:X}", address),
            length: buf.len(),
            data: buf,
            highlights: Vec::new(),
            highlight_length: 0,
        })
    }

    pub fn scan_pattern<F>(
        &self,
        snapshot_id: i64,
        pattern_str: &str,
        progress_cb: Option<F>,
    ) -> Result<crate::models::ScanResult>
    where
        F: Fn(ScanProgress),
    {
        use std::time::Instant;
        let start = Instant::now();

        let pattern = parse_pattern(pattern_str)?;
        if pattern.is_empty() {
            return Err(anyhow!("模式为空"));
        }

        let regions = self.db.get_regions(snapshot_id)?;

        let scannable_regions: Vec<_> = regions.iter()
            .filter(|r| r.is_readable && r.has_data)
            .collect();

        let total_regions = scannable_regions.len();
        let mut matches = Vec::new();
        let mut regions_scanned = 0usize;
        let mut bytes_scanned: u64 = 0;
        let mut regions_skipped_no_access = 0usize;
        let mut regions_skipped_guard = 0usize;

        for (idx, region) in regions.iter().enumerate() {
            if !region.is_readable {
                if region.protection.contains("GUARD") {
                    regions_skipped_guard += 1;
                } else {
                    regions_skipped_no_access += 1;
                }
                continue;
            }
            if !region.has_data {
                continue;
            }

            let (_id, dp, d_off, d_len) = self.db.get_region_data_info(region.id)?;
            let data_path = match dp {
                Some(p) => p,
                None => continue,
            };
            let full_path = self.data_dir.join(&data_path);

            let region_base = u64::from_str_radix(&region.base_address, 16).unwrap_or(0);

            if let Some(ref cb) = progress_cb {
                let progress = if total_regions > 0 {
                    ((idx as f64 / total_regions as f64) * 100.0) as u32
                } else { 0 };
                cb(ScanProgress {
                    current: idx,
                    total: total_regions,
                    percent: progress.min(99),
                    current_region: region.module_name.clone().or(region.details.clone()).or_else(|| format!("0x{}", region.base_address)),
                    bytes_scanned,
                    matches_found: matches.len(),
                });
            }

            match read_region_data(&full_path, d_off, d_len) {
                Ok(data) if !data.is_empty() => {
                    regions_scanned += 1;
                    bytes_scanned += data.len() as u64;

                    for m in search_pattern(&data, &pattern) {
                        let match_addr = region_base + m as u64;
                        let start_ctx = m.saturating_sub(4);
                        let end_ctx = (m + pattern.len() + 12).min(data.len());
                        let ctx = &data[start_ctx..end_ctx];
                        let ctx_hex = ctx.iter()
                            .map(|b| format!("{:02X}", b))
                            .collect::<Vec<_>>()
                            .join(" ");

                        matches.push(crate::models::ScanMatch {
                            address: format!("{:X}", match_addr),
                            region_type: region.region_type.clone(),
                            module_name: region.module_name.clone(),
                            offset_in_region: format!("{:X}", m),
                            context_hex: ctx_hex,
                        });

                        if matches.len() >= 5000 {
                            break;
                        }
                    }
                }
                _ => continue,
            }

            if matches.len() >= 5000 {
                break;
            }
        }

        if let Some(ref cb) = progress_cb {
            cb(ScanProgress {
                current: total_regions,
                total: total_regions,
                percent: 100,
                current_region: "扫描完成".to_string(),
                bytes_scanned,
                matches_found: matches.len(),
            });
        }

        let total = matches.len();
        Ok(crate::models::ScanResult {
            matches,
            total_matches: total,
            regions_scanned,
            bytes_scanned,
            elapsed_ms: start.elapsed().as_millis(),
            regions_skipped_no_access,
            regions_skipped_guard,
        })
    }

    pub fn compare_snapshots(&self, snapshot_a_id: i64, snapshot_b_id: i64) -> Result<DiffResult> {
        use std::time::Instant;
        let start = Instant::now();

        let snap_a = self.db.get_snapshot(snapshot_a_id)?
            .ok_or_else(|| anyhow!("快照 A 不存在"))?;
        let snap_b = self.db.get_snapshot(snapshot_b_id)?
            .ok_or_else(|| anyhow!("快照 B 不存在"))?;

        if snap_a.pid != snap_b.pid {
            return Err(anyhow!("两个快照属于不同进程，无法对比：A(PID={}), B(PID={})",
                snap_a.pid, snap_b.pid));
        }

        let regions_a = self.db.get_regions(snapshot_a_id)?;
        let regions_b = self.db.get_regions(snapshot_b_id)?;

        let map_a: std::collections::HashMap<String, &MemoryRegion> =
            regions_a.iter().map(|r| (r.base_address.clone(), r)).collect();
        let map_b: std::collections::HashMap<String, &MemoryRegion> =
            regions_b.iter().map(|r| (r.base_address.clone(), r)).collect();

        let mut diff_regions: Vec<DiffRegion> = Vec::new();
        let mut regions_compared = 0usize;
        let mut regions_modified = 0usize;
        let mut regions_added = 0usize;
        let mut regions_removed = 0usize;
        let mut total_changed_bytes = 0usize;

        for (base_addr, region_b) in &map_b {
            if let Some(region_a) = map_a.get(base_addr) {
                regions_compared += 1;

                let has_data_a = region_a.has_data && region_a.is_readable;
                let has_data_b = region_b.has_data && region_b.is_readable;

                if !has_data_a && !has_data_b {
                    continue;
                }

                let data_a = if has_data_a {
                    self.load_region_data(region_a).ok()
                } else { None };
                let data_b = if has_data_b {
                    self.load_region_data(region_b).ok()
                } else { None };

                match (data_a, data_b) {
                    (Some(a), Some(b)) => {
                        let changed = diff_bytes(&a, &b);
                        if !changed.is_empty() {
                            let region_base =
                                u64::from_str_radix(&region_b.base_address, 16).unwrap_or(0);
                            let diff_bytes_list: Vec<DiffByte> = changed.iter().map(|&(off, old, new)| {
                                DiffByte {
                                    offset_in_region: off as u64,
                                    absolute_address: format!("{:X}", region_base + off as u64),
                                    old_value: Some(old),
                                    new_value: Some(new),
                                }
                            }).collect();
                            let size = region_b.region_size.max(1) as f64;
                            let change_count = diff_bytes_list.len();
                            diff_regions.push(DiffRegion {
                                base_address: region_b.base_address.clone(),
                                region_size: region_b.region_size as u64,
                                region_type: region_b.region_type.clone(),
                                module_name: region_b.module_name.clone(),
                                change_type: DiffChangeType::Modified,
                                changed_bytes: diff_bytes_list,
                                change_count,
                                change_percent: (change_count as f64 / size) * 100.0,
                            });
                            regions_modified += 1;
                            total_changed_bytes += change_count;
                        }
                    }
                    (None, Some(b)) => {
                        let region_base =
                            u64::from_str_radix(&region_b.base_address, 16).unwrap_or(0);
                        let diff_bytes_list: Vec<DiffByte> = b.iter().enumerate().map(|(off, &val)| DiffByte {
                            offset_in_region: off as u64,
                            absolute_address: format!("{:X}", region_base + off as u64),
                            old_value: None,
                            new_value: Some(val),
                        }).collect();
                        let change_count = diff_bytes_list.len();
                        let size = region_b.region_size.max(1) as f64;
                        diff_regions.push(DiffRegion {
                            base_address: region_b.base_address.clone(),
                            region_size: region_b.region_size as u64,
                            region_type: region_b.region_type.clone(),
                            module_name: region_b.module_name.clone(),
                            change_type: DiffChangeType::Added,
                            changed_bytes: diff_bytes_list,
                            change_count,
                            change_percent: (change_count as f64 / size) * 100.0,
                        });
                        regions_added += 1;
                        total_changed_bytes += change_count;
                    }
                    (Some(a), None) => {
                        let region_base =
                            u64::from_str_radix(&region_a.base_address, 16).unwrap_or(0);
                        let diff_bytes_list: Vec<DiffByte> = a.iter().enumerate().map(|(off, &val)| DiffByte {
                            offset_in_region: off as u64,
                            absolute_address: format!("{:X}", region_base + off as u64),
                            old_value: Some(val),
                            new_value: None,
                        }).collect();
                        let change_count = diff_bytes_list.len();
                        let size = region_a.region_size.max(1) as f64;
                        diff_regions.push(DiffRegion {
                            base_address: region_a.base_address.clone(),
                            region_size: region_a.region_size as u64,
                            region_type: region_a.region_type.clone(),
                            module_name: region_a.module_name.clone(),
                            change_type: DiffChangeType::Removed,
                            changed_bytes: diff_bytes_list,
                            change_count,
                            change_percent: (change_count as f64 / size) * 100.0,
                        });
                        regions_removed += 1;
                        total_changed_bytes += change_count;
                    }
                    _ => {}
                }
            } else {
                let has_data = region_b.has_data && region_b.is_readable;
                let region_base =
                    u64::from_str_radix(&region_b.base_address, 16).unwrap_or(0);

                let (changed_bytes_list, count) = if has_data {
                    match self.load_region_data(region_b) {
                        Ok(data) => {
                            let list: Vec<DiffByte> = data.iter().enumerate()
                                .map(|(off, &val)| DiffByte {
                                    offset_in_region: off as u64,
                                    absolute_address: format!("{:X}", region_base + off as u64),
                                    old_value: None,
                                    new_value: Some(val),
                                }).collect();
                            let c = list.len();
                            (list, c)
                        }
                        _ => (Vec::new(), 0),
                    }
                } else { (Vec::new(), 0) };

                let size = region_b.region_size.max(1) as f64;
                diff_regions.push(DiffRegion {
                    base_address: region_b.base_address.clone(),
                    region_size: region_b.region_size as u64,
                    region_type: region_b.region_type.clone(),
                    module_name: region_b.module_name.clone(),
                    change_type: DiffChangeType::Added,
                    changed_bytes: changed_bytes_list,
                    change_count: count,
                    change_percent: (count as f64 / size) * 100.0,
                });
                regions_added += 1;
                total_changed_bytes += count;
            }
        }

        for (base_addr, region_a) in &map_a {
            if !map_b.contains_key(base_addr) {
                let has_data = region_a.has_data && region_a.is_readable;
                let region_base =
                    u64::from_str_radix(&region_a.base_address, 16).unwrap_or(0);

                let (changed_bytes_list, count) = if has_data {
                    match self.load_region_data(region_a) {
                        Ok(data) => {
                            let list: Vec<DiffByte> = data.iter().enumerate()
                                .map(|(off, &val)| DiffByte {
                                    offset_in_region: off as u64,
                                    absolute_address: format!("{:X}", region_base + off as u64),
                                    old_value: Some(val),
                                    new_value: None,
                                }).collect();
                            let c = list.len();
                            (list, c)
                        }
                        _ => (Vec::new(), 0),
                    }
                } else { (Vec::new(), 0) };

                let size = region_a.region_size.max(1) as f64;
                diff_regions.push(DiffRegion {
                    base_address: region_a.base_address.clone(),
                    region_size: region_a.region_size as u64,
                    region_type: region_a.region_type.clone(),
                    module_name: region_a.module_name.clone(),
                    change_type: DiffChangeType::Removed,
                    changed_bytes: changed_bytes_list,
                    change_count: count,
                    change_percent: (count as f64 / size) * 100.0,
                });
                regions_removed += 1;
                total_changed_bytes += count;
            }
        }

        diff_regions.sort_by(|a, b| b.change_count.cmp(&a.change_count));

        Ok(DiffResult {
            snapshot_a_id,
            snapshot_b_id,
            process_name: snap_a.process_name.clone(),
            elapsed_ms: start.elapsed().as_millis(),
            regions_compared,
            regions_modified,
            regions_added,
            regions_removed,
            total_changed_bytes,
            diff_regions,
        })
    }

    fn load_region_data(&self, region: &MemoryRegion) -> Result<Vec<u8>> {
        if !region.has_data {
            return Ok(Vec::new());
        }
        let (_id, dp, d_off, d_len) = self.db.get_region_data_info(region.id)?;
        let data_path = dp.ok_or_else(|| anyhow!("无数据路径"))?;
        let full_path = self.data_dir.join(&data_path);
        read_region_data(&full_path, d_off, d_len)
    }

    pub fn start_monitor<F>(&self, pid: u32, interval_ms: u64, cycle_cb: Option<F>) -> Result<MonitorStatus>
    where
        F: Fn(MonitorCycleEvent) + Send + Sync + 'static,
    {
        let mut mon = self.monitor.lock();

        if mon.shared.running.load(Ordering::SeqCst) {
            return Err(anyhow!("已有监控任务正在运行"));
        }

        let process_info = win32::list_processes()?
            .into_iter()
            .find(|p| p.pid == pid)
            .ok_or_else(|| anyhow!("找不到进程 PID={}", pid))?;

        if win32::is_protected_system_process(&process_info.name) {
            return Err(anyhow!("进程 {} 是系统保护进程，无法监控", process_info.name));
        }

        let h = win32::open_process(pid).map_err(|e| anyhow!(e))?;

        *mon.shared.pid.lock() = Some(pid);
        *mon.shared.process_name.lock() = Some(process_info.name.clone());
        *mon.shared.started_at.lock() = Some(Utc::now().timestamp_millis());
        *mon.shared.current_cycle.lock() = 0;
        *mon.shared.interval_ms.lock() = interval_ms.max(200);
        *mon.shared.total_changes.lock() = 0;
        mon.shared.logs.lock().clear();
        *mon.shared.last_error.lock() = None;
        mon.shared.running.store(true, Ordering::SeqCst);

        let shared = mon.shared.clone();
        let interval = *shared.interval_ms.lock();
        let cycle_cb_arc: Arc<Mutex<Option<Box<dyn Fn(MonitorCycleEvent) + Send + Sync + 'static>>>> =
            Arc::new(Mutex::new(cycle_cb.map(|cb| Box::new(cb) as Box<dyn Fn(MonitorCycleEvent) + Send + Sync>)));

        let handle = thread::spawn(move || {
            let mut prev_snapshot: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
            let mut prev_meta: std::collections::HashMap<String, (RegionType, String)> = std::collections::HashMap::new();
            let mut local_cycle: usize = 0;

            loop {
                if !shared.running.load(Ordering::SeqCst) {
                    break;
                }

                let cycle_start = std::time::Instant::now();
                local_cycle += 1;
                *shared.current_cycle.lock() = local_cycle;
                let timestamp = Utc::now().timestamp_millis();

                let cycle_regions = match win32::get_memory_regions(h.0, 0) {
                    Ok(r) => r,
                    Err(e) => {
                        *shared.last_error.lock() = Some(format!("获取内存区域失败: {}", e));
                        std::thread::sleep(Duration::from_millis(interval));
                        continue;
                    }
                };

                let mut current_snapshot: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
                let mut current_meta: std::collections::HashMap<String, (RegionType, String)> = std::collections::HashMap::new();
                let mut cycle_changes = 0usize;

                for region in &cycle_regions {
                    if !region.is_readable || region.state != "Commit" {
                        continue;
                    }
                    let rsize = region.region_size as usize;
                    if rsize == 0 { continue; }
                    let max_read = rsize.min(4 * 1024 * 1024);
                    let base_addr = u64::from_str_radix(&region.base_address, 16).unwrap_or(0);

                    let data = match win32::try_read_memory_safe(h.0, base_addr, max_read) {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    if data.is_empty() { continue; }

                    let key = region.base_address.clone();
                    current_meta.insert(key.clone(),
                        (region.region_type.clone(), region.module_name.clone()));

                    if let Some(old) = prev_snapshot.get(&key) {
                        let diffs = diff_bytes(old, &data);
                        if !diffs.is_empty() {
                            let (rtype, mname) = current_meta.get(&key)
                                .cloned().unwrap_or((RegionType::Other, String::new()));
                            let mut log_lock = shared.logs.lock();
                            for (off, oldv, newv) in &diffs {
                                log_lock.push(MonitorLogEntry {
                                    timestamp,
                                    cycle_index: local_cycle,
                                    base_address: key.clone(),
                                    region_type: rtype.clone(),
                                    module_name: mname.clone(),
                                    change_type: DiffChangeType::Modified,
                                    offset_in_region: *off as u64,
                                    absolute_address: format!("{:X}", base_addr + *off as u64),
                                    old_value: Some(*oldv),
                                    new_value: Some(*newv),
                                });
                            }
                            let dlen = diffs.len();
                            let mut total = shared.total_changes.lock();
                            *total += dlen;
                            cycle_changes += dlen;

                            if log_lock.len() > 100_000 {
                                let drain = log_lock.len() - 50_000;
                                log_lock.drain(0..drain);
                            }
                            drop(total);
                            drop(log_lock);
                        }
                    } else {
                        let (rtype, mname) = current_meta.get(&key)
                            .cloned().unwrap_or((RegionType::Other, String::new()));
                        let mut log_lock = shared.logs.lock();
                        let cap = data.len().min(64);
                        for (off, &val) in data.iter().take(cap).enumerate() {
                            log_lock.push(MonitorLogEntry {
                                timestamp,
                                cycle_index: local_cycle,
                                base_address: key.clone(),
                                region_type: rtype.clone(),
                                module_name: mname.clone(),
                                change_type: DiffChangeType::Added,
                                offset_in_region: off as u64,
                                absolute_address: format!("{:X}", base_addr + off as u64),
                                old_value: None,
                                new_value: Some(val),
                            });
                        }
                        let nadd = cap;
                        let mut total = shared.total_changes.lock();
                        *total += nadd;
                        cycle_changes += nadd;
                        drop(total);
                        drop(log_lock);
                    }

                    current_snapshot.insert(key, data);
                }

                for (key, old_data) in &prev_snapshot {
                    if !current_snapshot.contains_key(key) {
                        let (rtype, mname) = prev_meta.get(key)
                            .cloned().unwrap_or((RegionType::Other, String::new()));
                        let base_addr = u64::from_str_radix(key, 16).unwrap_or(0);
                        let mut log_lock = shared.logs.lock();
                        let cap = old_data.len().min(64);
                        for (off, &val) in old_data.iter().take(cap).enumerate() {
                            log_lock.push(MonitorLogEntry {
                                timestamp,
                                cycle_index: local_cycle,
                                base_address: key.clone(),
                                region_type: rtype.clone(),
                                module_name: mname.clone(),
                                change_type: DiffChangeType::Removed,
                                offset_in_region: off as u64,
                                absolute_address: format!("{:X}", base_addr + off as u64),
                                old_value: Some(val),
                                new_value: None,
                            });
                        }
                        let nrem = cap;
                        let mut total = shared.total_changes.lock();
                        *total += nrem;
                        cycle_changes += nrem;
                    }
                }

                prev_snapshot = current_snapshot;
                prev_meta = current_meta;

                let duration = cycle_start.elapsed().as_millis() as u64;
                let total_so_far = *shared.total_changes.lock();

                if let Some(cb) = cycle_cb_arc.lock().as_ref() {
                    cb(MonitorCycleEvent {
                        cycle_index: local_cycle,
                        timestamp,
                        duration_ms: duration,
                        changes_found: cycle_changes,
                        total_changes_so_far: total_so_far,
                    });
                }

                let sleep_ms = if interval > duration { interval - duration } else { 100 };
                std::thread::sleep(Duration::from_millis(sleep_ms));
            }

            drop(h);
        });

        mon.handle = Some(handle);
        drop(mon);
        Ok(self.get_monitor_status())
    }

    pub fn stop_monitor(&self) -> MonitorStatus {
        let mut mon = self.monitor.lock();
        mon.shared.running.store(false, Ordering::SeqCst);
        if let Some(h) = mon.handle.take() {
            drop(h);
        }
        drop(mon);
        self.get_monitor_status()
    }

    pub fn get_monitor_status(&self) -> MonitorStatus {
        let mon = self.monitor.lock();
        let s = &mon.shared;
        MonitorStatus {
            is_running: s.running.load(Ordering::SeqCst),
            pid: *s.pid.lock(),
            process_name: s.process_name.lock().clone(),
            started_at: *s.started_at.lock(),
            current_cycle: *s.current_cycle.lock(),
            interval_ms: *s.interval_ms.lock(),
            total_changes: *s.total_changes.lock(),
            log_entry_count: s.logs.lock().len(),
            last_error: s.last_error.lock().clone(),
        }
    }

    pub fn get_monitor_logs(&self, limit: Option<usize>) -> Vec<MonitorLogEntry> {
        let mon = self.monitor.lock();
        let logs = mon.shared.logs.lock();
        if let Some(n) = limit {
            let start = logs.len().saturating_sub(n);
            logs[start..].to_vec()
        } else {
            logs.clone()
        }
    }
}

fn parse_address(s: &str) -> Result<u64> {
    let trimmed = s.trim();
    let without_prefix = trimmed.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(without_prefix, 16)
        .map_err(|e| anyhow!("无效地址 '{}': {}", s, e))
}

fn parse_pattern(s: &str) -> Result<Vec<Option<u8>>> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.is_empty() {
        return Err(anyhow!("请输入十六进制特征码"));
    }
    let mut result = Vec::with_capacity(tokens.len());
    for tok in tokens {
        let t = tok.trim();
        if t.is_empty() { continue; }
        if t == "??" || t == "**" || t == ".." || t == "__" {
            result.push(None);
        } else {
            let bytes = parse_hex_token(t)?;
            result.extend(bytes);
        }
    }
    Ok(result)
}

fn parse_hex_token(t: &str) -> Result<Vec<Option<u8>>> {
    let mut res = Vec::new();
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c1 = chars[i];
        if c1 == '{' && i + 2 < chars.len() {
            let end = chars[i..].iter().position(|&c| c == '}');
            if let Some(end_idx) = end {
                let num_str: String = chars[i + 1..i + end_idx].iter().collect();
                if let Ok(n) = num_str.parse::<usize>() {
                    for _ in 0..n {
                        res.push(None);
                    }
                    i += end_idx + 1;
                    continue;
                }
            }
        }
        if i + 1 < chars.len() {
            let byte_str: String = [chars[i], chars[i + 1]].iter().collect();
            let b = u8::from_str_radix(&byte_str, 16)
                .map_err(|e| anyhow!("无效字节 '{}': {}", byte_str, e))?;
            res.push(Some(b));
            i += 2;
        } else {
            return Err(anyhow!("不完整的十六进制字符: {}", t));
        }
    }
    Ok(res)
}

fn read_region_data<P: AsRef<Path>>(
    path: P,
    offset: i64,
    length: i64,
) -> Result<Vec<u8>> {
    if length <= 0 { return Ok(Vec::new()); }
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(offset as u64))?;
    let mut buf = vec![0u8; length as usize];
    let mut total = 0;
    while total < buf.len() {
        let n = f.read(&mut buf[total..])?;
        if n == 0 { break; }
        total += n;
    }
    buf.truncate(total);
    Ok(buf)
}

fn search_pattern(data: &[u8], pattern: &[Option<u8>]) -> Vec<usize> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return Vec::new();
    }
    let mut results = Vec::new();
    let first_fixed = pattern.iter().position(|p| p.is_some());
    match first_fixed {
        Some(ff) => {
            let first_byte = pattern[ff].unwrap();
            let mut i = 0;
            while i + pattern.len() <= data.len() {
                let start = match data[i..].iter().position(|&b| b == first_byte) {
                    Some(p) => i + p,
                    None => break,
                };
                let check_start = if start >= ff { start - ff } else { start };
                if check_start + pattern.len() <= data.len() {
                    if match_at(data, pattern, check_start) {
                        results.push(check_start);
                    }
                }
                i = start + 1;
            }
        }
        None => {
            for i in 0..=(data.len() - pattern.len()) {
                results.push(i);
                if results.len() >= 5000 { break; }
            }
        }
    }
    results
}

#[inline]
fn match_at(data: &[u8], pattern: &[Option<u8>], pos: usize) -> bool {
    if pos + pattern.len() > data.len() { return false; }
    for (i, p) in pattern.iter().enumerate() {
        match p {
            Some(expected) if data[pos + i] != *expected => return false,
            _ => continue,
        }
    }
    true
}

fn diff_bytes(a: &[u8], b: &[u8]) -> Vec<(usize, u8, u8)> {
    let min_len = a.len().min(b.len());
    let mut result = Vec::new();
    let max_diff = 65536;

    for i in 0..min_len {
        if a[i] != b[i] {
            result.push((i, a[i], b[i]));
            if result.len() >= max_diff {
                break;
            }
        }
    }
    result
}
