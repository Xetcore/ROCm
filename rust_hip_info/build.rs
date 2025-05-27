use std::env;
use std::path::PathBuf;

fn main() {
    // 1. Determine HIP include and lib paths
    let rocm_path_env_val = env::var("ROCM_PATH").unwrap_or_else(|_| "/opt/rocm".to_string());
    let rocm_path = PathBuf::from(rocm_path_env_val);

    let hip_include_path = rocm_path.join("include");
    let hip_lib_path = rocm_path.join("lib");

    let hip_api_header = hip_include_path.join("hip/hip_runtime_api.h");
    let hip_version_header = hip_include_path.join("hip/hip_version.h"); // For version functions

    // Check if essential header exists
    if !hip_api_header.exists() {
        eprintln!(
            "Error: HIP API header not found at {}. \
            Please ensure ROCM_PATH is set correctly (current value: '{}') or ROCm is installed in /opt/rocm. \
            This build script requires access to HIP headers to generate Rust bindings.",
            hip_api_header.display(), rocm_path.display()
        );
        // Consider adding a more visible warning if ROCM_PATH was defaulted:
        if env::var("ROCM_PATH").is_err() {
             eprintln!("Hint: ROCM_PATH environment variable was not set, defaulted to /opt/rocm.");
        }
        std::process::exit(1); // Fail the build if essential header is missing
    }

    // 2. Setup bindgen
    println!("cargo:rerun-if-changed={}", hip_api_header.display());
    if hip_version_header.exists() {
        println!("cargo:rerun-if-changed={}", hip_version_header.display());
    } else {
        println!(
            "cargo:warning=HIP version header not found at {}. Driver/Runtime version functions might not be available if not defined in hip_runtime_api.h itself.",
            hip_version_header.display()
        );
    }

    let mut builder = bindgen::Builder::default()
        .header(hip_api_header.to_string_lossy())
        // Add clang args for include paths.
        .clang_arg(format!("-I{}", hip_include_path.display())) // General /opt/rocm/include
        // It's often good to also add the specific subdirectory if headers use relative paths like "hip_common.h"
        .clang_arg(format!("-I{}", hip_include_path.join("hip").display()))
        .allowlist_function("hipGetDeviceCount")
        .allowlist_function("hipGetDeviceProperties")
        .allowlist_function("hipDriverGetVersion")
        .allowlist_function("hipRuntimeGetVersion")
        .allowlist_type("hipDeviceProp_t")
        .allowlist_type("hipError_t")
        .allowlist_var("hipSuccess") // For `pub const hipSuccess: hipError_t = 0;`
        // Potentially problematic types/vars to blocklist if they cause issues:
        .blocklist_item("IMAGE_pitch") // From original build.rs, seems like a known issue
        .blocklist_item("hipDeviceArch_t") // Can conflict if hiprtc.h is pulled in transitively
        .blocklist_item("hipUUID")    // Can conflict
        .rustified_enum("hipError_t") // Generate hipError_t as a Rust enum
        .derive_default(true)   // Derive Default for generated structs
        .derive_debug(true)     // Derive Debug for generated structs
        .prepend_enum_name(false) // Avoids names like hipError_t_hipSuccess if hipSuccess were an enum variant
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new())); // For cargo integration

    if hip_version_header.exists() {
        builder = builder.header(hip_version_header.to_string_lossy());
    }

    let bindings = builder.generate().expect("Unable to generate bindings");

    // 3. Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    // 4. Link against libamdhip64.so
    println!("cargo:rustc-link-lib=amdhip64");
    println!("cargo:rustc-link-search=native={}", hip_lib_path.display());
}
