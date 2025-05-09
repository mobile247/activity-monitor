// activity_monitor/src/macos.rs
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::os::raw::{c_void, c_int};

static RUNNING: AtomicBool = AtomicBool::new(false);
static mut EVENT_TAP_REF: Option<*mut c_void> = None;

// Stateful tracking of keys and activity
lazy_static::lazy_static! {
    static ref PRESSED_KEYS: Mutex<HashMap<u16, Instant>> = Mutex::new(HashMap::new());
    static ref KEY_TIMEOUT: Mutex<Duration> = Mutex::new(Duration::from_secs(2)); // Timeout for key presses
}

// Define CGPoint structure
#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}

// Smart activity detection for keyboard
fn process_keyboard_event(key_code: u16, is_down: bool) -> (bool, bool) {
    let mut keys = PRESSED_KEYS.lock().unwrap();
    let key_timeout = *KEY_TIMEOUT.lock().unwrap();
    
    // Get current time
    let now = Instant::now();
    
    // For key down events
    if is_down {
        // If this key is not already pressed (or has timed out), count it as new activity
        if !keys.contains_key(&key_code) || 
           now.duration_since(*keys.get(&key_code).unwrap()) > key_timeout {
            // Update key press time
            keys.insert(key_code, now);
            
            // Signal genuine activity
            return (true, true);
        }
    } else {
        // Remove key from pressed keys if it exists
        if keys.contains_key(&key_code) {
            // Only count key up if it hasn't been too long since key down (prevent stuck keys)
            let time_since_key_down = if let Some(time) = keys.get(&key_code) {
                now.duration_since(*time)
            } else {
                Duration::from_secs(0)
            };
            
            keys.remove(&key_code);
            
            // For key up, we don't count it as genuine activity
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

// Define event types
const EVENT_TYPE_KEY_DOWN: u32 = 10;
const EVENT_TYPE_KEY_UP: u32 = 11;
const EVENT_TYPE_MOUSE_EVENTS: [u32; 6] = [1, 2, 3, 4, 5, 22]; // Various mouse events

// Callback type for event tap
type EventTapCallbackType = unsafe extern "C" fn(
    proxy: *mut c_void,
    event_type: c_int,
    event: *mut c_void,
    user_info: *mut c_void,
) -> *mut c_void;

// Link to our C shim
extern "C" {
    // Initialize event tap
    fn InitializeEventTap(
        callback: EventTapCallbackType,
        user_data: *mut c_void,
    ) -> *mut c_void;
    
    // Enable or disable the event tap
    fn EnableEventTap(tap: *mut c_void, enable: bool);
    
    // Clean up the event tap
    fn CleanupEventTap(tap: *mut c_void);
    
    // Add the event tap to the current run loop
    fn AddEventTapToCurrentRunLoop(tap: *mut c_void);
    
    // Run the current run loop for a specified time
    fn RunCurrentRunLoopWithTimeout(seconds: f64) -> c_int;
    
    // Get key code from event
    fn GetKeyCodeFromEvent(event: *mut c_void) -> u16;
    
    // Get current mouse position
    fn GetCurrentMousePos(point: *mut CGPoint) -> c_int;
}

// Callback function for the event tap
unsafe extern "C" fn event_callback(
    _proxy: *mut c_void,
    event_type: c_int,
    event: *mut c_void,
    _user_info: *mut c_void,
) -> *mut c_void {
    let event_type_u32 = event_type as u32;
    
    // Handle keyboard events
    if event_type_u32 == EVENT_TYPE_KEY_DOWN || event_type_u32 == EVENT_TYPE_KEY_UP {
        // Extract key code from event
        let key_code = GetKeyCodeFromEvent(event);
        
        // Process the event - get both increment_counter and is_genuine flags
        let (increment_counter, is_genuine) = process_keyboard_event(
            key_code, 
            event_type_u32 == EVENT_TYPE_KEY_DOWN
        );
        
        if increment_counter {
            // Only increment counter if we detected new activity
            super::increment_keyboard();
        }
        
        // Update activity time only for genuine activity
        if is_genuine {
            super::update_genuine_activity_time(true);
        }
    } 
    // Handle mouse events
    else if EVENT_TYPE_MOUSE_EVENTS.contains(&event_type_u32) {
        // Mouse activity is always considered genuine
        super::increment_mouse();
        super::update_genuine_activity_time(true);
    }
    
    // Return the event unchanged
    event
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
            // Create an event tap for keyboard and mouse events
            let event_tap = InitializeEventTap(event_callback, std::ptr::null_mut());
            
            if !event_tap.is_null() {
                EVENT_TAP_REF = Some(event_tap);
                
                // Add the event tap to the current run loop
                AddEventTapToCurrentRunLoop(event_tap);
                
                // Enable the event tap
                EnableEventTap(event_tap, true);
                
                // Run the event loop while monitoring is active
                let mut last_cleanup = Instant::now();
                let cleanup_interval = Duration::from_secs(10);
                
                while RUNNING.load(Ordering::SeqCst) {
                    // Run the run loop for a short interval (50ms)
                    RunCurrentRunLoopWithTimeout(0.05);
                    
                    // Periodically clean up stale keys
                    let now = Instant::now();
                    if now.duration_since(last_cleanup) > cleanup_interval {
                        cleanup_stale_keys();
                        last_cleanup = now;
                    }
                    
                    // Small sleep to prevent excessive CPU usage
                    thread::sleep(Duration::from_millis(5));
                }
                
                // Clean up the event tap
                EnableEventTap(event_tap, false);
                CleanupEventTap(event_tap);
                EVENT_TAP_REF = None;
            }
        }
    });
}

pub fn stop_monitoring() {
    RUNNING.store(false, Ordering::SeqCst);
    
    // Give the monitoring thread a moment to clean up
    thread::sleep(Duration::from_millis(100));
    
    // Explicitly disable and clean up the event tap if it's still active
    unsafe {
        if let Some(tap) = EVENT_TAP_REF {
            EnableEventTap(tap, false);
            CleanupEventTap(tap);
            EVENT_TAP_REF = None;
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