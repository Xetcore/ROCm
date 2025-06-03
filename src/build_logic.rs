use anyhow::{Context, Result, anyhow};
use log::{info, warn, debug};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use std::env::{self, JoinPathsError}; // Modified to bring JoinPathsError into scope
use std::ffi::OsString; // Added for OsString type

use crate::config::Config;
use crate::utils::{run_command, find_cmake_projects, is_package_selected, SelectedPurpose};

fn run_cmake_configure(config: &Config, project_path: &Path, build_dir: &Path) -> Result<()> {
    let cmake_source_dir = project_path;
    let mut cmake_cmd = Command::new("cmake");
    cmake_cmd.arg(format!("-S{}", cmake_source_dir.display()));
    cmake_cmd.arg(format!("-B{}", build_dir.display()));
    cmake_cmd.arg(format!("-DCMAKE_BUILD_TYPE={}", config.build_type));

    if let Some(install_dir) = &config.install_dir {
        cmake_cmd.arg(format!("-DCMAKE_INSTALL_PREFIX={}", install_dir.display()));
    }

    // Add rocm-cmake path for other projects to find it
    // This assumes that rocm-cmake itself doesn't need this when being built.
    if project_path != config.rocm_cmake_path {
        if let Some(rocm_cmake_install_path) = config.get_package_install_dir("rocm-cmake") {
            if rocm_cmake_install_path.is_dir() {
                debug!("Found rocm-cmake install path at: {}. Attempting to prepend to CMAKE_PREFIX_PATH for configuring {}.", rocm_cmake_install_path.display(), project_path.display());
                match generate_updated_cmake_prefix_path(&rocm_cmake_install_path, env::var_os("CMAKE_PREFIX_PATH")) {
                    Ok(new_cmake_prefix_path_osstring) => {
                        cmake_cmd.env("CMAKE_PREFIX_PATH", new_cmake_prefix_path_osstring);
                        // To log the path effectively, we'd ideally inspect cmake_cmd.get_envs(),
                        // but let's assume generate_updated_cmake_prefix_path logs sufficiently.
                        // The debug log from generate_updated_cmake_prefix_path shows the generated list.
                        // For the command itself:
                        if log::log_enabled!(log::Level::Debug) {
                             if let Some(val) = cmake_cmd.get_envs().get("CMAKE_PREFIX_PATH") {
                                debug!("For project '{}', CMAKE_PREFIX_PATH will be set to: {:?}", project_path.display(), val);
                             } else {
                                debug!("For project '{}', CMAKE_PREFIX_PATH was prepared but seems unset in command. This is unexpected.", project_path.display());
                             }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to construct new CMAKE_PREFIX_PATH for project '{}': {}. Project may not find rocm-cmake.", project_path.display(), e);
                    }
                }
            } else {
                warn!("Configured rocm-cmake install path '{}' does not exist or is not a directory. Project {} may not find rocm-cmake.", rocm_cmake_install_path.display(), project_path.display());
            }
        } else {
            debug!("No specific rocm-cmake install directory configured (config.install_dir is None). Project {} will rely on global CMake paths to find rocm-cmake.", project_path.display());
        }
    }

    for arg in &config.cmake_args {
        cmake_cmd.arg(arg);
    }

    info!(
        "Configuring project: {} with build dir: {}",
        project_path.file_name().unwrap_or_default().to_string_lossy(),
        build_dir.display()
    );
    debug!("CMake configure command: {:?}", cmake_cmd);

    run_command(cmake_cmd, "CMake configuration")
}

fn run_cmake_build(build_dir: &Path, config: &Config) -> Result<()> {
    let mut cmake_cmd = Command::new("cmake");
    cmake_cmd.arg("--build").arg(build_dir);
    cmake_cmd.arg("--config").arg(&config.build_type);
    if let Some(job_count) = config.jobs {
        cmake_cmd.arg("--parallel").arg(job_count.to_string());
    }

    info!("Building project in: {}", build_dir.display());
    debug!("CMake build command: {:?}", cmake_cmd);
    run_command(cmake_cmd, "CMake build")
}

fn run_cmake_install(build_dir: &Path, config: &Config) -> Result<()> {
    let mut cmake_cmd = Command::new("cmake");
    cmake_cmd.arg("--install").arg(build_dir);
    cmake_cmd.arg("--config").arg(&config.build_type);

    info!("Installing project from: {}", build_dir.display());
    debug!("CMake install command: {:?}", cmake_cmd);
    run_command(cmake_cmd, "CMake install")
}

pub fn run_build(config: &Config) -> Result<()> {
    info!("Starting build process...");

    // 1. Build rocm-cmake first if it's implicitly or explicitly targeted
    //    or if no specific packages are given (build all)
    let build_rocm_cmake_first = config.packages.is_empty() ||
                               config.packages.iter().any(|p| p == "rocm-cmake" || p == "rocm-cmake/");
    
    if build_rocm_cmake_first {
        info!("Processing rocm-cmake first...");
        let rocm_cmake_build_dir = config.get_package_build_dir("rocm-cmake");
        fs::create_dir_all(&rocm_cmake_build_dir).with_context(|| format!("Failed to create build directory for rocm-cmake: {}", rocm_cmake_build_dir.display()))?;

        run_cmake_configure(config, &config.rocm_cmake_path, &rocm_cmake_build_dir)
            .with_context(|| format!("CMake configuration failed for rocm-cmake at {}", config.rocm_cmake_path.display()))?;
        run_cmake_build(&rocm_cmake_build_dir, config)
            .with_context(|| format!("CMake build failed for rocm-cmake in {}", rocm_cmake_build_dir.display()))?;
        if config.install_dir.is_some() {
            // Install rocm-cmake to its own subdir within the main install_dir
            let rocm_cmake_install_dir_specific = config.get_package_install_dir("rocm-cmake");
             if let Some(idir) = rocm_cmake_install_dir_specific {
                let mut install_cmd = Command::new("cmake");
                install_cmd.arg("--install").arg(&rocm_cmake_build_dir);
                install_cmd.arg("--config").arg(&config.build_type);
                install_cmd.arg("--prefix").arg(idir); // Install rocm-cmake to its own folder
                info!("Installing rocm-cmake to: {}", idir.display());
                debug!("CMake install command for rocm-cmake: {:?}", install_cmd);
                run_command(install_cmd, "CMake install for rocm-cmake")?;
            }
        }
        info!("rocm-cmake processed successfully.");
    } else {
        info!("Skipping rocm-cmake build as it's not explicitly targeted and other packages are specified.");
        // Ensure the build directory for rocm-cmake exists if other projects need it.
        // This is crucial if rocm-cmake was pre-built or is available through other means.
        let rocm_cmake_build_path = config.rocm_cmake_path.join("build");
        if !rocm_cmake_build_path.exists() {
             warn!("rocm-cmake build directory ({}) not found. Other projects might fail if they depend on it.", rocm_cmake_build_path.display());
        }
    }


    // 2. Find other CMake projects in the source directory (excluding rocm-cmake itself)
    let projects = find_cmake_projects(
        &config.source_dir,
        Some(&config.rocm_cmake_path),
        config.project_search_depth,
    )?;
    if projects.is_empty() && config.packages.iter().any(|p| p != "rocm-cmake") {
         warn!("No other CMake projects found in {}", config.source_dir.display());
    } else if projects.is_empty() && config.packages.is_empty() {
        info!("No other CMake projects found besides rocm-cmake. Build process might be targeting rocm-cmake only or the source directory is empty.");
    }


    for project_path in projects {
        let project_name = project_path.file_name().unwrap_or_default().to_string_lossy().to_string();

        if !is_package_selected(&config.packages, &project_name, SelectedPurpose::Build) {
            info!("Skipping project {} as it's not in the selected packages for build.", project_name);
            continue;
        }
        
        info!("Processing project: {}", project_name);
        let project_build_dir = config.get_package_build_dir(&project_name);
        fs::create_dir_all(&project_build_dir).with_context(|| format!("Failed to create build directory for {}: {}", project_name, project_build_dir.display()))?;

        run_cmake_configure(config, &project_path, &project_build_dir)
            .with_context(|| format!("CMake configuration failed for {} at {}", project_name, project_path.display()))?;
        run_cmake_build(&project_build_dir, config)
            .with_context(|| format!("CMake build failed for {} in {}", project_name, project_build_dir.display()))?;

        if config.install_dir.is_some() {
            run_cmake_install(&project_build_dir, config)
                .with_context(|| format!("CMake install failed for {} from {}", project_name, project_build_dir.display()))?;
        }
        info!("Successfully processed project: {}", project_name);
    }

    if config.packages.is_empty() && !build_rocm_cmake_first && projects.is_empty() {
        info!("No packages specified and no projects found (rocm-cmake was not built in this run). Nothing to do.");
    } else {
        info!("Build process completed.");
    }
    Ok(())
}

// Helper function to generate an updated CMAKE_PREFIX_PATH string
pub(crate) fn generate_updated_cmake_prefix_path(
    new_path_to_prepend: &PathBuf,
    current_cmake_prefix_path_os: Option<OsString>,
) -> Result<OsString, JoinPathsError> {
    let mut paths: Vec<PathBuf> = current_cmake_prefix_path_os
        .map(|val| env::split_paths(&val).collect())
        .unwrap_or_else(Vec::new);

    // Prepend new_path_to_prepend, ensuring no duplicates
    if !paths.contains(new_path_to_prepend) {
        paths.insert(0, new_path_to_prepend.clone());
        debug!("Prepending '{}' to CMAKE_PREFIX_PATH.", new_path_to_prepend.display());
    } else {
        debug!("'{}' is already in CMAKE_PREFIX_PATH. No change needed to its position by this operation.", new_path_to_prepend.display());
    }

    let new_path_list = env::join_paths(paths.iter())?;
    debug!("Generated new CMAKE_PREFIX_PATH list: {:?}", new_path_list);
    Ok(new_path_list)
}

#[cfg(test)]
mod tests {
    use super::generate_updated_cmake_prefix_path;
    use std::env;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn test_generate_prefix_path_initial_none() {
        let new_path = PathBuf::from("/opt/rocm-cmake");
        let result = generate_updated_cmake_prefix_path(&new_path, None).unwrap();
        assert_eq!(result, OsString::from("/opt/rocm-cmake"));
    }

    #[test]
    fn test_generate_prefix_path_prepend_existing() {
        let new_path = PathBuf::from("/opt/rocm-cmake");
        let existing_paths_os = OsString::from("/usr/local");
        let result = generate_updated_cmake_prefix_path(&new_path, Some(existing_paths_os.clone())).unwrap();

        let expected_paths = vec![PathBuf::from("/opt/rocm-cmake"), PathBuf::from("/usr/local")];
        let expected_os = env::join_paths(expected_paths).unwrap();
        assert_eq!(result, expected_os);
    }

    #[test]
    fn test_generate_prefix_path_prepend_multiple_existing() {
        let new_path = PathBuf::from("/opt/rocm-cmake");
        let existing_paths_vec = vec![PathBuf::from("/usr/local"), PathBuf::from("/opt/other")];
        let existing_paths_os = env::join_paths(existing_paths_vec.iter()).unwrap();

        let result = generate_updated_cmake_prefix_path(&new_path, Some(existing_paths_os.clone())).unwrap();

        let expected_paths_vec = vec![
            PathBuf::from("/opt/rocm-cmake"),
            PathBuf::from("/usr/local"),
            PathBuf::from("/opt/other"),
        ];
        let expected_os = env::join_paths(expected_paths_vec).unwrap();
        assert_eq!(result, expected_os);
    }

    #[test]
    fn test_generate_prefix_path_no_duplicate_prepend() {
        let new_path = PathBuf::from("/opt/rocm-cmake");
        let existing_paths_vec = vec![PathBuf::from("/usr/local"), PathBuf::from("/opt/rocm-cmake")];
        let existing_paths_os = env::join_paths(existing_paths_vec.iter()).unwrap();

        let result = generate_updated_cmake_prefix_path(&new_path, Some(existing_paths_os.clone())).unwrap();

        // If new_path is already present, it should not be prepended again,
        // and its original position should be maintained by current logic.
        assert_eq!(result, existing_paths_os);
    }

    #[test]
    fn test_generate_prefix_path_empty_new_path_os_existing() {
        // Test with an empty new_path (though function expects &PathBuf, so it can't be "empty" in PathBuf sense)
        // This test is more about ensuring behavior with empty existing path.
        let new_path = PathBuf::from("/opt/rocm-cmake");
        let empty_existing_path = OsString::from("");
        let result = generate_updated_cmake_prefix_path(&new_path, Some(empty_existing_path)).unwrap();
        // split_paths on "" yields a single empty PathBuf.
        // join_paths on ["/opt/rocm-cmake", ""] (or just ["/opt/rocm-cmake"] if "" is filtered,
        // which it isn't by split_paths)
        // PathBuf::from("") is a valid path.
        // Let's check actual behavior of split_paths and join_paths with empty strings.
        // env::split_paths("") -> yields one empty path `PathBuf::new("")`
        // So paths becomes `vec![PathBuf::new("")`
        // Then we prepend, so `vec![PathBuf::from("/opt/rocm-cmake"), PathBuf::new("")`
        let expected_paths = vec![PathBuf::from("/opt/rocm-cmake"), PathBuf::new("")];
        let expected_os = env::join_paths(expected_paths).unwrap();
        assert_eq!(result, expected_os);
    }

    #[test]
    fn test_generate_prefix_path_new_path_is_empty_string() {
        // PathBuf::from("") is a valid, though unusual, path.
        let new_path = PathBuf::from("");
        let existing_paths_os = OsString::from("/usr/local");
        let result = generate_updated_cmake_prefix_path(&new_path, Some(existing_paths_os.clone())).unwrap();

        // new_path ("") should be prepended if not already there.
        // Assuming "" is not in ["/usr/local"]
        let expected_paths = vec![PathBuf::from(""), PathBuf::from("/usr/local")];
        let expected_os = env::join_paths(expected_paths).unwrap();
        assert_eq!(result, expected_os);
    }
}
