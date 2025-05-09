// build.rs
fn main() {
    #[cfg(target_os = "macos")]
    {
        // Set the minimum macOS version to 10.13 High Sierra
        println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=10.13");
        println!("cargo:rustc-link-arg=-mmacosx-version-min=10.13");
        
        // Ensure the necessary frameworks are linked
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
        
        // Compile the C shim for macOS
        cc::Build::new()
            .file("macos_shim.c")
            .flag("-mmacosx-version-min=10.13")
            .flag("-Wall")
            .include("/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk/System/Library/Frameworks/ApplicationServices.framework/Headers")
            .include("/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk/System/Library/Frameworks/CoreFoundation.framework/Headers")
            .compile("macos_shim");
    }

    #[cfg(target_os = "windows")]
    {
        // Windows-specific build configuration
        println!("cargo:rustc-link-lib=user32");
    }
}