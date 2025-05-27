use anyhow::{Result, Context}; // Added Context
use log::{info, error, warn};

// Ensure ffi module is declared so it's compiled and its types are available to safe_hip_wrappers if needed.
// However, ffi types are typically used through `crate::ffi::*` in safe_hip_wrappers.
mod ffi;
mod safe_hip_wrappers;

use safe_hip_wrappers::{
    get_device_count,
    get_device_properties, // This returns ffi::hipDeviceProp_t
    get_driver_version,
    get_runtime_version,
    DeviceProperties, // This is the Rust-friendly struct
};

fn main() -> Result<()> {
    // Initialize env_logger. RUST_LOG environment variable can control logging levels (e.g., info, debug).
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting HIP device query application...");

    // 1. Get Driver and Runtime Versions
    match get_driver_version() {
        Ok(version) => info!("HIP Driver Version: {}", version),
        Err(e) => warn!("Could not retrieve HIP driver version: {}. (This can be normal if only runtime is used or in certain environments such as containers without full driver components accessible).", e),
    }

    match get_runtime_version() {
        Ok(version) => info!("HIP Runtime Version: {}", version),
        Err(e) => {
            // Runtime version is generally expected to be available.
            error!("Failed to get HIP runtime version: {}", e);
            error!("This might indicate a problem with the HIP/ROCm installation or environment setup (e.g., LD_LIBRARY_PATH).");
        }
    }

    // 2. Get the number of HIP-capable devices
    let device_count = match get_device_count() {
        Ok(count) => {
            info!("Found {} HIP device(s).", count);
            if count == 0 {
                info!("No HIP devices found. Ensure ROCm is installed correctly and GPUs are available.");
                return Ok(()); // Exit gracefully if no devices are found.
            }
            count
        }
        Err(e) => {
            error!("Critical: Failed to get HIP device count: {}", e);
            error!(
                "Please ensure that the ROCm environment is set up correctly: \
                drivers are installed, user has permissions to access GPU devices, \
                and LD_LIBRARY_PATH might need to include the ROCm library path (e.g., /opt/rocm/lib)."
            );
            return Err(e); // For critical errors like this, exiting with an error is appropriate.
        }
    };

    // 3. Iterate through each device and print its properties
    println!("\n========== HIP Device Information ==========");
    for i in 0..device_count {
        info!("Querying properties for device ID: {}", i);
        match get_device_properties(i) {
            Ok(c_props) => { // c_props is of type ffi::hipDeviceProp_t
                // Convert the C struct to our Rust-friendly struct using TryFrom
                match DeviceProperties::try_from((i, c_props)) {
                    Ok(rust_props) => {
                        println!("\n--- Device ID: {} ---", rust_props.device_id);
                        println!("  Name:                     {}", rust_props.name);
                        println!("  Total Global Memory:      {:.2} GB ({:.0} MB, {} bytes)",
                            rust_props.total_global_mem as f64 / (1024.0 * 1024.0 * 1024.0), // GB
                            rust_props.total_global_mem as f64 / (1024.0 * 1024.0),      // MB
                            rust_props.total_global_mem                                 // Bytes
                        );
                        println!("  Compute Capability:       {}.{}", rust_props.compute_major, rust_props.compute_minor);
                        println!("  GCN Architecture Name:    {}", rust_props.gcn_arch_name);
                        println!("  PCI Bus ID:               {}", rust_props.pci_bus_id);
                        println!("  PCI Device ID:            {}", rust_props.pci_device_id);
                        println!("  Max Threads Per Block:    {}", rust_props.max_threads_per_block);
                    }
                    Err(e) => {
                        error!("Failed to convert properties for device {}: {}", i, e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to get properties for device {}: {}", i, e);
            }
        }
    }
    println!("\n==========================================");
    info!("HIP device query finished.");

    Ok(())
}
