use anyhow::{Context, Result, anyhow};
use log::{debug, error, info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use glob::glob;
use std::env;

use crate::config_smi::RocmSmiConfig; // For rocm_cmake_params

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

/// Sets common ROCm CMake parameters based on the configuration.
/// Corresponds to parts of `rocm_common_cmake_params` in `compute_utils.sh`.
pub fn rocm_common_cmake_params(config: &RocmSmiConfig, cmd: &mut Command) {
    cmd.arg(format!("-DCMAKE_INSTALL_PREFIX={}", config.rocm_path.display()));
    cmd.arg(format!("-DROCM_PATH={}", config.rocm_path.display()));
    cmd.arg(format!("-DCMAKE_PREFIX_PATH={}", config.cmake_prefix_path.display())); // For finding rocm-cmake etc.

    // Build type
    let build_type = if env::var("CMAKE_BUILD_TYPE").unwrap_or_default().to_lowercase() == "debug" {
        "Debug"
    } else {
        "Release" // Default to Release if not specified or not "Debug"
    };
    cmd.arg(format!("-DCMAKE_BUILD_TYPE={}", build_type));

    // CPack Generator
    if !config.cpack_generator.is_empty() {
        cmd.arg(format!("-DCPACK_GENERATOR={}", config.cpack_generator));
    }
    // Patch version for CPack
    cmd.arg(format!("-DCPACK_PACKAGE_VERSION_PATCH={}", config.rocm_patch_version));

    // Set CMAKE_INSTALL_LIBDIR based on config.lib_dir_suffix
    cmd.arg(format!("-DCMAKE_INSTALL_LIBDIR={}", config.lib_dir_suffix));

    // Address sanitizer general flag (specific compiler flags might be needed too)
    if config.enable_address_sanitizer {
        cmd.arg("-DENABLE_ASAN=ON"); // Assuming CMake project uses this convention
                                     // Actual compiler flags for ASAN might be set via CMAKE_C_FLAGS/CMAKE_CXX_FLAGS
                                     // or by the project's CMake script when ENABLE_ASAN is ON.
                                     // The original script sets CXXFLAGS directly. We can pass them via cmake.
        let cxx_flags = env::var("CXXFLAGS").unwrap_or_default();
        cmd.arg(format!("-DCMAKE_CXX_FLAGS={}", format!("{} -fsanitize=address -fno-omit-frame-pointer", cxx_flags)));
        let c_flags = env::var("CFLAGS").unwrap_or_default();
        cmd.arg(format!("-DCMAKE_C_FLAGS={}", format!("{} -fsanitize=address -fno-omit-frame-pointer", c_flags)));
    }

    // Static libs
    if config.static_libs {
        cmd.arg("-DBUILD_SHARED_LIBS=OFF");
        cmd.arg("-DBUILD_STATIC_LIBS=ON"); // Some projects use this
    } else {
        cmd.arg("-DBUILD_SHARED_LIBS=ON");
    }

    // Add other common params from compute_utils.sh as needed
    // e.g. ROCM_SYMLINK_LIBS, USE_LD_GOLD, etc. if applicable to this project
    // For rocm-smi-lib, these might not all be relevant.
}

/// Sets ROCm CMake parameters that are often specific to individual component builds.
/// Corresponds to `rocm_cmake_params` in `compute_utils.sh`.
pub fn rocm_cmake_params(config: &RocmSmiConfig, cmd: &mut Command) {
    // This function in compute_utils.sh primarily sets:
    // - CMAKE_INSTALL_PREFIX, ROCM_PATH (covered by common)
    // - CMAKE_BUILD_TYPE (covered by common)
    // - CMAKE_MODULE_PATH (related to finding rocm-cmake, covered by CMAKE_PREFIX_PATH)
    // - BUILD_SHARED_LIBS (covered by common based on static_libs flag)
    // - CPACK_GENERATOR, CPACK_PACKAGE_VERSION_PATCH (covered by common)
    // - LIB_SUFFIX (covered by common via CMAKE_INSTALL_LIBDIR)
    // - PROJECT_NAME_SUFFIX (specific to rocm-smi-lib, handled in build_smi_logic)
    // - INSTALL_PREFIX (seems redundant with CMAKE_INSTALL_PREFIX)
    // - CMAKE_INSTALL_LIBDIR (covered by common)

    // Most are covered. If there were other specific logic from rocm_cmake_params for this component,
    // it would be added here. For instance, if it needed to set specific defines or options.

    // Example: if rocm-smi-lib had a specific option:
    // cmd.arg("-DSMI_LIB_SPECIFIC_OPTION=VALUE");

    // The original script also had logic for finding python and setting PYTHON_EXECUTABLE.
    // If this project needs python for anything (e.g., tests, docs), that logic would go here.
    // For now, assuming rocm-smi-lib's CMake doesn't require explicit Python path for core build.
    if let Ok(python_exe) = env::var("PYTHON_EXECUTABLE") {
        if !python_exe.is_empty() {
            cmd.arg(format!("-DPYTHON_EXECUTABLE={}", python_exe));
        }
    } else if let Ok(python_path) = which::which("python3") {
         cmd.arg(format!("-DPYTHON_EXECUTABLE={}", python_path.display()));
    } else if let Ok(python_path) = which::which("python") {
         cmd.arg(format!("-DPYTHON_EXECUTABLE={}", python_path.display()));
    } else {
        warn!("Python executable not found, PYTHON_EXECUTABLE CMake variable will not be set.");
    }


    // Code coverage flags (if CMAKE_BUILD_TYPE is Debug and CODE_COVERAGE is set)
    if env::var("CMAKE_BUILD_TYPE").unwrap_or_default().to_lowercase() == "debug" {
        if let Ok(code_coverage) = env::var("CODE_COVERAGE") {
            if !code_coverage.is_empty() && code_coverage != "0" {
                let cxx_flags = env::var("CXXFLAGS").unwrap_or_default();
                cmd.arg(format!("-DCMAKE_CXX_FLAGS={}", format!("{} -fprofile-arcs -ftest-coverage", cxx_flags)));
                 let c_flags = env::var("CFLAGS").unwrap_or_default();
                cmd.arg(format!("-DCMAKE_C_FLAGS={}", format!("{} -fprofile-arcs -ftest-coverage", c_flags)));
            }
        }
    }
}


/// Copies generated packages from a build directory to a destination directory,
/// filtering by a name suffix (e.g., "-rocm-smi-lib64").
pub fn copy_packages_with_suffix(build_dir: &Path, package_dest_dir: &Path, name_suffix: &str) -> Result<()> {
    fs::create_dir_all(package_dest_dir).with_context(|| {
        format!(
            "Failed to create package destination directory: {}",
            package_dest_dir.display()
        )
    })?;

    // Glob for .deb and .rpm packages. More specific if needed.
    let patterns = ["*.deb", "*.rpm"];
    let mut found_packages = false;

    for pattern in patterns.iter() {
        let glob_pattern = build_dir.join(pattern);
        debug!("Searching for packages in '{}' with pattern: {}", build_dir.display(), glob_pattern.display());

        for entry in glob(&glob_pattern.to_string_lossy())? {
            match entry {
                Ok(path) => {
                    if path.is_file() {
                        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                        // Check if the filename contains the required suffix before the extension
                        if let Some(stem) = path.file_stem() {
                            if stem.to_string_lossy().contains(name_suffix) {
                                let dest_path = package_dest_dir.join(&*file_name);
                                info!("Copying package {} to {}", path.display(), dest_path.display());
                                fs::copy(&path, &dest_path).with_context(|| {
                                    format!("Failed to copy package {} to {}", path.display(), dest_path.display())
                                })?;
                                found_packages = true;
                            } else {
                                debug!("Skipping package {} as it does not contain suffix '{}'", file_name, name_suffix);
                            }
                        }
                    }
                }
                Err(e) => warn!("Error during package globbing for pattern {}: {}", pattern, e),
            }
        }
    }
    if !found_packages {
         warn!("No packages found matching suffix '{}' in {}. Check CPack configuration, generated package names, and suffix.", name_suffix, build_dir.display());
    }
    Ok(())
}
