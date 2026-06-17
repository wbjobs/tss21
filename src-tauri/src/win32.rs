use std::ptr;
use std::mem;
use winapi::shared::minwindef::{DWORD, HMODULE, LPVOID, BOOL, TRUE, FALSE, LUID, PLUID, PBOOL};
use winapi::shared::ntdef::{HANDLE, LPCSTR, LPCWSTR, PVOID, ULONG, NTSTATUS};
use winapi::um::tlhelp32::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
    Thread32First, Thread32Next,
    PROCESSENTRY32W, THREADENTRY32,
    TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD,
};
use winapi::um::winbase::{
    OpenProcess,
    MEMORY_BASIC_INFORMATION,
    MEM_COMMIT, MEM_FREE, MEM_RESERVE,
    PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY,
    PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY,
    PAGE_GUARD, PAGE_NOCACHE, PAGE_WRITECOMBINE,
    MEM_IMAGE, MEM_MAPPED, MEM_PRIVATE,
};
use winapi::um::memoryapi::{VirtualQueryEx, ReadProcessMemory};
use winapi::um::processthreadsapi::GetCurrentProcessId;
use winapi::um::handleapi::CloseHandle;
use winapi::um::psapi::{K32EnumProcessModulesEx, K32GetModuleBaseNameW, K32GetModuleFileNameExW, LIST_MODULES_ALL};
use winapi::um::winnt::{
    PROCESS_QUERY_INFORMATION, PROCESS_VM_READ, PROCESS_QUERY_LIMITED_INFORMATION,
    TOKEN_QUERY, TOKEN_ADJUST_PRIVILEGES, SE_PRIVILEGE_ENABLED,
    ANYSIZE_ARRAY, PRIVILEGE_SET,
};
use winapi::um::securitybaseapi::{AdjustTokenPrivileges, PrivilegeCheck, LookupPrivilegeValueW};
use winapi::um::processenv::GetStdHandle;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};

use crate::models::{ProcessInfo, MemoryRegion, RegionType};

const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_PARTIAL_COPY: u32 = 299;

pub struct ProcessHandle(pub HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 as isize != -1 {
            unsafe { CloseHandle(self.0); }
        }
    }
}

unsafe fn wchar_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    let mut cur = ptr;
    while *cur != 0 {
        len += 1;
        cur = cur.add(1);
        if len > 32767 { break; }
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    String::from_utf16_lossy(slice)
}

fn str_to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn get_last_error_msg() -> String {
    let err = unsafe { GetLastError() };
    match err {
        ERROR_ACCESS_DENIED => format!("拒绝访问 (ERROR_ACCESS_DENIED, code 5)"),
        ERROR_INVALID_PARAMETER => format!("无效参数 (ERROR_INVALID_PARAMETER, code 87)"),
        ERROR_PARTIAL_COPY => format!("部分复制成功 (ERROR_PARTIAL_COPY, code 299)"),
        other => format!("系统错误码: {}", other),
    }
}

pub fn is_admin() -> bool {
    let mut h_token: HANDLE = ptr::null_mut();
    let ok = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES,
            &mut h_token
        )
    };
    if ok == FALSE as BOOL {
        return false;
    }
    let _guard = ProcessHandle(h_token);

    let debug_name = str_to_wide("SeDebugPrivilege");
    let mut luid: LUID = unsafe { mem::zeroed() };
    let ok = unsafe {
        LookupPrivilegeValueW(
            ptr::null(),
            debug_name.as_ptr(),
            &mut luid
        )
    };
    if ok == FALSE as BOOL {
        return false;
    }

    let mut privs: PRIVILEGE_SET = unsafe { mem::zeroed() };
    privs.PrivilegeCount = 1;
    privs.Control = PRIVILEGE_SET as u32;
    privs.Privilege[0].Luid = luid;
    privs.Privilege[0].Attributes = SE_PRIVILEGE_ENABLED;

    let mut result: BOOL = FALSE as BOOL;
    let ok = unsafe {
        PrivilegeCheck(
            h_token,
            &mut privs,
            &mut result as PBOOL
        )
    };
    ok != FALSE as BOOL && result != FALSE as BOOL
}

