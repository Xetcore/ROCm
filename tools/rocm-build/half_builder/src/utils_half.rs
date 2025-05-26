use anyhow::{Context, Result, anyhow};
use log::{debug, error, info};
// use std::path::{Path, PathBuf}; // Not directly used by run_command
use std::process::Command;
// use std::fs; // Not directly used by run_command
// use glob::glob; // Not directly used by run_command

/// Runs a command and checks its exit status.
pub fn run_command(mut command: Command, description: &str) -> Result<()> {
    debug!("Running command for {}: {:?}", description, command);
    let status = command
        .status()
        .with_context(|| format!("Failed to execute command for {}", description))?;

    if !status.success() {
        error!(
            "Command for {} failed with status: {}",
            description, status
        );
        Err(anyhow!("Command failed for {}: {}", description, status))
    } else {
        info!("Successfully executed command for {}", description);
        Ok(())
    }
}

// Note: `find_cmake_projects` and `copy_if_selected` from the generic builder's utils 
// are not strictly needed here as 'half' is a single, specific component.
// The `build_half_logic.rs` handles its specific CMake project path directly
// and `copy_packages` within that module handles its specific package output.
// If other utility functions specific to `half_builder` were needed, they would go here.
// For now, `run_command` is the primary utility.
