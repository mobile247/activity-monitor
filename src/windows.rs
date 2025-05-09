// activity_monitor/src/windows.rs
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowsHookExW, UnhookWindowsHookEx, CallNextHookEx,
    WH_KEYBOARD_LL, WH_MOUSE_LL, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, MSG, GetMessageW, TranslateMessage, DispatchMessageW,
    WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};
use windows::Win32::Foundation::{LPARAM, WPARAM, LRESULT, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

static RUNNING: AtomicBool = AtomicBool::new(false);
static mut KEYBOARD_HOOK: Option<HHOOK> = None;
static mut MOUSE_HOOK: Option<HHOOK> = None;

// Track pressed keys to avoid duplicate counting
lazy_static::lazy_static! {
    static ref PRESSED_KEYS: Mutex<HashMap<u32, Instant>> = Mutex::new(HashMap::new());
    static ref KEY_TIMEOUT: Mutex<Duration> = Mutex::new(Duration::from_secs(2)); // Timeout for key presses
}

// Smart activity detection
fn process_keyboard_event(virtual_key: u32, is_down: bool) -> (bool, bool) {
    let mut keys = PRESSED_KEYS.lock().unwrap();
    let key_timeout = *KEY_TIMEOUT.lock().unwrap();
    
    // Get current time
    let now = Instant::now();
    
    // For key down events
    if is_down {
        // If this key is not already pressed (or has timed out), count it as new activity
        if !keys.contains_key(&virtual_key) || 
           now.duration_since(*keys.get(&virtual_key).unwrap()) > key_timeout {
            // Update key press time
            keys.insert(virtual_key, now);
            
            // Signal genuine activity
            return (true, true);
        }
    } else {
        // Remove key from pressed keys if it exists
        if keys.contains_key(&virtual_key) {
            // Only count key up if it hasn't been too long since key down (prevent stuck keys)
            let time_since_key_down = if let Some(time) = keys.get(&virtual_key) {
                now.duration_since(*time)
            } else {
                Duration::from_secs(0)
            };
            
            keys.remove(&virtual_key);
            
            // For key up, we still count it as keyboard activity, but not genuine activity
            // if it's been held down too long
            let is_genuine = time_since_key_down < key_timeout;
            
            // Return keyboard activity but no genuine activity for key up events
            return (false, false);
        }
    }
    
    // No new activity detected
    (false, false)
}

// Check for timeout on all pressed keys
fn cleanup_stale_keys() {
    let mut keys = PRESSED_KEYS.lock().unwrap();
    let now = Instant::now();
    let key_timeout = *KEY_TIMEOUT.lock().unwrap();
    
    // Remove keys that have been pressed too long (stuck keys)
    keys.retain(|_, time| now.duration_since(*time) < key_timeout);
}

pub fn start_monitoring() {
    if RUNNING.load(Ordering::SeqCst) {
        return;
    }
    
    RUNNING.store(true, Ordering::SeqCst);
    
    // Reset state
    reset_monitoring_state();
    
    thread::spawn(|| {
        unsafe {
            let h_module = GetModuleHandleW(None).unwrap_or_default();
            
            // Set keyboard hook
            KEYBOARD_HOOK = Some(SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                h_module,
                0
            ).expect("Failed to set keyboard hook"));
                        
            // Set mouse hook
            MOUSE_HOOK = Some(SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_proc),
                h_module,
                0
            ).expect("Failed to set mouse hook"));

            // Message loop to keep hooks active
            let mut msg = MSG::default();
            let mut last_cleanup = Instant::now();
            let cleanup_interval = Duration::from_secs(10);
            
            while RUNNING.load(Ordering::SeqCst) {
                if GetMessageW(&mut msg, HWND(0), 0, 0).as_bool() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else {
                    break;
                }
                
                thread::sleep(Duration::from_millis(10));
                
                // Periodically clean up stale keys
                let now = Instant::now();
                if now.duration_since(last_cleanup) > cleanup_interval {
                    cleanup_stale_keys();
                    last_cleanup = now;
                }
            }
            
            // Clean up hooks
            if let Some(hook) = KEYBOARD_HOOK {
                UnhookWindowsHookEx(hook);
                KEYBOARD_HOOK = None;
            }
            
            if let Some(hook) = MOUSE_HOOK {
                UnhookWindowsHookEx(hook);
                MOUSE_HOOK = None;
            }
        }
    });
}

pub fn stop_monitoring() {
    RUNNING.store(false, Ordering::SeqCst);
    
    unsafe {
        if let Some(hook) = KEYBOARD_HOOK {
            UnhookWindowsHookEx(hook);
            KEYBOARD_HOOK = None;
        }
        
        if let Some(hook) = MOUSE_HOOK {
            UnhookWindowsHookEx(hook);
            MOUSE_HOOK = None;
        }
    }
    
    // Clear state
    reset_monitoring_state();
}

// Reset monitoring state (called from lib.rs)
pub fn reset_monitoring_state() {
    let mut keys = PRESSED_KEYS.lock().unwrap();
    keys.clear();
}

extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kbd_struct: *const KBDLLHOOKSTRUCT = lparam.0 as *const _;
        
        // Only process if we have valid kbd data
        if !kbd_struct.is_null() {
            unsafe {
                let virtual_key = (*kbd_struct).vkCode;
                
                // Check message type for key down vs key up
                let is_key_down = wparam.0 == WM_KEYDOWN as usize || 
                                  wparam.0 == WM_SYSKEYDOWN as usize;
                
                // Process the event - get both increment_counter and is_genuine flags
                let (increment_counter, is_genuine) = process_keyboard_event(virtual_key, is_key_down);
                
                if increment_counter {
                    // Increment keyboard counter for activity
                    super::increment_keyboard();
                }
                
                // Update last activity time only for genuine activity
                if is_genuine {
                    super::update_genuine_activity_time(true);
                }
            }
        }
    }
    
    unsafe {
        CallNextHookEx(HHOOK(0), code, wparam, lparam)
    }
}

extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // Mouse activity is always considered genuine
        super::increment_mouse();
        super::update_genuine_activity_time(true);
    }
    
    unsafe {
        CallNextHookEx(HHOOK(0), code, wparam, lparam)
    }
}