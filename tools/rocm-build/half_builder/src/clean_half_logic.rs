use anyhow::{Context, Result};
use log::info;
use std::fs;

use crate::config_half::HalfConfig;

pub fn run_clean(config: &HalfConfig) -> Result<()> {
    info!("Starting clean process for 'half' library...");

    // Clean build directory for 'half'
    if config.build_dir_half.exists() {
        info!("Removing build directory for 'half': {}", config.build_dir_half.display());
        fs::remove_dir_all(&config.build_dir_half)
            .with_context(|| format!("Failed to remove build directory for 'half': {}", config.build_dir_half.display()))?;
    } else {
        info!("Build directory for 'half' not found, skipping: {}", config.build_dir_half.display());
    }

    // Clean package directory for 'half'
    if config.package_dir_half.exists() {
        info!("Removing package directory for 'half': {}", config.package_dir_half.display());
        fs::remove_dir_all(&config.package_dir_half)
            .with_context(|| format!("Failed to remove package directory for 'half': {}", config.package_dir_half.display()))?;
    } else {
        info!("Package directory for 'half' not found, skipping: {}", config.package_dir_half.display());
    }
    
    // Note: This clean logic does not remove anything from the global ROCM_PATH (e.g., /opt/rocm)
    // as that's a system-level directory. It only cleans local build/package artifacts.

    info!("Clean process for 'half' library completed.");
    Ok(())
}
