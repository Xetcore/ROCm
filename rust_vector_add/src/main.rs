// rust_vector_add/src/main.rs

#[macro_use] // To use hip_call! from hip_utils
extern crate log;

mod hip_utils;

use anyhow::{anyhow, Context, Result};
use hip_utils::*; // Imports hip_call, hipError_t, etc.
use std::ffi::{c_void, CString};
use std::mem;
use std::path::Path;
use std::ptr;
use std::time::Instant;

// Ensure this path is correct for your setup or make it configurable
const KERNEL_PATH: &str = "./vectorAdd_kernel.co"; 
// This assumes vectorAdd_kernel.co is in the same directory as the executable,
// or in the current working directory when running `cargo run`.
// Users will need to compile vectorAdd_kernel.hip to vectorAdd_kernel.co using hipcc
// e.g.: hipcc --genco vectorAdd_kernel.hip -o vectorAdd_kernel.co

const VECTOR_SIZE: usize = 1024 * 1024; // Example vector size
const THREADS_PER_BLOCK: usize = 256;

fn main() -> Result<()> {
    env_logger::init();
    info!("Starting rust_vector_add application...");

    let n = VECTOR_SIZE;
    let data_size = n * mem::size_of::<f32>();

    // 1. Initialize Host Vectors
    info!("Initializing host vectors (A, B, C_cpu, C_gpu) of size {}", n);
    let mut h_a: Vec<f32> = (0..n).map(|i| i as f32).collect();
    let mut h_b: Vec<f32> = (0..n).map(|i| (n - i) as f32).collect();
    let mut h_c_gpu: Vec<f32> = vec![0.0; n]; // For GPU result
    let mut h_c_cpu: Vec<f32> = vec![0.0; n]; // For CPU verification

    // 2. Allocate Device Memory
    info!("Allocating device memory (d_A, d_B, d_C)");
    let mut d_a: hipDeviceptr_t = ptr::null_mut();
    let mut d_b: hipDeviceptr_t = ptr::null_mut();
    let mut d_c: hipDeviceptr_t = ptr::null_mut();

    // Use scopeguard for resource cleanup
    let mut _d_a_guard = scopeguard::guard(d_a, |ptr| {
        if !ptr.is_null() {
            debug!("Freeing d_A");
            hip_call!(hipFree(ptr)).expect("Failed to free d_A");
        }
    });
    let mut _d_b_guard = scopeguard::guard(d_b, |ptr| {
        if !ptr.is_null() {
            debug!("Freeing d_B");
            hip_call!(hipFree(ptr)).expect("Failed to free d_B");
        }
    });
    let mut _d_c_guard = scopeguard::guard(d_c, |ptr| {
        if !ptr.is_null() {
            debug!("Freeing d_C");
            hip_call!(hipFree(ptr)).expect("Failed to free d_C");
        }
    });
    
    hip_call!(hipMalloc(&mut d_a as *mut _ as *mut *mut c_void, data_size))
        .context("Failed to allocate device memory for A")?;
    *_d_a_guard = d_a; // Update guard with actual pointer
    hip_call!(hipMalloc(&mut d_b as *mut _ as *mut *mut c_void, data_size))
        .context("Failed to allocate device memory for B")?;
    *_d_b_guard = d_b; // Update guard
    hip_call!(hipMalloc(&mut d_c as *mut _ as *mut *mut c_void, data_size))
        .context("Failed to allocate device memory for C")?;
    *_d_c_guard = d_c; // Update guard

    // 3. Copy Data from Host to Device (H2D)
    info!("Copying data from host to device (H2D)");
    hip_call!(hipMemcpy(
        d_a,
        h_a.as_ptr() as *const c_void,
        data_size,
        hipMemcpyKind::hipMemcpyHostToDevice
    ))
    .context("Failed H2D memcpy for A")?;

    hip_call!(hipMemcpy(
        d_b,
        h_b.as_ptr() as *const c_void,
        data_size,
        hipMemcpyKind::hipMemcpyHostToDevice
    ))
    .context("Failed H2D memcpy for B")?;

    // 4. Load HIP Kernel Module
    info!("Loading HIP kernel module from: {}", KERNEL_PATH);
    if !Path::new(KERNEL_PATH).exists() {
        return Err(anyhow!("Kernel file not found: {}. Please compile vectorAdd_kernel.hip to .co using hipcc.", KERNEL_PATH));
    }
    let mut module: hipModule_t = ptr::null_mut();
    // hipModuleLoadData requires a buffer of the .co file content
    let image = std::fs::read(KERNEL_PATH)
        .with_context(|| format!("Failed to read kernel file: {}", KERNEL_PATH))?;
    
    hip_call!(hipModuleLoadData(&mut module, image.as_ptr() as *const c_void))
        .context("Failed to load HIP module")?;
    let _module_guard = scopeguard::guard(module, |m| {
        if !m.is_null() {
            debug!("Unloading HIP module");
            hip_call!(hipModuleUnload(m)).expect("Failed to unload module");
        }
    });


    // 5. Get Kernel Function from Module
    info!("Getting kernel function 'vectorAdd' from module");
    let mut function: hipFunction_t = ptr::null_mut();
    let kernel_name = CString::new("vectorAdd").expect("CString::new failed for kernel name");
    hip_call!(hipModuleGetFunction(
        &mut function,
        module,
        kernel_name.as_ptr()
    ))
    .context("Failed to get kernel function from module")?;

    // 6. Setup Kernel Arguments
    // Kernel signature: extern "C" __global__ void vectorAdd(const float* A, const float* B, float* C, int N)
    // Arguments need to be an array of pointers to the actual arguments.
    let mut args: Vec<*mut c_void> = vec![
        &mut d_a as *mut _ as *mut c_void, // &A
        &mut d_b as *mut _ as *mut c_void, // &B
        &mut d_c as *mut _ as *mut c_void, // &C
        &mut (n as i32) as *mut _ as *mut c_void, // &N (passed as i32)
    ];
    // The hipModuleLaunchKernel expects a *mut [*mut c_void], which is a raw pointer to the slice's data.
    let mut kernel_params: Vec<*mut c_void> = args.iter_mut().map(|arg| *arg).collect();


    // 7. Launch Kernel
    let grid_dim = (n + THREADS_PER_BLOCK - 1) / THREADS_PER_BLOCK;
    info!(
        "Launching 'vectorAdd' kernel with gridDim={} blockDim={}",
        grid_dim, THREADS_PER_BLOCK
    );
    let launch_start_time = Instant::now();
    hip_call!(hipModuleLaunchKernel(
        function,
        grid_dim as u32, // gridDimX
        1,                // gridDimY
        1,                // gridDimZ
        THREADS_PER_BLOCK as u32, // blockDimX
        1,                // blockDimY
        1,                // blockDimZ
        0,                // sharedMemBytes
        ptr::null_mut(),  // stream (0 for default stream)
        kernel_params.as_mut_ptr(), // kernelParams
        ptr::null_mut()   // extra
    ))
    .context("Failed to launch kernel")?;

    // 8. Synchronize Device
    hip_call!(hipDeviceSynchronize()).context("Failed hipDeviceSynchronize")?;
    let launch_duration = launch_start_time.elapsed();
    info!("Kernel execution time: {:?}", launch_duration);

    // 9. Copy Data from Device to Host (D2H)
    info!("Copying results from device to host (D2H)");
    hip_call!(hipMemcpy(
        h_c_gpu.as_mut_ptr() as *mut c_void,
        d_c,
        data_size,
        hipMemcpyKind::hipMemcpyDeviceToHost
    ))
    .context("Failed D2H memcpy for C")?;

    // 10. Verify Results
    info!("Verifying GPU results against CPU computation...");
    let cpu_start_time = Instant::now();
    for i in 0..n {
        h_c_cpu[i] = h_a[i] + h_b[i];
    }
    let cpu_duration = cpu_start_time.elapsed();
    info!("CPU computation time: {:?}", cpu_duration);

    let mut correct_results = 0;
    for i in 0..n {
        if (h_c_gpu[i] - h_c_cpu[i]).abs() < 1e-5 { // Tolerance for f32 comparison
            correct_results += 1;
        }
    }

    if correct_results == n {
        info!("Test PASSED: All {} results are correct.", n);
    } else {
        error!(
            "Test FAILED: {} out of {} results are incorrect.",
            n - correct_results,
            n
        );
        // Optionally print some differing values
        for i in 0..n {
            if (h_c_gpu[i] - h_c_cpu[i]).abs() >= 1e-5 {
                error!("Diff at index {}: GPU={}, CPU={}", i, h_c_gpu[i], h_c_cpu[i]);
                if i > 20 { break; } // Print only a few diffs
            }
        }
        return Err(anyhow!("Verification failed."));
    }
    
    // Resources d_a, d_b, d_c, and module are cleaned up by scope guards when they go out of scope.
    info!("rust_vector_add application completed successfully.");
    Ok(())
}