pub fn is_protected_system_process(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        "csrss.exe" | "smss.exe" | "wininit.exe" | "winlogon.exe"
        | "services.exe" | "lsass.exe" | "lsm.exe" | "svchost.exe"
        | "system" | "registry" | "memcompress.exe" | "fontdrvhost.exe"
        | "dwm.exe" | "igfxCUIService.exe" | "nvcontainer.exe"
    )
}

fn protection_to_string(prot: DWORD) -> String {
    let mut parts = Vec::new();
    let base = prot & 0xFF;
    match base {
        PAGE_EXECUTE => parts.push("--X"),
        PAGE_EXECUTE_READ => parts.push("R-X"),
        PAGE_EXECUTE_READWRITE => parts.push("RWX"),
        PAGE_EXECUTE_WRITECOPY => parts.push("RWXC"),
        PAGE_NOACCESS => parts.push("NOA"),
        PAGE_READONLY => parts.push("R--"),
        PAGE_READWRITE => parts.push("RW-"),
        PAGE_WRITECOPY => parts.push("RWC"),
        _ => parts.push("???"),
    }
    if prot & PAGE_GUARD != 0 { parts.push("GUARD"); }
    if prot & PAGE_NOCACHE != 0 { parts.push("NC"); }
    if prot & PAGE_WRITECOMBINE != 0 { parts.push("WC"); }
    parts.join("|")
}

fn is_readable(prot: DWORD) -> bool {
    if prot & PAGE_GUARD != 0 { return false; }
    if prot & PAGE_NOACCESS != 0 { return false; }
    let base = prot & 0xFF;
    matches!(
        base,
        PAGE_READONLY | PAGE_READWRITE | PAGE_EXECUTE_READ
        | PAGE_EXECUTE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_WRITECOPY
    )
}

fn is_writable(prot: DWORD) -> bool {
    if prot & PAGE_GUARD != 0 { return false; }
    if prot & PAGE_NOACCESS != 0 { return false; }
    let base = prot & 0xFF;
    matches!(
        base,
        PAGE_READWRITE | PAGE_EXECUTE_READWRITE
        | PAGE_WRITECOPY | PAGE_EXECUTE_WRITECOPY
    )
}

fn state_to_string(state: DWORD) -> String {
    match state {
        MEM_COMMIT => "Commit".into(),
        MEM_FREE => "Free".into(),
        MEM_RESERVE => "Reserve".into(),
        _ => format!("Unknown({})", state),
    }
}

fn type_info_to_string(t: DWORD) -> String {
    match t {
        MEM_IMAGE => "Image".into(),
        MEM_MAPPED => "Mapped".into(),
        MEM_PRIVATE => "Private".into(),
        _ => format!("Unknown({})", t),
    }
}

fn base_address_string(addr: u64) -> String {
    format!("{:X}", addr)
}

pub fn open_process(pid: u32) -> Result<ProcessHandle, String> {
    let process_info_opt = list_processes().ok()
        .and_then(|list| list.into_iter().find(|p| p.pid == pid));

    if let Some(info) = &process_info_opt {
        if is_protected_system_process(&info.name) {
            return Err(format!(
                "进程 {} 是受保护的系统进程，无法读取。\n\
                 该进程为 Windows 关键系统进程，即使以管理员身份也无法直接读取内存。\n\
                 建议选择普通用户进程（如 notepad.exe、chrome.exe、游戏进程等）",
                info.name
            ));
        }
    }

    let desired_access = PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_QUERY_LIMITED_INFORMATION;
    let handle = unsafe {
        OpenProcess(desired_access, FALSE as BOOL, pid)
    };

    if handle.is_null() {
        let err = unsafe { GetLastError() };
        let admin = is_admin();
        let suggestion = if err == ERROR_ACCESS_DENIED {
            if admin {
                "当前已为管理员权限，但进程可能受反作弊/DRM保护，或属于系统关键进程"
            } else {
                "请尝试以【管理员身份运行】此程序，然后重试"
            }
        } else {
            "请检查进程是否仍在运行"
        };

        let name = process_info_opt.as_ref().map(|p| p.name.as_str()).unwrap_or("未知进程");
        Err(format!(
            "无法打开进程 PID={} ({}): {}\n\n建议: {}",
            pid, name, get_last_error_msg(), suggestion
        ))
    } else {
        Ok(ProcessHandle(handle))
    }
}

