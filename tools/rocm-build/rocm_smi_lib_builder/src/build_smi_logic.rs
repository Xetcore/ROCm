use anyhow::{Context, Result, anyhow};
use log::{info, warn, debug};
use std::path::Path;
use std::process::Command;
use std::fs;
use glob::glob;
use num_cpus;
use std::env;

use crate::config_smi::{RocmSmiConfig, PackageType}; // Assuming PackageType will be used by copy_packages
use crate::utils_smi::{run_command, rocm_common_cmake_params, rocm_cmake_params, copy_packages_with_suffix};

fn run_cmake_configure(
    config: &RocmSmiConfig,
    build_dir: &Path,
) -> Result<()> {
    let cmake_source_dir = &config.smi_src_dir;
    let mut cmake_cmd = Command::new("cmake");

    cmake_cmd.arg(format!("-S{}", cmake_source_dir.display()));
    cmake_cmd.arg(format!("-B{}", build_dir.display()));

    // Apply common and specific ROCm CMake parameters
    rocm_common_cmake_params(config, &mut cmake_cmd);
    rocm_cmake_params(config, &mut cmake_cmd); // rocm_cmake_params might override or add to common ones

    // Specific flags for rocm-smi-lib not covered by the generic functions
    cmake_cmd.arg(format!("-DPROJECT_NAME_SUFFIX={}", config.package_name_suffix));
    
    // For rocm-smi-lib, the original script sets LIB_SUFFIX based on 64/32 bit.
    // This is typically handled by CMAKE_INSTALL_LIBDIR or GNUInstallDirs.
    // We'll ensure CMAKE_INSTALL_LIBDIR is set.
    cmake_cmd.arg(format!("-DCMAKE_INSTALL_LIBDIR={}", config.lib_dir_suffix));


    if config.static_libs {
        cmake_cmd.arg("-DBUILD_SHARED_LIBS=OFF");
        cmake_cmd.arg("-DBUILD_STATIC_LIBS=ON");
    } else {
        cmake_cmd.arg("-DBUILD_SHARED_LIBS=ON");
        // BUILD_STATIC_LIBS=OFF is usually default if BUILD_SHARED_LIBS=ON
    }
    
    if config.enable_address_sanitizer {
        // ASAN flags for compiler are typically set via CMAKE_C_FLAGS / CMAKE_CXX_FLAGS
        // The original script sets environment variables. We'll do both for robustness.
        cmake_cmd.arg("-DENABLE_ASAN=ON"); // If the CMake project supports this option
        // Environment variables will be set when the command is run.
    }
    
    info!(
        "Configuring 'rocm-smi-lib' project from: {} with build dir: {}",
        cmake_source_dir.display(),
        build_dir.display()
    );
    debug!("CMake configure command for 'rocm-smi-lib': {:?}", cmake_cmd);

    // ASAN Environment Variables
    if config.enable_address_sanitizer {
        let asan_options = env::var("ASAN_OPTIONS").unwrap_or_default();
        let lsan_options = env::var("LSAN_OPTIONS").unwrap_or_default();
        cmake_cmd.env("ASAN_OPTIONS", format!("{}:detect_leaks=0", asan_options));
        cmake_cmd.env("LSAN_OPTIONS", format!("{}:suppressions={}/asan_suppressions.txt", lsan_options, config.smi_src_dir.display()));
        cmake_cmd.env("LD_PRELOAD", env::var("ASAN_PRELOAD").unwrap_or_else(|_| "/usr/lib/x86_64-linux-gnu/libasan.so".to_string())); // Example path
        info!("ASAN environment variables configured for cmake process.");
    }


    run_command(cmake_cmd, "CMake configuration for 'rocm-smi-lib'")
}

fn run_cmake_build(build_dir: &Path, config: &RocmSmiConfig) -> Result<()> {
    let mut cmake_cmd = Command::new("cmake");
    cmake_cmd.arg("--build").arg(build_dir);
    let num_jobs = num_cpus::get();
    cmake_cmd.arg("--").arg("-j").arg(num_jobs.to_string());

    // ASAN Environment Variables for build step too
    if config.enable_address_sanitizer {
        let asan_options = env::var("ASAN_OPTIONS").unwrap_or_default();
        let lsan_options = env::var("LSAN_OPTIONS").unwrap_or_default();
        cmake_cmd.env("ASAN_OPTIONS", format!("{}:detect_leaks=0", asan_options));
        cmake_cmd.env("LSAN_OPTIONS", format!("{}:suppressions={}/asan_suppressions.txt", lsan_options, config.smi_src_dir.display()));
        cmake_cmd.env("LD_PRELOAD", env::var("ASAN_PRELOAD").unwrap_or_else(|_| "/usr/lib/x86_64-linux-gnu/libasan.so".to_string()));
    }

    info!("Building 'rocm-smi-lib' project in: {} with {} jobs", build_dir.display(), num_jobs);
    debug!("CMake build command for 'rocm-smi-lib': {:?}", cmake_cmd);
    run_command(cmake_cmd, "CMake build for 'rocm-smi-lib'")
}

