use anyhow::{Context, Result};
use log::{info, warn, debug}; // Added debug
use std::fs;
use std::path::PathBuf; // Added PathBuf
use std::collections::HashSet; // Added HashSet

use crate::config::Config;
use crate::utils::find_cmake_projects;

pub fn run_clean(config: &Config) -> Result<()> {
    info!("Starting clean process...");

    // Determine which packages to clean
    let mut all_found_project_paths_set: HashSet<PathBuf> = HashSet::new();
    info!("Discovering projects for cleaning from specified source directories...");
    for src_dir in &config.source_dirs {
        if !src_dir.is_dir() {
            warn!("Source directory {} for cleaning does not exist or is not a directory. Skipping.", src_dir.display());
            continue;
        }
        debug!("Searching for projects to clean in source directory: {}", src_dir.display());
        match find_cmake_projects(
            src_dir,
            None, // For cleaning, do not exclude rocm-cmake from discovery
            config.project_search_depth,
        ) {
            Ok(found_in_src_dir) => {
                for project_path in found_in_src_dir {
                    all_found_project_paths_set.insert(project_path);
                }
            }
            Err(e) => {
                warn!("Failed to find projects for cleaning in source directory {}: {}. Skipping this directory.", src_dir.display(), e);
            }
        }
    }

    let all_projects_in_src: Vec<String> = all_found_project_paths_set
        .iter()
        .map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned())
        .collect();
    
    let mut packages_to_clean = Vec::new();
    if config.packages.is_empty() {
        // If no specific packages, clean all found projects + rocm-cmake
        packages_to_clean = all_projects_in_src;
        if !packages_to_clean.iter().any(|p| p == "rocm-cmake") {
             packages_to_clean.push("rocm-cmake".to_string()); // Ensure rocm-cmake is included
        }
    } else {
        // Clean only specified packages
        for pkg_name in &config.packages {
            // Normalize package name (e.g., remove trailing slash)
            let normalized_name = pkg_name.trim_end_matches('/').to_string();
            if all_projects_in_src.contains(&normalized_name) || normalized_name == "rocm-cmake" {
                if !packages_to_clean.contains(&normalized_name) {
                    packages_to_clean.push(normalized_name);
                }
            } else {
                warn!("Specified package '{}' not found as a CMake project in source or as 'rocm-cmake'. Skipping its clean.", pkg_name);
            }
        }
    }
    
    if packages_to_clean.is_empty() && !config.packages.is_empty() {
        warn!("No valid packages selected for cleaning based on input and found projects.");
        return Ok(());
    } else if packages_to_clean.is_empty() {
        info!("No packages found to clean.");
        return Ok(());
    }

    info!("Targeting the following packages for cleaning: {:?}", packages_to_clean);

    for package_name in &packages_to_clean {
        // Clean build directory for the package
        let package_build_dir = config.get_package_build_dir(package_name);
        if package_build_dir.exists() {
            info!("Removing build directory for {}: {}", package_name, package_build_dir.display());
            fs::remove_dir_all(&package_build_dir)
                .with_context(|| format!("Failed to remove build directory for {}: {}", package_name, package_build_dir.display()))?;
        } else {
            info!("Build directory for {} not found, skipping: {}", package_name, package_build_dir.display());
        }

        // Clean install directory for the package (if install_dir is set)
        if let Some(package_install_dir) = config.get_package_install_dir(package_name) {
            if package_install_dir.exists() {
                info!("Removing install directory for {}: {}", package_name, package_install_dir.display());
                fs::remove_dir_all(&package_install_dir)
                    .with_context(|| format!("Failed to remove install directory for {}: {}", package_name, package_install_dir.display()))?;
            } else {
                info!("Install directory for {} not found, skipping: {}", package_name, package_install_dir.display());
            }
        }
    }
    
    // If config.packages is empty (meaning clean all), also try to remove the top-level build_dir and install_dir
    // but only if they are empty after cleaning individual packages.
    if config.packages.is_empty() {
        if config.build_dir.exists() && fs::read_dir(&config.build_dir)?.next().is_none() {
            info!("Removing top-level build directory: {}", config.build_dir.display());
            fs::remove_dir(&config.build_dir).with_context(|| format!("Failed to remove top-level build directory {}", config.build_dir.display()))?;
        }
        if let Some(ref idir) = config.install_dir {
            if idir.exists() && fs::read_dir(idir)?.next().is_none() {
                info!("Removing top-level install directory: {}", idir.display());
                fs::remove_dir(idir).with_context(|| format!("Failed to remove top-level install directory {}", idir.display()))?;
            }
        }
    }


    info!("Clean process completed.");
    Ok(())
}
