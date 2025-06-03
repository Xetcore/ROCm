use anyhow::{Context, Result, anyhow};
use log::{debug, error, info};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use glob::glob;

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
        Err(anyhow!("Command failed: {}", description))
    } else {
        info!("Successfully executed command for {}", description);
        Ok(())
    }
}

/// Identifies CMake-based project directories within a given source directory.
/// Optionally excludes a specific path (e.g., rocm-cmake itself).
pub fn find_cmake_projects(source_dir: &Path, exclude_path: Option<&Path>) -> Result<Vec<PathBuf>> {
    debug!("Searching for CMake projects in: {} (excluding {:?})", source_dir.display(), exclude_path);
    let mut projects = Vec::new();
    
    // Search for CMakeLists.txt in immediate subdirectories
    for entry in fs::read_dir(source_dir)
        .with_context(|| format!("Failed to read source directory: {}", source_dir.display()))? 
    {
        let entry = entry.with_context(|| "Failed to read directory entry")?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(ex_path) = exclude_path {
                if path == ex_path {
                    debug!("Skipping excluded path: {}", path.display());
                    continue;
                }
            }
            if path.join("CMakeLists.txt").is_file() {
                debug!("Found CMake project: {}", path.display());
                projects.push(path);
            } else {
                // Also check for common project subdirectories like 'src' if CMakeLists.txt is not in root
                // This is a simple heuristic and might need refinement
                let src_cmakelists = path.join("src/CMakeLists.txt");
                if src_cmakelists.is_file() {
                     debug!("Found CMake project (in src/): {}", path.display());
                     projects.push(path); // Add the parent directory of src/
                }
            }
        }
    }

    // Alternative: Use glob to find all CMakeLists.txt files and then get their parent directories.
    // This can be more flexible but might pick up CMakeLists.txt in unexpected places if not careful.
    // For now, sticking to direct subdirectories.
    // Example using glob:
    // let pattern = source_dir.join("**/CMakeLists.txt");
    // for entry in glob(&pattern.to_string_lossy())? {
    //     match entry {
    //         Ok(path) => {
    //             if let Some(parent_dir) = path.parent() {
    //                 if parent_dir != source_dir { // Exclude top-level CMakeLists.txt if any
    //                     if let Some(ex_path) = exclude_path {
    //                         if parent_dir == ex_path { continue; }
    //                     }
    //                     if !projects.contains(&parent_dir.to_path_buf()) {
    //                         projects.push(parent_dir.to_path_buf());
    //                     }
    //                 }
    //             }
    //         }
    //         Err(e) => error!("Glob error: {}", e),
    //     }
    // }


    if projects.is_empty() {
        info!("No CMake projects found in {}", source_dir.display());
    }
    Ok(projects)
}


#[derive(PartialEq, Eq, Debug)]
pub enum SelectedPurpose {
    Build,
    Clean,
    Outdir,
}

/// Checks if a package should be processed based on the user's selection.
/// If `selected_packages` is empty, it means "all packages" are implicitly selected.
/// Otherwise, only packages explicitly listed are selected.
pub fn is_package_selected(selected_packages: &[String], package_name: &str, purpose: SelectedPurpose) -> bool {
    if selected_packages.is_empty() {
        debug!("No specific packages selected for {:?}, processing all (including '{}').", purpose, package_name);
        return true; // No specific packages listed, so all are considered selected
    }
    let normalized_package_name = package_name.trim_end_matches('/');
    let is_selected = selected_packages.iter().any(|s| s.trim_end_matches('/') == normalized_package_name);
    if is_selected {
        debug!("Package '{}' is selected for {:?}.", package_name, purpose);
    } else {
        debug!("Package '{}' is NOT selected for {:?}.", package_name, purpose);
    }
    is_selected
}
