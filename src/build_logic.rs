use anyhow::{Context, Result, anyhow};
use log::{info, warn, debug};
use std::path::Path;
use std::process::Command;
use std::fs;

use crate::config::Config;
use crate::utils::{run_command, find_cmake_projects, copy_if_selected, SelectedPurpose};

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
         cmake_cmd.arg(format!("-Drocm-cmake_DIR={}", config.rocm_cmake_path.join("build").display()));
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
    cmake_cmd.arg("--").arg("-j"); // Parallel build

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
    let projects = find_cmake_projects(&config.source_dir, Some(&config.rocm_cmake_path))?;
    if projects.is_empty() && config.packages.iter().any(|p| p != "rocm-cmake") {
         warn!("No other CMake projects found in {}", config.source_dir.display());
    } else if projects.is_empty() && config.packages.is_empty() {
        info!("No other CMake projects found besides rocm-cmake. Build process might be targeting rocm-cmake only or the source directory is empty.");
    }


    for project_path in projects {
        let project_name = project_path.file_name().unwrap_or_default().to_string_lossy().to_string();

        if !copy_if_selected(&config.packages, &project_name, SelectedPurpose::Build) {
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