pub fn probe_process_access(pid: u32) -> (bool, String) {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            FALSE as BOOL,
            pid
        )
    };
    if handle.is_null() {
        let err = unsafe { GetLastError() };
        let msg = if err == ERROR_ACCESS_DENIED {
            if is_admin() {
                "拒绝访问 - 进程受保护，即使管理员也无法读取".to_string()
            } else {
                "拒绝访问 - 请以管理员身份运行此程序".to_string()
            }
        } else {
            format!("无法访问: {}", get_last_error_msg())
        };
        (false, msg)
    } else {
        unsafe { CloseHandle(handle); }
        (true, "可以访问".to_string())
    }
}

pub fn list_processes() -> Result<Vec<ProcessInfo>, String> {
    let h_snapshot = unsafe {
        CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
    };
    if h_snapshot.is_null() || h_snapshot as isize == -1 {
        return Err(format!("创建进程快照失败: {}", get_last_error_msg()));
    }
    let _guard = ProcessHandle(h_snapshot);

    let mut processes: Vec<ProcessInfo> = Vec::new();
    let mut pe32: PROCESSENTRY32W = unsafe { mem::zeroed() };
    pe32.dwSize = mem::size_of::<PROCESSENTRY32W>() as DWORD;

    let result = unsafe { Process32FirstW(h_snapshot, &mut pe32) };
    if result == FALSE as BOOL {
        return Ok(processes);
    }

    loop {
        let pid = pe32.th32ProcessID;
        let name = unsafe { wchar_to_string(pe32.szExeFile.as_ptr()) };
        let current_pid = unsafe { GetCurrentProcessId() };
        let (path, memory_usage_mb, thread_count) = if pid != current_pid {
            match open_process_silent(pid) {
                Some(h) => (
                    get_process_path(h.0, pid).unwrap_or_default(),
                    get_process_memory_usage(h.0).unwrap_or(0.0),
                    get_process_thread_count(pid).unwrap_or(pe32.cntThreads)
                ),
                None => (String::new(), 0.0, pe32.cntThreads),
            }
        } else {
            (String::new(), 0.0, pe32.cntThreads)
        };

        processes.push(ProcessInfo {
            pid,
            name,
            path,
            memory_usage_mb,
            thread_count,
            parent_pid: Some(pe32.th32ParentProcessID),
        });

        let next = unsafe { Process32NextW(h_snapshot, &mut pe32) };
        if next == FALSE as BOOL {
            break;
        }
    }
    Ok(processes)
}

fn open_process_silent(pid: u32) -> Option<ProcessHandle> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_QUERY_LIMITED_INFORMATION,
            FALSE as BOOL,
            pid
        )
    };
    if handle.is_null() {
        None
    } else {
        Some(ProcessHandle(handle))
    }
}

fn get_process_path(h_process: HANDLE, _pid: u32) -> Result<String, String> {
    let mut buf: [u16; 1024] = [0; 1024];
    let size = unsafe {
        K32GetModuleFileNameExW(h_process, ptr::null_mut(), buf.as_mut_ptr(), 1024)
    };
    if size == 0 {
        Err("获取进程路径失败".into())
    } else {
        Ok(String::from_utf16_lossy(&buf[..size as usize]))
    }
}

fn get_process_memory_usage(h_process: HANDLE) -> Result<f64, String> {
    let regions = get_memory_regions_internal(h_process, 0, false)?;
    let total: i64 = regions.iter().filter(|r| r.state == "Commit").map(|r| r.region_size).sum();
    Ok(total as f64 / (1024.0 * 1024.0))
}

