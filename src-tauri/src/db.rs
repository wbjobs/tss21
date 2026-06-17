use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use parking_lot::Mutex;
use std::sync::Arc;
use std::path::PathBuf;

use crate::models::{SnapshotInfo, MemoryRegion, RegionType};

fn region_type_to_str(t: &RegionType) -> &'static str {
    match t {
        RegionType::Image => "image",
        RegionType::Heap => "heap",
        RegionType::Stack => "stack",
        RegionType::Private => "private",
        RegionType::Mapped => "mapped",
        RegionType::Other => "other",
    }
}

fn str_to_region_type(s: &str) -> RegionType {
    match s {
        "image" => RegionType::Image,
        "heap" => RegionType::Heap,
        "stack" => RegionType::Stack,
        "private" => RegionType::Private,
        "mapped" => RegionType::Mapped,
        _ => RegionType::Other,
    }
}

fn parse_readable_from_protection(prot: &str) -> bool {
    if prot.contains("NOA") || prot.contains("GUARD") {
        return false;
    }
    prot.contains("R--") || prot.contains("RW-") || prot.contains("R-X")
        || prot.contains("RWX") || prot.contains("RWC") || prot.contains("RWXC")
}

fn parse_writable_from_protection(prot: &str) -> bool {
    if prot.contains("NOA") || prot.contains("GUARD") {
        return false;
    }
    prot.contains("RW-") || prot.contains("RWX") || prot.contains("RWC") || prot.contains("RWXC")
}

