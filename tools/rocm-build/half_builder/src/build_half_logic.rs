use anyhow::{Context, Result, anyhow};
use log::{info, warn, debug};
use std::path::Path;
use std::process::Command;
use std::fs;
use glob::glob;
use num_cpus;

use crate::config_half::{HalfConfig, PackageTypeCopy};
use crate::utils_half::run_command;

fn run_cmake_configure(
    config: &HalfConfig,
    build_dir: &Path,
    is_release: bool,
    enable_address_sanitizer: bool,
    build_static_libs: bool,
) -> Result<()> {
    let cmake_source_dir = &config.half_src_dir;
    let mut cmake_cmd = Command::new("cmake");

    cmake_cmd.arg(format!("-S{}", cmake_source_dir.display()));
    cmake_cmd.arg(format!("-B{}", build_dir.display()));

    let build_type = if is_release { "Release" } else { "Debug" };
    cmake_cmd.arg(format!("-DCMAKE_BUILD_TYPE={}", build_type));

    // Standard ROCm flags
    cmake_cmd.arg(format!("-DCMAKE_INSTALL_PREFIX={}", config.rocm_path.display()));
    cmake_cmd.arg(format!("-DROCM_PATH={}", config.rocm_path.display()));

    // Half-specific options (if any, most are standard)
    // For 'half', it's a header-only library, but we still "install" it to the ROCm path.
    // The build process for header-only can be minimal, often just an install step.
    // However, it usually has a CMakeLists.txt that supports standard build options.

    if build_static_libs {
        cmake_cmd.arg("-DBUILD_SHARED_LIBS=OFF");
        cmake_cmd.arg("-DBUILD_STATIC_LIBS=ON"); // Explicitly if available
    } else {
        cmake_cmd.arg("-DBUILD_SHARED_LIBS=ON");
        cmake_cmd.arg("-DBUILD_STATIC_LIBS=OFF"); // Explicitly if available
    }

    if enable_address_sanitizer {
        // ASAN is typically for compiled code, might not apply directly to header-only 'half'
        // but tests might use it. Add if project supports it.
        warn!("AddressSanitizer requested for 'half'. This might not apply to a header-only library itself but could affect tests if built.");
        cmake_cmd.arg("-DENABLE_ASAN=ON"); // Example flag, actual flag might differ
    }
    
    // CPack configuration
    if !config.cpack_generator.is_empty() {
        cmake_cmd.arg(format!("-DCPACK_GENERATOR={}", config.cpack_generator));
    }
    cmake_cmd.arg(format!("-DCPACK_PACKAGE_VERSION_PATCH={}", config.rocm_patch_version));
    // For 'half', the package name is usually just 'half'.
    // If it needs the rocm-specific naming like hipBLAS ('hipblas'), adjust here.
    // cmake_cmd.arg("-DCPACK_PACKAGE_NAME=half"); // Usually derived by CMake from project name


    info!(
        "Configuring 'half' project from: {} with build dir: {}",
        cmake_source_dir.display(),
        build_dir.display()
    );
    debug!("CMake configure command for 'half': {:?}", cmake_cmd);

    run_command(cmake_cmd, "CMake configuration for 'half'")
}

fn run_cmake_build(build_dir: &Path) -> Result<()> {
    let mut cmake_cmd = Command::new("cmake");
    cmake_cmd.arg("--build").arg(build_dir);
    // CMAKE_BUILD_TYPE is set at configure time.
    // cmake_cmd.arg("--config").arg(if is_release { "Release" } else { "Debug" }); 
    let num_jobs = num_cpus::get();
    cmake_cmd.arg("--").arg("-j").arg(num_jobs.to_string());

    info!("Building 'half' project in: {} with {} jobs", build_dir.display(), num_jobs);
    debug!("CMake build command for 'half': {:?}", cmake_cmd);
    run_command(cmake_cmd, "CMake build for 'half'")
}