fn get_process_thread_count(target_pid: u32) -> Result<u32, String> {
    let h_snapshot = unsafe {
        CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
    };
    if h_snapshot.is_null() || h_snapshot as isize == -1 {
        return Err("线程快照失败".into());
    }
    let _guard = ProcessHandle(h_snapshot);

    let mut te32: THREADENTRY32 = unsafe { mem::zeroed() };
    te32.dwSize = mem::size_of::<THREADENTRY32>() as DWORD;

    let mut count = 0u32;
    let mut res = unsafe { Thread32First(h_snapshot, &mut te32) };
    while res != FALSE as BOOL {
        if te32.th32OwnerProcessID == target_pid {
            count += 1;
        }
        res = unsafe { Thread32Next(h_snapshot, &mut te32) };
    }
    Ok(count)
}

struct ModuleInfo {
    pub base: u64,
    pub size: u64,
    pub name: String,
}

fn get_process_modules(h_process: HANDLE) -> Result<Vec<ModuleInfo>, String> {
    let mut mods: Vec<HMODULE> = vec![ptr::null_mut(); 1024];
    let mut cb_needed: DWORD = 0;
    let ok = unsafe {
        K32EnumProcessModulesEx(
            h_process,
            mods.as_mut_ptr(),
            (mods.len() * mem::size_of::<HMODULE>()) as DWORD,
            &mut cb_needed,
            LIST_MODULES_ALL
        )
    };
    if ok == FALSE as BOOL {
        return Ok(Vec::new());
    }
    let count = (cb_needed as usize) / mem::size_of::<HMODULE>();
    let mut result = Vec::with_capacity(count);
    for i in 0..count.min(mods.len()) {
        let h_mod = mods[i];
        if h_mod.is_null() { continue; }
        let mut name_buf: [u16; 512] = [0; 512];
        let len = unsafe {
            K32GetModuleBaseNameW(h_process, h_mod, name_buf.as_mut_ptr(), 512)
        };
        let name = if len > 0 {
            String::from_utf16_lossy(&name_buf[..len as usize])
        } else {
            continue;
        };
        let base = h_mod as u64;
        result.push(ModuleInfo { base, size: 0, name });
    }
    Ok(result)
}

fn classify_region(
    base: u64,
    _size: i64,
    mbi: &MEMORY_BASIC_INFORMATION,
    modules: &[ModuleInfo],
) -> RegionType {
    if mbi.Type == MEM_IMAGE {
        return RegionType::Image;
    }
    for m in modules {
        if base == m.base {
            return RegionType::Image;
        }
    }
    if mbi.Type == MEM_MAPPED {
        return RegionType::Mapped;
    }
    if mbi.Protect & PAGE_GUARD != 0 || is_likely_stack(base, _size, mbi) {
        return RegionType::Stack;
    }
    if is_likely_heap(mbi) {
        return RegionType::Heap;
    }
    RegionType::Private
}

fn is_likely_stack(base: u64, size: i64, mbi: &MEMORY_BASIC_INFORMATION) -> bool {
    if mbi.Protect & PAGE_GUARD != 0 { return true; }
    if (mbi.Protect & 0xFF) == PAGE_READWRITE && size <= 0x200000 {
        let mod_base = base & 0xFFFFFFFF00000000;
        if mod_base == 0 || mod_base == 0x7FF0000000000000 {
            return true;
        }
    }
    false
}

fn is_likely_heap(mbi: &MEMORY_BASIC_INFORMATION) -> bool {
    let prot = mbi.Protect & 0xFF;
    (prot == PAGE_READWRITE || prot == PAGE_WRITECOPY) && mbi.Type == MEM_PRIVATE
}

fn find_module_name(base: u64, modules: &[ModuleInfo]) -> String {
    for m in modules {
        if base == m.base {
            return m.name.clone();
        }
    }
    String::new()
}

