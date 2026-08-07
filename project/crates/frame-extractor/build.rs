use std::env;
use std::path::PathBuf;

/// Build script for frame-extractor crate.
///
/// Handles:
/// - FFmpeg library linking via pkg-config or environment variables
/// - Optional Zig-based C library compilation for performance-critical paths
/// - Platform-specific configuration
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    println!("cargo:rerun-if-env-changed=SENTINEL_USE_ZIG");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    // Attempt to link FFmpeg libraries through pkg-config
    link_ffmpeg();

    // Optionally compile Zig-based helper libraries
    if env::var("SENTINEL_USE_ZIG").is_ok() {
        compile_zig_helpers();
    }

    // Emit configuration flags for conditional compilation
    emit_cfg_flags();
}

/// Link FFmpeg libraries using pkg-config or fallback to manual linking.
fn link_ffmpeg() {
    let ffmpeg_libs = [
        "libavcodec",
        "libavformat",
        "libavutil",
        "libswscale",
        "libswresample",
    ];

    // Try pkg-config first
    let mut pkg_config_success = true;
    for lib in &ffmpeg_libs {
        match pkg_config::Config::new()
            .statik(false)
            .probe(lib) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("pkg-config failed for {}: {}", lib, e);
                pkg_config_success = false;
                break;
            }
        }
    }

    if !pkg_config_success {
        // Fallback: use FFMPEG_DIR environment variable
        if let Ok(ffmpeg_dir) = env::var("FFMPEG_DIR") {
            let lib_dir = PathBuf::from(&ffmpeg_dir).join("lib");
            println!("cargo:rustc-link-search=native={}", lib_dir.display());

            // Link individual FFmpeg libraries
            for lib in &ffmpeg_libs {
                let lib_name = lib.trim_start_matches("lib");
                println!("cargo:rustc-link-lib={}", lib_name);
            }
        } else {
            // Last resort: hope the libraries are in the default search path
            for lib in &ffmpeg_libs {
                let lib_name = lib.trim_start_matches("lib");
                println!("cargo:rustc-link-lib={}", lib_name);
            }
        }
    }
}

/// Compile Zig-based helper libraries for performance-critical operations.
///
/// This is triggered when the `SENTINEL_USE_ZIG` environment variable is set.
/// Zig is used for cross-compilation of small C helper functions that
/// accelerate pixel format conversion and histogram computation.
fn compile_zig_helpers() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let zig_src = PathBuf::from("src/native/zig_helpers.zig");

    if !zig_src.exists() {
        // No Zig source available, skip compilation
        return;
    }

    let target = env::var("TARGET").unwrap();
    let zig_target = map_rust_target_to_zig(&target);

    let output_lib = out_dir.join("libzig_helpers.a");

    let status = std::process::Command::new("zig")
        .args([
            "build-lib",
            "-O",
            "ReleaseFast",
            "-target",
            &zig_target,
            "-femit-bin",
            output_lib.to_str().unwrap(),
            zig_src.to_str().unwrap(),
        ])
        .status();

    match status {
        Ok(exit) if exit.success() => {
            println!("cargo:rustc-link-search=native={}", out_dir.display());
            println!("cargo:rustc-link-lib=static=zig_helpers");
        }
        Ok(exit) => {
            eprintln!("Zig compilation failed with exit code: {:?}", exit.code());
        }
        Err(e) => {
            eprintln!("Failed to run zig compiler: {}", e);
        }
    }
}

/// Map Rust target triple to Zig target triple.
fn map_rust_target_to_zig(target: &str) -> String {
    match target {
        "x86_64-unknown-linux-gnu" => "x86_64-linux-gnu".to_string(),
        "aarch64-unknown-linux-gnu" => "aarch64-linux-gnu".to_string(),
        "x86_64-apple-darwin" => "x86_64-macos-none".to_string(),
        "aarch64-apple-darwin" => "aarch64-macos-none".to_string(),
        "x86_64-pc-windows-gnu" => "x86_64-windows-gnu".to_string(),
        "x86_64-pc-windows-msvc" => "x86_64-windows-msvc".to_string(),
        other => other.to_string(),
    }
}

/// Emit cfg flags for conditional compilation based on platform and features.
fn emit_cfg_flags() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    if target_os == "linux" {
        println!("cargo:rustc-cfg=platform_linux");
    } else if target_os == "macos" {
        println!("cargo:rustc-cfg=platform_macos");
    } else if target_os == "windows" {
        println!("cargo:rustc-cfg=platform_windows");
    }

    if target_arch == "x86_64" {
        println!("cargo:rustc-cfg=arch_x86_64");
    } else if target_arch == "aarch64" {
        println!("cargo:rustc-cfg=arch_aarch64");
    }
}
