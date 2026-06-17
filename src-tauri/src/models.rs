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
}