fn run_cmake_install(build_dir: &Path) -> Result<()> {
    let mut cmake_cmd = Command::new("cmake");
    cmake_cmd.arg("--install").arg(build_dir);
    // cmake_cmd.arg("--config").arg(if is_release { "Release" } else { "Debug" });

    info!("Installing 'half' project from: {}", build_dir.display());
    debug!("CMake install command for 'half': {:?}", cmake_cmd);
    run_command(cmake_cmd, "CMake install for 'half'")
}

fn run_cpack(build_dir: &Path) -> Result<()> {
    let mut cpack_cmd = Command::new("cpack");
    cpack_cmd.current_dir(build_dir); // CPack usually runs from the build directory
    cpack_cmd.arg("--config").arg("CPackConfig.cmake"); // Or CPackSourceConfig.cmake

    info!("Running CPack in: {}", build_dir.display());
    debug!("CPack command for 'half': {:?}", cpack_cmd);
    run_command(cpack_cmd, "CPack for 'half'")
}

fn copy_packages(config: &HalfConfig, package_type_filter: &PackageTypeCopy) -> Result<()> {
    let build_dir = &config.build_dir_half;
    let package_dest_dir = &config.package_dir_half;

    fs::create_dir_all(package_dest_dir).with_context(|| {
        format!(
            "Failed to create package destination directory: {}",
            package_dest_dir.display()
        )
    })?;

    let glob_pattern = build_dir.join(package_type_filter.as_str_for_glob());
    info!("Searching for packages in '{}' with pattern: {}", build_dir.display(), glob_pattern.display());

    let mut found_packages = false;
    for entry in glob(&glob_pattern.to_string_lossy())? {
        match entry {
            Ok(path) => {
                if path.is_file() {
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                     if package_type_filter.matches(&file_name) {
                        let dest_path = package_dest_dir.join(&*file_name);
                        info!("Copying package {} to {}", path.display(), dest_path.display());
                        fs::copy(&path, &dest_path).with_context(|| {
                            format!("Failed to copy package {} to {}", path.display(), dest_path.display())
                        })?;
                        found_packages = true;
                    }
                }
            }
            Err(e) => warn!("Error during package globbing: {}", e),
        }
    }
    if !found_packages {
        warn!("No packages found matching filter '{:?}' in {}. Check CPack configuration and output.", package_type_filter, build_dir.display());
    }
    Ok(())
}


pub fn run_build(
    config: &HalfConfig,
    is_release: bool,
    enable_address_sanitizer: bool,
    build_static_libs: bool,
    package_type_copy: &PackageTypeCopy,
    _build_wheel: bool, // half lib does not produce wheels. Parameter kept for CLI consistency.
) -> Result<()> {
    info!("Starting build process for 'half' library...");
    if _build_wheel {
        info!("Python wheel build requested for 'half', but it's not applicable. Skipping wheel build.");
    }

    let build_dir = &config.build_dir_half;

    // 1. Configure
    run_cmake_configure(config, build_dir, is_release, enable_address_sanitizer, build_static_libs)
        .context("CMake configuration failed for 'half'")?;

    // 2. Build
    // For header-only, build might be minimal or just run tests if configured.
    // The standard `cmake --build` command should handle it correctly.
    run_cmake_build(build_dir)
        .context("CMake build failed for 'half'")?;

    // 3. Install
    // This will install headers to CMAKE_INSTALL_PREFIX (e.g., /opt/rocm/include)
    run_cmake_install(build_dir)
        .context("CMake install failed for 'half'")?;
    
    // 4. Package (CPack)
    if !config.cpack_generator.is_empty() && config.cpack_generator.to_uppercase() != "NONE" {
        run_cpack(build_dir)
            .context("CPack failed for 'half'")?;
        
        // 5. Copy packages to the final destination
        copy_packages(config, package_type_copy)
            .context("Failed to copy generated packages for 'half'")?;
    } else {
        info!("CPack generation skipped as cpack_generator is empty or 'NONE'.");
    }


    info!("'half' library build process completed successfully.");
    Ok(())
}