pub struct Database {
    pub db_path: PathBuf,
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new() -> Result<Self> {
        let db_path = Self::db_path()?;
        Self::open(&db_path)
    }

    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let db_path = path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&db_path)?;
        let db = Self { conn: Arc::new(Mutex::new(conn)), db_path };
        db.init_schema()?;
        Ok(db)
    }

    pub fn dummy() -> Self {
        Self {
            db_path: PathBuf::new(),
            conn: Arc::new(Mutex::new(Connection::open_in_memory().unwrap())),
        }
    }

    fn db_path() -> Result<PathBuf> {
        let base = if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata)
        } else {
            let exe = std::env::current_exe()?;
            exe.parent().unwrap_or(&exe).to_path_buf()
        };
        Ok(base.join("MemSnapAnalyzer").join("snapshots.db"))
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pid INTEGER NOT NULL,
                process_name TEXT NOT NULL,
                process_path TEXT,
                total_size_bytes INTEGER NOT NULL DEFAULT 0,
                region_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memory_regions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                snapshot_id INTEGER NOT NULL,
                region_type TEXT NOT NULL,
                base_address TEXT NOT NULL,
                region_size INTEGER NOT NULL,
                protection TEXT,
                state TEXT,
                type_info TEXT,
                module_name TEXT,
                details TEXT,
                data_path TEXT,
                data_offset INTEGER NOT NULL DEFAULT 0,
                data_length INTEGER NOT NULL DEFAULT 0,
                is_readable INTEGER NOT NULL DEFAULT 1,
                is_writable INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (snapshot_id) REFERENCES snapshots(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_regions_snapshot ON memory_regions(snapshot_id);
            CREATE INDEX IF NOT EXISTS idx_regions_address ON memory_regions(snapshot_id, base_address);
            "#
        )?;

        let _ = conn.execute(
            "ALTER TABLE memory_regions ADD COLUMN is_readable INTEGER NOT NULL DEFAULT 1",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE memory_regions ADD COLUMN is_writable INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(())
    }

    pub fn create_snapshot(
        &self,
        pid: u32,
        process_name: &str,
        process_path: &str,
        created_at: i64,
    ) -> Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO snapshots (pid, process_name, process_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![pid as i64, process_name, process_path, created_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_snapshot_stats(
        &self,
        snapshot_id: i64,
        total_size_bytes: i64,
        region_count: i64,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE snapshots SET total_size_bytes = ?1, region_count = ?2 WHERE id = ?3",
            params![total_size_bytes, region_count, snapshot_id],
        )?;
        Ok(())
    }

    pub fn insert_region(
        &self,
        region: &MemoryRegion,
        data_path: Option<&str>,
        data_offset: i64,
        data_length: i64,
    ) -> Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            r#"INSERT INTO memory_regions
               (snapshot_id, region_type, base_address, region_size, protection, state,
                type_info, module_name, details, data_path, data_offset, data_length,
                is_readable, is_writable)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
            params![
                region.snapshot_id,
                region_type_to_str(&region.region_type),
                region.base_address,
                region.region_size,
                region.protection,
                region.state,
                region.type_info,
                region.module_name,
                region.details,
                data_path,
                data_offset,
                data_length,
                if region.is_readable { 1 } else { 0 },
                if region.is_writable { 1 } else { 0 },
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, pid, process_name, process_path, total_size_bytes, region_count, created_at
             FROM snapshots ORDER BY id DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            let total_bytes: i64 = row.get(4)?;
            Ok(SnapshotInfo {
                id: row.get(0)?,
                pid: row.get::<_, i64>(1)? as u32,
                process_name: row.get(2)?,
                process_path: row.get(3)?,
                total_size_bytes: total_bytes,
                total_size_mb: total_bytes as f64 / (1024.0 * 1024.0),
                region_count: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    pub fn get_snapshot(&self, id: i64) -> Result<Option<SnapshotInfo>> {
        let conn = self.conn.lock();
        let row = conn.query_row(
            "SELECT id, pid, process_name, process_path, total_size_bytes, region_count, created_at
             FROM snapshots WHERE id = ?1",
            params![id],
            |row| {
                let total_bytes: i64 = row.get(4)?;
                Ok(SnapshotInfo {
                    id: row.get(0)?,
                    pid: row.get::<_, i64>(1)? as u32,
                    process_name: row.get(2)?,
                    process_path: row.get(3)?,
                    total_size_bytes: total_bytes,
                    total_size_mb: total_bytes as f64 / (1024.0 * 1024.0),
                    region_count: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        ).optional()?;
        Ok(row)
    }

    pub fn delete_snapshot(&self, id: i64, data_dir: &PathBuf) -> Result<()> {
        let regions = self.get_regions(id)?;
        for r in &regions {
            let (_, data_path, _, _) = self.get_region_data_info(r.id)?;
            if let Some(path) = data_path {
                let _ = std::fs::remove_file(data_dir.join(path));
            }
        }
        let conn = self.conn.lock();
        conn.execute("DELETE FROM memory_regions WHERE snapshot_id = ?1", params![id])?;
        conn.execute("DELETE FROM snapshots WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_regions(&self, snapshot_id: i64) -> Result<Vec<MemoryRegion>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"SELECT r.id, r.snapshot_id, r.region_type, r.base_address, r.region_size,
                      r.protection, r.state, r.type_info, r.module_name, r.details,
                      CASE WHEN r.data_length > 0 THEN 1 ELSE 0 END as has_data,
                      COALESCE(r.is_readable, 1),
                      COALESCE(r.is_writable, 0)
               FROM memory_regions r WHERE r.snapshot_id = ?1
               ORDER BY r.base_address ASC"#
        )?;
        let rows = stmt.query_map(params![snapshot_id], |row| {
            let protection: String = row.get(5).unwrap_or_default();
            let db_readable: i64 = row.get(11).unwrap_or(1);
            let db_writable: i64 = row.get(12).unwrap_or(0);
            let readable = db_readable > 0 || parse_readable_from_protection(&protection);
            let writable = db_writable > 0 || parse_writable_from_protection(&protection);
            Ok(MemoryRegion {
                id: row.get(0)?,
                snapshot_id: row.get(1)?,
                region_type: str_to_region_type(&row.get::<_, String>(2)?),
                base_address: row.get(3)?,
                region_size: row.get(4)?,
                protection,
                state: row.get(6).unwrap_or_default(),
                type_info: row.get(7).unwrap_or_default(),
                module_name: row.get(8).unwrap_or_default(),
                details: row.get(9).unwrap_or_default(),
                has_data: row.get::<_, i64>(10)? > 0,
                is_readable: readable,
                is_writable: writable,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    pub fn get_region_data_info(&self, region_id: i64) -> Result<(i64, Option<String>, i64, i64)> {
        let conn = self.conn.lock();
        let row = conn.query_row(
            "SELECT id, data_path, data_offset, data_length FROM memory_regions WHERE id = ?1",
            params![region_id],
            |row| {
                let dp: Option<String> = row.get(1)?;
                Ok((row.get::<_, i64>(0)?, dp, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
            }
        )?;
        Ok(row)
    }

    pub fn find_region_containing(&self, snapshot_id: i64, addr: u64) -> Result<Option<MemoryRegion>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"SELECT r.id, r.snapshot_id, r.region_type, r.base_address, r.region_size,
                      r.protection, r.state, r.type_info, r.module_name, r.details,
                      CASE WHEN r.data_length > 0 THEN 1 ELSE 0 END as has_data,
                      COALESCE(r.is_readable, 1),
                      COALESCE(r.is_writable, 0)
               FROM memory_regions r
               WHERE r.snapshot_id = ?1
               ORDER BY r.base_address ASC"#
        )?;
        let rows = stmt.query_map(params![snapshot_id], |row| {
            let protection: String = row.get(5).unwrap_or_default();
            let db_readable: i64 = row.get(11).unwrap_or(1);
            let db_writable: i64 = row.get(12).unwrap_or(0);
            let readable = db_readable > 0 || parse_readable_from_protection(&protection);
            let writable = db_writable > 0 || parse_writable_from_protection(&protection);
            Ok(MemoryRegion {
                id: row.get(0)?,
                snapshot_id: row.get(1)?,
                region_type: str_to_region_type(&row.get::<_, String>(2)?),
                base_address: row.get(3)?,
                region_size: row.get(4)?,
                protection,
                state: row.get(6).unwrap_or_default(),
                type_info: row.get(7).unwrap_or_default(),
                module_name: row.get(8).unwrap_or_default(),
                details: row.get(9).unwrap_or_default(),
                has_data: row.get::<_, i64>(10)? > 0,
                is_readable: readable,
                is_writable: writable,
            })
        })?;
        for r in rows {
            let r = r?;
            let base = u64::from_str_radix(&r.base_address, 16).unwrap_or(0);
            if addr >= base && addr < base + r.region_size as u64 {
                return Ok(Some(r));
            }
        }
        Ok(None)
    }
}
