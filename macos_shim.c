// macos_shim.c
#include <ApplicationServices/ApplicationServices.h>
#include <CoreFoundation/CoreFoundation.h>
#include <stdbool.h>
#include <stdio.h>

// Callback type definition to match Rust's definition
typedef CGEventRef (*EventTapCallback)(CGEventTapProxy proxy, CGEventType type, 
                                      CGEventRef event, void *userInfo);

// Create and initialize an event tap for keyboard and mouse events
void* InitializeEventTap(EventTapCallback callback, void* userData) {
    // Define which events we want to listen for
    CGEventMask eventMask = (1 << kCGEventKeyDown) | (1 << kCGEventKeyUp) |
                           (1 << kCGEventLeftMouseDown) | (1 << kCGEventLeftMouseUp) |
                           (1 << kCGEventRightMouseDown) | (1 << kCGEventRightMouseUp) |
                           (1 << kCGEventMouseMoved) | (1 << kCGEventScrollWheel);
    
    // Create the event tap
    CFMachPortRef eventTap = CGEventTapCreate(
        kCGSessionEventTap,           // Tap at session level (user's session)
        kCGHeadInsertEventTap,        // Insert at start of event chain
        kCGEventTapOptionDefault,     // Default options
        eventMask,                    // Event types to listen for
        callback,                     // Callback function
        userData                      // User data (passed to callback)
    );
    
    if (!eventTap) {
        printf("Failed to create event tap.\n");
        return NULL;
    }
    
    return eventTap;
}

// Enable or disable the event tap
void EnableEventTap(void* tap, bool enable) {
    if (tap) {
        CGEventTapEnable((CFMachPortRef)tap, enable);
    }
}

// Add the event tap to the current run loop
void AddEventTapToCurrentRunLoop(void* tap) {
    if (tap) {
        // Create a run loop source from the event tap
        CFRunLoopSourceRef runLoopSource = 
            CFMachPortCreateRunLoopSource(kCFAllocatorDefault, (CFMachPortRef)tap, 0);
        
        if (runLoopSource) {
            // Add the run loop source to the current run loop
            CFRunLoopAddSource(CFRunLoopGetCurrent(), 
                              runLoopSource, 
                              kCFRunLoopDefaultMode);
            
            // We don't need to keep the reference since the run loop retains it
            CFRelease(runLoopSource);
        }
    }
}

// Clean up the event tap
void CleanupEventTap(void* tap) {
    if (tap) {
        // Note: We don't need to remove the run loop source as CFRunLoopStop
        // will clean up resources. Just release the tap.
        CFRelease((CFMachPortRef)tap);
    }
}

// Run the current run loop with a timeout
int RunCurrentRunLoopWithTimeout(double seconds) {
    return CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, true);
}

// Extract key code from a keyboard event
uint16_t GetKeyCodeFromEvent(CGEventRef event) {
    if (event) {
        return (uint16_t)CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
    }
    return 0;
}

// Get current mouse position
int GetCurrentMousePos(CGPoint* point) {
    if (!point) return -1;
    
    CGEventRef event = CGEventCreate(NULL);
    if (event) {
        *point = CGEventGetLocation(event);
        CFRelease(event);
        return 0;
    }
    return -1;
}

// Print information about a specific event (for debugging)
void PrintEventInfo(CGEventType type, CGEventRef event) {
    printf("Event type: %d\n", (int)type);
    
    if (type == kCGEventKeyDown || type == kCGEventKeyUp) {
        CGKeyCode keyCode = (CGKeyCode)CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
        printf("Key code: %d\n", (int)keyCode);
    } else if (type == kCGEventMouseMoved || 
              type == kCGEventLeftMouseDown || type == kCGEventLeftMouseUp ||
              type == kCGEventRightMouseDown || type == kCGEventRightMouseUp) {
        CGPoint location = CGEventGetLocation(event);
        printf("Mouse location: (%f, %f)\n", location.x, location.y);
    }
}