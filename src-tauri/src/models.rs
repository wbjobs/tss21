use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub path: String,
    pub memory_usage_mb: f64,
    pub thread_count: u32,
    pub parent_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub id: i64,
    pub pid: u32,
    pub process_name: String,
    pub process_path: String,
    pub total_size_bytes: i64,
    pub total_size_mb: f64,
    pub region_count: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RegionType {
    Image,
    Heap,
    Stack,
    Private,
    Mapped,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRegion {
    pub id: i64,
    pub snapshot_id: i64,
    pub region_type: RegionType,
    pub base_address: String,
    pub region_size: i64,
    pub protection: String,
    pub state: String,
    pub type_info: String,
    pub module_name: String,
    pub details: String,
    pub has_data: bool,
    pub is_readable: bool,
    pub is_writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryReadResult {
    pub base_address: String,
    pub length: usize,
    pub data: Vec<u8>,
    pub highlights: Vec<usize>,
    #[serde(rename = "highlight_length")]
    pub highlight_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanMatch {
    pub address: String,
    pub region_type: RegionType,
    pub module_name: String,
    pub offset_in_region: String,
    pub context_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub matches: Vec<ScanMatch>,
    pub total_matches: usize,
    pub regions_scanned: usize,
    pub bytes_scanned: u64,
    pub elapsed_ms: u128,
    pub regions_skipped_no_access: usize,
    pub regions_skipped_guard: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub current: usize,
    pub total: usize,
    pub percent: u32,
    pub current_region: String,
    pub bytes_scanned: u64,
    pub matches_found: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivilegeCheckResult {
    pub is_admin: bool,
    pub can_open_process: bool,
    pub suggested_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DiffChangeType {
    Modified,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffByte {
    pub offset_in_region: u64,
    pub absolute_address: String,
    pub old_value: Option<u8>,
    pub new_value: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffRegion {
    pub base_address: String,
    pub region_size: u64,
    pub region_type: RegionType,
    pub module_name: String,
    pub change_type: DiffChangeType,
    pub changed_bytes: Vec<DiffByte>,
    pub change_count: usize,
    pub change_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    pub snapshot_a_id: i64,
    pub snapshot_b_id: i64,
    pub process_name: String,
    pub elapsed_ms: u128,
    pub regions_compared: usize,
    pub regions_modified: usize,
    pub regions_added: usize,
    pub regions_removed: usize,
    pub total_changed_bytes: usize,
    pub diff_regions: Vec<DiffRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorLogEntry {
    pub timestamp: i64,
    pub cycle_index: usize,
    pub base_address: String,
    pub region_type: RegionType,
    pub module_name: String,
    pub change_type: DiffChangeType,
    pub offset_in_region: u64,
    pub absolute_address: String,
    pub old_value: Option<u8>,
    pub new_value: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorStatus {
    pub is_running: bool,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub started_at: Option<i64>,
    pub current_cycle: usize,
    pub interval_ms: u64,
    pub total_changes: usize,
    pub log_entry_count: usize,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorCycleEvent {
    pub cycle_index: usize,
    pub timestamp: i64,
    pub duration_ms: u64,
    pub changes_found: usize,
    pub total_changes_so_far: usize,
}
