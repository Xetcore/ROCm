use anyhow::{Context, Result, anyhow};
use log::{info, warn, debug};
use std::fs;
use std::path::Path;

use crate::config_smi::RocmSmiConfig;

// Helper to remove files based on a glob pattern within a directory
fn remove_files_glob(dir: &Path, pattern: &str, description: &str) -> Result<()> {
    let glob_path = dir.join(pattern);
    debug!("Searching for files to remove in {} with pattern: {}", dir.display(), glob_path.display());
    match glob::glob(&glob_path.to_string_lossy()) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(path) => {
                        if path.is_file() {
                            info!("Removing {} file: {}", description, path.display());
                            fs::remove_file(&path).with_context(|| format!("Failed to remove {} file: {}", description, path.display()))?;
                        } else if path.is_dir() {
                            // Should not happen for typical library file patterns, but handle defensively
                            warn!("Expected a file but found directory at {}, skipping removal for pattern '{}'", path.display(), pattern);
                        }
                    }
                    Err(e) => warn!("Error matching glob entry for {}: {}", description, e),
                }
            }
        }
        Err(e) => return Err(anyhow!("Invalid glob pattern for {}: {}", description, pattern).context(e)),
    }
    Ok(())
}


pub fn run_clean(config: &RocmSmiConfig) -> Result<()> {
    info!("Starting clean process for 'rocm-smi-lib' library...");

    // 1. Clean build directory for 'rocm-smi-lib'
    if config.build_dir_smi.exists() {
        info!("Removing build directory for 'rocm-smi-lib': {}", config.build_dir_smi.display());
        fs::remove_dir_all(&config.build_dir_smi)
            .with_context(|| format!("Failed to remove build directory for 'rocm-smi-lib': {}", config.build_dir_smi.display()))?;
    } else {
        info!("Build directory for 'rocm-smi-lib' not found, skipping: {}", config.build_dir_smi.display());
    }

    // 2. Clean package directory for 'rocm-smi-lib'
    if config.package_dir_smi.exists() {
        info!("Removing package directory for 'rocm-smi-lib': {}", config.package_dir_smi.display());
        fs::remove_dir_all(&config.package_dir_smi)
            .with_context(|| format!("Failed to remove package directory for 'rocm-smi-lib': {}", config.package_dir_smi.display()))?;
    } else {
        info!("Package directory for 'rocm-smi-lib' not found, skipping: {}", config.package_dir_smi.display());
    }
    
    // 3. Clean installed library files from ROCM_PATH
    // This needs to be careful and specific.
    // Original script logic:
    // rm -rf $ROCM_PATH/lib*/lib*rocm_smi*
    // rm -rf $ROCM_PATH/include/rocm_smi
    // rm -rf $ROCM_PATH/.info/rocm-smi* (if applicable, not explicitly in this script but common)

    let lib_dir_64 = config.rocm_path.join("lib64");
    let lib_dir_32 = config.rocm_path.join("lib"); // Assuming 'lib' for 32-bit per config
    let include_dir = config.rocm_path.join("include/rocm_smi");
    let info_dir_pattern = config.rocm_path.join(".info/rocm-smi*"); // Example, adjust if needed

    // Remove from lib64
    if lib_dir_64.exists() {
        remove_files_glob(&lib_dir_64, "lib*rocm_smi*", "rocm-smi-lib (64-bit)")?;
    } else {
        info!("Directory {} not found, skipping cleanup of 64-bit libs.", lib_dir_64.display());
    }

    // Remove from lib (for 32-bit installs or if used by 64-bit on some systems)
    if lib_dir_32.exists() && lib_dir_32 != lib_dir_64 { // Avoid double processing if lib == lib64
        remove_files_glob(&lib_dir_32, "lib*rocm_smi*", "rocm-smi-lib (32-bit/common)")?;
    } else if lib_dir_32 != lib_dir_64 {
        info!("Directory {} not found, skipping cleanup of 32-bit/common libs.", lib_dir_32.display());
    }
    
    // Remove include directory
    if include_dir.exists() {
        info!("Removing include directory: {}", include_dir.display());
        fs::remove_dir_all(&include_dir)
            .with_context(|| format!("Failed to remove include directory: {}", include_dir.display()))?;
    } else {
        info!("Include directory {} not found, skipping cleanup.", include_dir.display());
    }

    // Remove .info files (if this convention is used)
    // This is a more general glob on the file name itself
    let base_info_dir = config.rocm_path.join(".info");
    if base_info_dir.exists() {
        match glob::glob(&info_dir_pattern.to_string_lossy()) {
             Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(path) => {
                            if path.is_file() {
                                info!("Removing .info file: {}", path.display());
                                fs::remove_file(&path).with_context(|| format!("Failed to remove .info file: {}", path.display()))?;
                            } else if path.is_dir() { // If .info entries can be directories
                                info!("Removing .info directory: {}", path.display());
                                fs::remove_dir_all(&path).with_context(|| format!("Failed to remove .info directory: {}", path.display()))?;
                            }
                        }
                        Err(e) => warn!("Error matching .info glob entry: {}", e),
                    }
                }
            }
            Err(e) => warn!("Invalid .info glob pattern '{}': {}", info_dir_pattern.display(), e),
        }
    } else {
        info!("Directory {} not found, skipping cleanup of .info files.", base_info_dir.display());
    }


    info!("Clean process for 'rocm-smi-lib' library completed.");
    Ok(())
}
