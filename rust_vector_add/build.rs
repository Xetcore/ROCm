extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    // Tell cargo to look for shared libraries in the specified directory
    // Adjust this path to your HIP installation's lib directory if necessary
    println!("cargo:rustc-link-search=/opt/rocm/hip/lib"); 
    // Link against the HIP runtime library
    println!("cargo:rustc-link-lib=amdhip64");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=wrapper.h");

    // Add /opt/rocm/hip/include to clang's search paths.
    // This is crucial for bindgen to find <hip/hip_runtime_api.h>, etc.
    // Adjust this path if your ROCm installation differs.
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        // Rely on bindgen's default search paths or environment variables like HIP_PATH
        // now that libclang is installed. Explicitly adding /opt/rocm/... didn't resolve it.
        // Specify types and functions to generate bindings for.
        // This helps keep the generated bindings minimal and manageable.
        .allowlist_function("hipMalloc")
        .allowlist_function("hipFree")
        .allowlist_function("hipMemcpy")
        .allowlist_function("hipMemcpyKind") // enum used by hipMemcpy
        .allowlist_function("hipDeviceSynchronize")
        .allowlist_function("hipGetErrorString")
        .allowlist_function("hipModuleLoadData")
        .allowlist_function("hipModuleGetFunction")
        .allowlist_function("hipModuleLaunchKernel")
        .allowlist_function("hipModuleUnload")
        .allowlist_type("hipError_t")
        .allowlist_type("hipDeviceptr_t")
        .allowlist_type("hipModule_t")
        .allowlist_type("hipFunction_t")
        .allowlist_type("hipStream_t")
        .allowlist_type("hipMemcpyKind") // Also allowlist the enum type itself
        // Tell cargo to invalidate the built crate whenever any of
        // the included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
