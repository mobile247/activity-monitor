// activity_monitor/src/lib.rs
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "macos")]
mod macos;

// Global state
static MONITORING: AtomicBool = AtomicBool::new(false);
static KEYBOARD_COUNT: AtomicU64 = AtomicU64::new(0);
static MOUSE_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_GENUINE_ACTIVITY: AtomicU64 = AtomicU64::new(0);

// FFI exports
#[no_mangle]
pub extern "C" fn start_monitoring() -> bool {
    if MONITORING.load(Ordering::SeqCst) {
        return false; // Already monitoring
    }
    
    MONITORING.store(true, Ordering::SeqCst);
    
    // Reset counters
    reset_counters();
    
    // Start platform-specific monitoring
    #[cfg(target_os = "windows")]
    windows::start_monitoring();
    
    #[cfg(target_os = "macos")]
    macos::start_monitoring();
    
    true
}

#[no_mangle]
pub extern "C" fn stop_monitoring() -> bool {
    if !MONITORING.load(Ordering::SeqCst) {
        return false; // Not monitoring
    }
    
    MONITORING.store(false, Ordering::SeqCst);
    
    // Stop platform-specific monitoring
    #[cfg(target_os = "windows")]
    windows::stop_monitoring();
    
    #[cfg(target_os = "macos")]
    macos::stop_monitoring();
    
    true
}

#[no_mangle]
pub extern "C" fn get_keyboard_count() -> u64 {
    KEYBOARD_COUNT.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn get_mouse_count() -> u64 {
    MOUSE_COUNT.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn get_idle_time() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();
    
    let last = LAST_GENUINE_ACTIVITY.load(Ordering::SeqCst);
    if last == 0 || now < last {
        return 0;
    }
    
    now - last
}

#[no_mangle]
pub extern "C" fn reset_counters() {
    KEYBOARD_COUNT.store(0, Ordering::SeqCst);
    MOUSE_COUNT.store(0, Ordering::SeqCst);
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();
    LAST_GENUINE_ACTIVITY.store(now, Ordering::SeqCst);
    
    // Also reset platform-specific state if needed
    #[cfg(target_os = "windows")]
    windows::reset_monitoring_state();
    
    #[cfg(target_os = "macos")]
    macos::reset_monitoring_state();
}

#[no_mangle]
pub extern "C" fn save_activity_log(path_ptr: *const u8, path_len: usize) -> bool {
    if path_ptr.is_null() {
        return false;
    }
    
    let path_slice = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let path_str = match std::str::from_utf8(path_slice) {
        Ok(s) => s,
        Err(_) => return false,
    };
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();
        
    let keyboard_count = KEYBOARD_COUNT.load(Ordering::SeqCst);
    let mouse_count = MOUSE_COUNT.load(Ordering::SeqCst);
    let idle_time = get_idle_time();
    
    let log_entry = format!(
        "{},{},{},{}\n",
        now, keyboard_count, mouse_count, idle_time
    );
    
    let path = Path::new(path_str);
    let file_exists = path.exists();
    
    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(path) 
    {
        Ok(mut file) => {
            if !file_exists {
                // Write header if creating new file
                if let Err(_) = file.write_all(b"timestamp,keyboard_count,mouse_count,idle_time_seconds\n") {
                    return false;
                }
            }
            
            match file.write_all(log_entry.as_bytes()) {
                Ok(_) => {
                    // Reset counters after logging
                    reset_counters();
                    true
                },
                Err(_) => false,
            }
        },
        Err(_) => false,
    }
}

// Internal functions for the OS-specific modules to call
pub(crate) fn increment_keyboard() {
    KEYBOARD_COUNT.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn increment_mouse() {
    MOUSE_COUNT.fetch_add(1, Ordering::SeqCst);
}

// Update the timestamp for genuine user activity
pub(crate) fn update_genuine_activity_time(is_genuine: bool) {
    if is_genuine {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        
        LAST_GENUINE_ACTIVITY.store(now, Ordering::SeqCst);
    }
}