fn run_cmake_install(build_dir: &Path, config: &RocmSmiConfig) -> Result<()> {
    let mut cmake_cmd = Command::new("cmake");
    cmake_cmd.arg("--install").arg(build_dir);
    
    // ASAN Environment Variables for install step (less likely to be needed, but for consistency)
    if config.enable_address_sanitizer {
        let asan_options = env::var("ASAN_OPTIONS").unwrap_or_default();
        let lsan_options = env::var("LSAN_OPTIONS").unwrap_or_default();
        cmake_cmd.env("ASAN_OPTIONS", format!("{}:detect_leaks=0", asan_options));
        cmake_cmd.env("LSAN_OPTIONS", format!("{}:suppressions={}/asan_suppressions.txt", lsan_options, config.smi_src_dir.display()));
        cmake_cmd.env("LD_PRELOAD", env::var("ASAN_PRELOAD").unwrap_or_else(|_| "/usr/lib/x86_64-linux-gnu/libasan.so".to_string()));
    }


    info!("Installing 'rocm-smi-lib' project from: {}", build_dir.display());
    debug!("CMake install command for 'rocm-smi-lib': {:?}", cmake_cmd);
    run_command(cmake_cmd, "CMake install for 'rocm-smi-lib'")
}

fn run_cpack(build_dir: &Path, config: &RocmSmiConfig) -> Result<()> {
    let mut cpack_cmd = Command::new("cpack");
    cpack_cmd.current_dir(build_dir); 
    // CPack arguments can be complex, often derived from CMake config.
    // If CPackConfig.cmake is generated properly, this should be enough.
    // Add -G if directly specifying generators, but usually it's from CMAKE_CPACK_GENERATOR
    // cpack_cmd.arg("-G").arg(&config.cpack_generator);
    // cpack_cmd.arg("--config").arg("CPackConfig.cmake"); // Or CPackSourceConfig.cmake

    // ASAN Environment Variables for CPack (highly unlikely to be needed)
    if config.enable_address_sanitizer {
        let asan_options = env::var("ASAN_OPTIONS").unwrap_or_default();
        let lsan_options = env::var("LSAN_OPTIONS").unwrap_or_default();
        cpack_cmd.env("ASAN_OPTIONS", format!("{}:detect_leaks=0", asan_options));
        cpack_cmd.env("LSAN_OPTIONS", format!("{}:suppressions={}/asan_suppressions.txt", lsan_options, config.smi_src_dir.display()));
        cpack_cmd.env("LD_PRELOAD", env::var("ASAN_PRELOAD").unwrap_or_else(|_| "/usr/lib/x86_64-linux-gnu/libasan.so".to_string()));
    }

    info!("Running CPack in: {}", build_dir.display());
    debug!("CPack command for 'rocm-smi-lib': {:?}", cpack_cmd);
    run_command(cpack_cmd, "CPack for 'rocm-smi-lib'")
}


pub fn run_build(config: &RocmSmiConfig) -> Result<()> {
    info!("Starting build process for 'rocm-smi-lib' library...");

    let build_dir = &config.build_dir_smi;

    // 1. Configure
    run_cmake_configure(config, build_dir)
        .context("CMake configuration failed for 'rocm-smi-lib'")?;

    // 2. Build
    run_cmake_build(build_dir, config)
        .context("CMake build failed for 'rocm-smi-lib'")?;

    // 3. Install
    run_cmake_install(build_dir, config)
        .context("CMake install failed for 'rocm-smi-lib'")?;
    
    // 4. Package (CPack)
    if !config.cpack_generator.is_empty() && config.cpack_generator.to_uppercase() != "NONE" {
        run_cpack(build_dir, config)
            .context("CPack failed for 'rocm-smi-lib'")?;
        
        // 5. Copy packages to the final destination
        // The package name suffix (e.g., -rocm-smi-lib64) should be part of the generated package file name by CMake/CPack.
        copy_packages_with_suffix(build_dir, &config.package_dir_smi, &config.package_name_suffix)
            .context("Failed to copy generated packages for 'rocm-smi-lib'")?;
    } else {
        info!("CPack generation skipped as cpack_generator is empty or 'NONE'.");
    }

    info!("'rocm-smi-lib' library build process completed successfully.");
    Ok(())
}