fn get_memory_regions_internal(h_process: HANDLE, snapshot_id: i64, with_data: bool) -> Result<Vec<MemoryRegion>, String> {
    let modules = get_process_modules(h_process).unwrap_or_default();

    let mut regions: Vec<MemoryRegion> = Vec::new();
    let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { mem::zeroed() };
    let mut address: u64 = 0;
    let mbi_size = mem::size_of::<MEMORY_BASIC_INFORMATION>();

    loop {
        let size = unsafe {
            VirtualQueryEx(
                h_process,
                address as *const _,
                &mut mbi,
                mbi_size
            )
        };
        if size != mbi_size {
            break;
        }

        let base = mbi.BaseAddress as u64;
        let rsize = mbi.RegionSize as i64;

        if mbi.State == MEM_FREE {
            address = base + rsize as u64;
            continue;
        }

        let region_type = classify_region(base, rsize, &mbi, &modules);
        let module_name = find_module_name(base, &modules);
        let details = if module_name.is_empty() {
            match region_type {
                RegionType::Heap => format!("堆区域"),
                RegionType::Stack => format!("线程栈"),
                RegionType::Private => format!("私有提交区域"),
                RegionType::Mapped => format!("映射文件"),
                _ => String::new(),
            }
        } else {
            String::new()
        };

        let readable = is_readable(mbi.Protect);
        let writable = is_writable(mbi.Protect);
        let should_store_data = with_data
            && mbi.State == MEM_COMMIT
            && readable
            && !module_name.is_empty()
            && rsize <= 256 * 1024 * 1024;

        regions.push(MemoryRegion {
            id: 0,
            snapshot_id,
            region_type,
            base_address: base_address_string(base),
            region_size: rsize,
            protection: protection_to_string(mbi.Protect),
            state: state_to_string(mbi.State),
            type_info: type_info_to_string(mbi.Type),
            module_name,
            details,
            has_data: should_store_data,
            is_readable: readable,
            is_writable: writable,
        });

        address = base + rsize as u64;
        if address == 0 {
            break;
        }
    }

    Ok(regions)
}

pub fn get_memory_regions(h_process: HANDLE, snapshot_id: i64) -> Result<Vec<MemoryRegion>, String> {
    get_memory_regions_internal(h_process, snapshot_id, true)
}

pub fn read_memory(h_process: HANDLE, address: u64, length: usize) -> Result<Vec<u8>, String> {
    if length == 0 {
        return Ok(Vec::new());
    }
    let mut buf: Vec<u8> = vec![0u8; length];
    let mut bytes_read: usize = 0;
    let ok = unsafe {
        ReadProcessMemory(
            h_process,
            address as *const _,
            buf.as_mut_ptr() as *mut _,
            length,
            &mut bytes_read as *mut usize as *mut _
        )
    };
    let err = unsafe { GetLastError() };
    if ok == FALSE as BOOL {
        if err == ERROR_PARTIAL_COPY && bytes_read > 0 {
            buf.truncate(bytes_read);
            return Ok(buf);
        }
        Err(format!("读取内存失败 @ 0x{:X}: {}", address, get_last_error_msg()))
    } else if bytes_read == 0 {
        Err(format!("读取内存失败 @ 0x{:X}: 读取字节数为0", address))
    } else {
        buf.truncate(bytes_read);
        Ok(buf)
    }
}

pub fn try_read_memory_safe(h_process: HANDLE, address: u64, length: usize) -> Result<Vec<u8>, String> {
    if length == 0 {
        return Ok(Vec::new());
    }
    let max_chunk = 4096;
    let mut result = Vec::with_capacity(length);
    let mut offset = 0usize;

    while offset < length {
        let chunk_size = (length - offset).min(max_chunk);
        match read_memory(h_process, address + offset as u64, chunk_size) {
            Ok(mut chunk) => {
                result.append(&mut chunk);
                if chunk.len() < chunk_size {
                    break;
                }
                offset += chunk_size;
            }
            Err(e) => {
                if result.is_empty() {
                    return Err(e);
                } else {
                    break;
                }
            }
        }
    }

    if result.is_empty() {
        Err(format!("读取内存失败 @ 0x{:X}", address))
    } else {
        Ok(result)
    }
}
