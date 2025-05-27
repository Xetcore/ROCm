use crate::ffi;
use anyhow::{anyhow, Result, Context}; // Added Context for more descriptive errors
use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::os::raw::c_char;

/// A safe wrapper for `hipGetDeviceCount`.
/// Returns the number of HIP-capable devices.
pub fn get_device_count() -> Result<i32> {
    let mut count: i32 = 0;
    // SAFETY: hipGetDeviceCount expects a mutable pointer to an i32, which it will initialize.
    // `&mut count` provides a valid pointer.
    let result = unsafe { ffi::hipGetDeviceCount(&mut count) };
    if result != ffi::hipSuccess {
        // The `ffi::hipError_t` type should be displayable due to `derive_debug` and `rustified_enum`.
        return Err(anyhow!("hipGetDeviceCount failed with HIP error: {:?}", result));
    }
    Ok(count)
}

/// A safe wrapper for `hipGetDeviceProperties`.
/// Retrieves properties for the specified device ID.
pub fn get_device_properties(device_id: i32) -> Result<ffi::hipDeviceProp_t> {
    // Create an uninitialized hipDeviceProp_t.
    // Using MaybeUninit is the idiomatic way to handle FFI output parameters.
    let mut props = MaybeUninit::<ffi::hipDeviceProp_t>::uninit();
    // SAFETY: hipGetDeviceProperties expects a pointer to a hipDeviceProp_t struct
    // that it will fill. `props.as_mut_ptr()` provides this.
    // `device_id` must be a valid device index (0 to count-1).
    let result = unsafe { ffi::hipGetDeviceProperties(props.as_mut_ptr(), device_id) };
    if result != ffi::hipSuccess {
        return Err(anyhow!(
            "hipGetDeviceProperties for device {} failed with HIP error: {:?}",
            device_id,
            result
        ));
    }
    // SAFETY: If hipGetDeviceProperties returns hipSuccess, it guarantees
    // that the `props` struct has been initialized.
    let props = unsafe { props.assume_init() };
    Ok(props)
}

/// A safe wrapper for `hipDriverGetVersion`.
/// Retrieves the HIP driver version.
pub fn get_driver_version() -> Result<i32> {
    let mut version: i32 = 0;
    // SAFETY: hipDriverGetVersion expects a mutable pointer to an i32.
    let result = unsafe { ffi::hipDriverGetVersion(&mut version) };
    if result != ffi::hipSuccess {
        return Err(anyhow!("hipDriverGetVersion failed with HIP error: {:?}", result));
    }
    Ok(version)
}

/// A safe wrapper for `hipRuntimeGetVersion`.
/// Retrieves the HIP runtime version.
pub fn get_runtime_version() -> Result<i32> {
    let mut version: i32 = 0;
    // SAFETY: hipRuntimeGetVersion expects a mutable pointer to an i32.
    let result = unsafe { ffi::hipRuntimeGetVersion(&mut version) };
    if result != ffi::hipSuccess {
        return Err(anyhow!("hipRuntimeGetVersion failed with HIP error: {:?}", result));
    }
    Ok(version)
}


/// A Rust-friendly struct to hold a selected subset of device properties.
#[derive(Debug, Default)]
pub struct DeviceProperties {
    pub device_id: i32,
    pub name: String,
    pub total_global_mem: usize, // size_t in C, maps to usize in Rust
    pub compute_major: i32,
    pub compute_minor: i32,
    pub pci_bus_id: i32,
    pub pci_device_id: i32,
    pub max_threads_per_block: i32,
    pub gcn_arch_name: String, // From hipDeviceProp_t.gcnArchName
}

impl DeviceProperties {
    /// Helper to convert a NUL-terminated fixed-size C char array to a Rust String.
    /// The input `c_chars` is a slice representing the fixed-size array from the C struct.
    fn c_chars_to_string(c_chars: &[c_char]) -> Result<String> {
        // SAFETY: We are taking a pointer to the start of the char array.
        // CStr::from_ptr will read until the first NUL byte.
        // This is safe if the C API guarantees NUL termination within the array bounds.
        // hipDeviceProp_t fields like 'name' are fixed-size char arrays and are
        // typically NUL-terminated by the HIP runtime.
        let c_str = unsafe { CStr::from_ptr(c_chars.as_ptr()) };
        c_str.to_str()
            .map(String::from)
            .with_context(|| format!("Failed to convert C string (potentially invalid UTF-8): {:?}", c_str))
    }
}

/// Converts from a tuple of (device_id, ffi::hipDeviceProp_t) to the Rust-friendly DeviceProperties.
impl TryFrom<(i32, ffi::hipDeviceProp_t)> for DeviceProperties {
    type Error = anyhow::Error;

    fn try_from((id, c_props): (i32, ffi::hipDeviceProp_t)) -> Result<Self, Self::Error> {
        Ok(DeviceProperties {
            device_id: id,
            name: Self::c_chars_to_string(&c_props.name).context("Failed to parse device name")?,
            total_global_mem: c_props.totalGlobalMem as usize,
            compute_major: c_props.major,
            compute_minor: c_props.minor,
            pci_bus_id: c_props.pciBusID,
            pci_device_id: c_props.pciDeviceID,
            max_threads_per_block: c_props.maxThreadsPerBlock,
            gcn_arch_name: Self::c_chars_to_string(&c_props.gcnArchName).context("Failed to parse GCN arch name")?,
        })
    }
}
