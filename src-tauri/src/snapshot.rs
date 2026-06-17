use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use chrono::Utc;

use crate::db::Database;
use crate::win32;
use crate::models::{SnapshotInfo, MemoryRegion, MemoryReadResult, RegionType};

pub struct SnapshotManager {
    pub db: Database,
    pub data_dir: PathBuf,
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
        Ok(Self { db, data_dir })
    }

    pub fn create_snapshot(&self, pid: u32) -> Result<SnapshotInfo> {
        let now = Utc::now().timestamp_millis();

        let h = win32::open_process(pid)
            .map_err(|e| anyhow!("打开进程失败: {}", e))?;

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

        for region in &regions {
            let base_addr = u64::from_str_radix(&region.base_address, 16).unwrap_or(0);
            let rsize = region.region_size as usize;
            let (stored_data_path, stored_offset, stored_length) = if region.state == "Commit" {
                match win32::read_memory(h.0, base_addr, rsize.min(256 * 1024 * 1024)) {
                    Ok(data) if !data.is_empty() => {
                        data_file_handle.write_all(&data)?;
                        let len = data.len() as i64;
                        let off = file_offset;
                        file_offset += len;
                        total_bytes += len;
                        (Some(data_file.clone()), off, len)
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

        self.db.get_snapshot(snapshot_id)?
            .ok_or_else(|| anyhow!("快照创建后无法读取"))
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
        let data_path = dp.ok_or_else(|| anyhow!("该内存区域未存储快照数据"))?;
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

    pub fn scan_pattern(
        &self,
        snapshot_id: i64,
        pattern_str: &str,
    ) -> Result<crate::models::ScanResult> {
        use std::time::Instant;
        let start = Instant::now();

        let pattern = parse_pattern(pattern_str)?;
        if pattern.is_empty() {
            return Err(anyhow!("模式为空"));
        }

        let regions = self.db.get_regions(snapshot_id)?;
        let mut matches = Vec::new();
        let mut regions_scanned = 0usize;
        let mut bytes_scanned: u64 = 0;

        for region in &regions {
            let (_id, dp, d_off, d_len) = self.db.get_region_data_info(region.id)?;
            let data_path = match dp {
                Some(p) => p,
                None => continue,
            };
            let full_path = self.data_dir.join(&data_path);

            let region_base = u64::from_str_radix(&region.base_address, 16).unwrap_or(0);

            if let Ok(data) = read_region_data(&full_path, d_off, d_len) {
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
        }

        let total = matches.len();
        Ok(crate::models::ScanResult {
            matches,
            total_matches: total,
            regions_scanned,
            bytes_scanned,
            elapsed_ms: start.elapsed().as_millis(),
        })
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
