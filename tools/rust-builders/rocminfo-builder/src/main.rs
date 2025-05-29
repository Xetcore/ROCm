use clap::Parser;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result, anyhow}; // Ensure anyhow is imported for anyhow::anyhow!
use std::env; 
use std::fs; 
use std::process::Command; 
use which::which; 
use glob::glob; 

/// Rust equivalent of the build_rocminfo.sh script.
/// Handles building and packaging of the rocminfo utility.
#[derive(Parser, Debug, Clone)] // Added Clone
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Path to the rocminfo source directory (replaces ROCMINFO_ROOT env var).
    #[arg(long, value_name = "PATH")]
    source_root: PathBuf,

    /// Optional: Install prefix for rocminfo (influences CMAKE_INSTALL_PREFIX).
    /// Defaults to "<package_root>/rocm" (similar to how /opt/rocm is structured).
    #[arg(long, value_name = "PATH")]
    install_prefix: Option<PathBuf>,

    /// Optional: Specify the build directory.
    /// Defaults to "<source-root>/build/rocminfo-builder".
    #[arg(long, value_name = "PATH")]
    build_dir: Option<PathBuf>,

    /// Optional: Specify the root directory for packages.
    /// Defaults to "<source-root>/dist".
    #[arg(long, value_name = "PATH")]
    package_root: Option<PathBuf>,
    
    /// Optional: Version patch number for CPack (ROCM_LIBPATCH_VERSION). Defaults to "0".
    #[arg(long, value_name = "VERSION", default_value = "0")]
    rocm_libpatch_version: String,

    /// Clean output and delete all intermediate work.
    #[arg(long, short = 'c')]
    clean: bool,

    /// Build a release version (default is debug: RelWithDebInfo for CMake, rel for make).
    #[arg(long, short = 'r')]
    release: bool,

    /// Build static lib (.a) instead of dynamic/shared (.so).
    #[arg(long, short = 's')]
    static_libs: bool,

    /// Enable address sanitizer.
    #[arg(long, short = 'a')]
    address_sanitizer: bool,

    /// Creates a python wheel package (if applicable for rocminfo).
    #[arg(long, short = 'w')]
    wheel: bool,

    /// Optional: Comma-separated list of GPUs to target (passed to CMake as -DGPU_LIST).
    #[arg(long, value_name = "LIST")]
    gpu_list: Option<String>,

    /// Optional: Number of parallel jobs for the build (e.g., cmake --build . --parallel N).
    #[arg(long, value_name = "N")]
    jobs: Option<usize>,

    /// Specify packaging format to copy (e.g. "deb", "rpm", "all").
    #[arg(long, value_name = "TYPE", default_value = "all")] // Already present from previous step
    package_type: String,

    /// Optional: Print path of output directory for specified package type (deb, rpm) and exit.
    #[arg(long)]
    outdir_target: Option<String>, // New argument

    /// Enable verbose logging.
    #[arg(long, short = 'v', action = clap::ArgAction::SetTrue)]
    verbose: bool,
}


#[derive(Debug)]
struct AppConfig {
    source_root: PathBuf,
    install_prefix: PathBuf,
    build_dir: PathBuf,
    package_root: PathBuf,
    deb_package_dir: PathBuf,
    rpm_package_dir: PathBuf,
    rocm_libpatch_version: String,
    clean: bool,
    build_type_cmake: String, 
    rocrts_bld_type_cmake: String, 
    build_shared_libs_cmake: String, 
    address_sanitizer_enabled: bool, 
    wheel: bool,
    gpu_list_cmake: Option<String>, 
    jobs: Option<usize>, 
    package_type_to_copy: String, 
    verbose: bool,
}

impl AppConfig {
    fn try_from_args(args: CliArgs) -> Result<Self> {
        let current_dir = env::current_dir().context("Failed to get current directory")?;

        let source_root = args.source_root.canonicalize()
            .with_context(|| format!("Failed to find or access source-root path: {:?}", args.source_root))?;

        let build_dir = args.build_dir
            .map(|p| if p.is_absolute() { p } else { current_dir.join(&p) })
            .unwrap_or_else(|| source_root.join("build").join("rocminfo-builder"));
        
        let package_root = args.package_root
            .map(|p| if p.is_absolute() { p } else { current_dir.join(&p) })
            .unwrap_or_else(|| source_root.join("dist"));
        
        let install_prefix = args.install_prefix
            .map(|p| if p.is_absolute() { p } else { current_dir.join(&p) })
            .unwrap_or_else(|| package_root.join("rocm")); 

        let deb_package_dir = package_root.join("deb").join("rocminfo");
        let rpm_package_dir = package_root.join("rpm").join("rocminfo");

        let (build_type_cmake, rocrts_bld_type_cmake) = if args.release {
            ("RelWithDebInfo".to_string(), "rel".to_string())
        } else {
            ("Debug".to_string(), "debug".to_string())
        };

        Ok(AppConfig {
            source_root,
            install_prefix,
            build_dir,
            package_root,
            deb_package_dir,
            rpm_package_dir,
            rocm_libpatch_version: args.rocm_libpatch_version,
            clean: args.clean,
            build_type_cmake,
            rocrts_bld_type_cmake,
            build_shared_libs_cmake: if args.static_libs { "OFF".to_string() } else { "ON".to_string() },
            address_sanitizer_enabled: args.address_sanitizer,
            wheel: args.wheel,
            gpu_list_cmake: args.gpu_list.map(|gpus| format!("-DGPU_LIST={}", gpus)),
            jobs: args.jobs,
            package_type_to_copy: args.package_type.to_lowercase(),
            verbose: args.verbose,
        })
    }
}

fn handle_clean(config: &AppConfig) -> Result<()> {
    if config.verbose {
        println!("Clean operation selected.");
    }

    let paths_to_remove_dirs = [
        &config.build_dir,
        &config.deb_package_dir,
        &config.rpm_package_dir,
    ];

    for path in paths_to_remove_dirs.iter() {
        if path.exists() {
            if config.verbose {
                println!("Attempting to remove directory: {:?}", path);
            }
            fs::remove_dir_all(path)
                .with_context(|| format!("Failed to remove directory: {:?}", path))?;
            if config.verbose {
                println!("Successfully removed directory: {:?}", path);
            }
        } else if config.verbose {
            println!("Directory {:?} does not exist. Nothing to remove.", path);
        }
    }

    let installed_binary_path = config.install_prefix.join("bin").join("rocminfo");
    if installed_binary_path.exists() {
        if config.verbose {
            println!("Attempting to remove installed binary: {:?}", installed_binary_path);
        }
        fs::remove_file(&installed_binary_path)
            .with_context(|| format!("Failed to remove installed binary: {:?}", installed_binary_path))?;
        if config.verbose {
            println!("Successfully removed installed binary: {:?}", installed_binary_path);
        }
    } else if config.verbose {
        println!("Installed binary {:?} does not exist. Nothing to remove.", installed_binary_path);
    }
    
    println!("Clean operation completed.");
    Ok(())
}

fn get_rocm_cmake_params(_config: &AppConfig) -> Vec<String> {
    Vec::new()
}

fn get_rocm_common_cmake_params(_config: &AppConfig) -> Vec<String> {
    Vec::new()
}

fn copy_packages(config: &AppConfig) -> Result<()> {
    if config.verbose {
        println!("Copying packages based on package_type_to_copy: {}", config.package_type_to_copy);
    }

    let copy_deb = config.package_type_to_copy == "all" || config.package_type_to_copy == "deb";
    let copy_rpm = config.package_type_to_copy == "all" || config.package_type_to_copy == "rpm";

    if copy_deb {
        fs::create_dir_all(&config.deb_package_dir)
            .with_context(|| format!("Failed to create DEB package directory: {:?}", config.deb_package_dir))?;
        let deb_pattern = config.build_dir.join("*.deb"); 
        if config.verbose {
            println!("Searching for DEB packages with pattern: {:?}", deb_pattern.to_string_lossy());
        }
        for entry in glob(&deb_pattern.to_string_lossy())? {
            match entry {
                Ok(path) => {
                    let file_name = path.file_name().ok_or_else(|| anyhow!("Failed to get filename from {:?}", path))?;
                    let dest_path = config.deb_package_dir.join(file_name);
                    if config.verbose {
                        println!("Copying {:?} to {:?}", path, dest_path);
                    }
                    fs::copy(&path, &dest_path)
                        .with_context(|| format!("Failed to copy {:?} to {:?}", path, dest_path))?;
                }
                Err(e) => return Err(anyhow!("Error matching DEB package: {}", e)),
            }
        }
    }

    if copy_rpm {
        fs::create_dir_all(&config.rpm_package_dir)
            .with_context(|| format!("Failed to create RPM package directory: {:?}", config.rpm_package_dir))?;
        let rpm_pattern = config.build_dir.join("*.rpm"); 
        if config.verbose {
            println!("Searching for RPM packages with pattern: {:?}", rpm_pattern.to_string_lossy());
        }
        for entry in glob(&rpm_pattern.to_string_lossy())? {
            match entry {
                Ok(path) => {
                    let file_name = path.file_name().ok_or_else(|| anyhow!("Failed to get filename from {:?}", path))?;
                    let dest_path = config.rpm_package_dir.join(file_name);
                    if config.verbose {
                        println!("Copying {:?} to {:?}", path, dest_path);
                    }
                    fs::copy(&path, &dest_path)
                        .with_context(|| format!("Failed to copy {:?} to {:?}", path, dest_path))?;
                }
                Err(e) => return Err(anyhow!("Error matching RPM package: {}", e)),
            }
        }
    }
    Ok(())
}


fn handle_build(config: &AppConfig) -> Result<()> {
    if config.verbose {
        println!("Build operation selected.");
        println!("Ensuring build directory exists: {:?}", config.build_dir);
    }

    fs::create_dir_all(&config.build_dir)
        .with_context(|| format!("Failed to create build directory: {:?}", config.build_dir))?;

    let cmake_exe = which("cmake").map_err(|e| anyhow!("cmake executable not found in PATH: {}", e))?;
    if config.verbose {
        println!("Found cmake executable at: {:?}", cmake_exe);
    }

    // CMake Configure Step
    if config.verbose {
        println!("Running CMake configure step...");
    }
    let mut cmake_configure_cmd = Command::new(&cmake_exe);
    cmake_configure_cmd.current_dir(&config.build_dir);
    cmake_configure_cmd.arg(&config.source_root); 
    cmake_configure_cmd.args(get_rocm_cmake_params(config));
    cmake_configure_cmd.args(get_rocm_common_cmake_params(config));
    cmake_configure_cmd.arg(format!("-DCMAKE_BUILD_TYPE={}", config.build_type_cmake));
    cmake_configure_cmd.arg(format!("-DBUILD_SHARED_LIBS={}", config.build_shared_libs_cmake));
    cmake_configure_cmd.arg(format!("-DCMAKE_INSTALL_PREFIX={}", config.install_prefix.display()));
    cmake_configure_cmd.arg(format!("-DROCRTST_BLD_TYPE={}", config.rocrts_bld_type_cmake));
    cmake_configure_cmd.arg("-DCPACK_PACKAGE_VERSION_MAJOR=1"); 
    cmake_configure_cmd.arg(format!("-DCPACK_PACKAGE_VERSION_MINOR={}", config.rocm_libpatch_version));
    cmake_configure_cmd.arg("-DCPACK_PACKAGE_VERSION_PATCH=0"); 
    cmake_configure_cmd.arg("-DCMAKE_SKIP_BUILD_RPATH=TRUE"); 
    if let Some(ref gpu_list_param) = config.gpu_list_cmake {
        cmake_configure_cmd.arg(gpu_list_param);
    }
    if config.address_sanitizer_enabled {
        if config.verbose {
            println!("Address sanitizer enabled. Setting CMake flag. Environment variable setup for ASan runtime (e.g., ASAN_OPTIONS) is pending full details from compute_utils.sh:set_asan_env_vars.");
        }
        cmake_configure_cmd.arg("-DENABLE_ADDRESS_SANITIZER=ON"); // Assumed CMake flag for compilation

        // TODO: Replicate compute_utils.sh:set_asan_env_vars if needed for child processes (e.g., CTest).
        // This would involve using cmake_configure_cmd.env("ASAN_OPTIONS", "value") and for other commands too if they run tests.
        // For example:
        // cmake_configure_cmd.env("ASAN_OPTIONS", "detect_leaks=0:detect_stack_use_after_return=1");
        // cmake_build_cmd.env("ASAN_OPTIONS", "detect_leaks=0:detect_stack_use_after_return=1"); // If CTest runs here
    }
    
    if config.verbose {
        println!("CMake configure command: {:?}", cmake_configure_cmd);
    }
    let configure_output = cmake_configure_cmd.output().with_context(|| "Failed to execute CMake configure command")?;
    if !configure_output.status.success() {
        return Err(anyhow!(
            "CMake configure command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
            configure_output.status, String::from_utf8_lossy(&configure_output.stderr), String::from_utf8_lossy(&configure_output.stdout)
        ));
    }
    if config.verbose {
        print!("CMake configure stdout:\n{}", String::from_utf8_lossy(&configure_output.stdout));
        if !configure_output.stderr.is_empty() { eprintln!("CMake configure stderr:\n{}", String::from_utf8_lossy(&configure_output.stderr)); }
    }

    // CMake Build Step
    if config.verbose {
        println!("Running CMake build step...");
    }
    let mut cmake_build_cmd = Command::new(&cmake_exe);
    cmake_build_cmd.current_dir(&config.build_dir);
    cmake_build_cmd.arg("--build").arg(".");
    if let Some(jobs) = config.jobs {
        cmake_build_cmd.arg("--parallel").arg(jobs.to_string());
    }
    if config.verbose {
        println!("CMake build command: {:?}", cmake_build_cmd);
    }
    let build_output = cmake_build_cmd.output().with_context(|| "Failed to execute CMake build command")?;
    if !build_output.status.success() {
        return Err(anyhow!(
            "CMake build command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
            build_output.status, String::from_utf8_lossy(&build_output.stderr), String::from_utf8_lossy(&build_output.stdout)
        ));
    }
    if config.verbose {
        print!("CMake build stdout:\n{}", String::from_utf8_lossy(&build_output.stdout));
        if !build_output.stderr.is_empty() { eprintln!("CMake build stderr:\n{}", String::from_utf8_lossy(&build_output.stderr)); }
    }

    // CMake Install Step
    if config.verbose {
        println!("Running CMake install step...");
    }
    let mut cmake_install_cmd = Command::new(&cmake_exe);
    cmake_install_cmd.current_dir(&config.build_dir);
    cmake_install_cmd.arg("--build").arg(".").arg("--target").arg("install");
    if let Some(jobs) = config.jobs { 
        cmake_install_cmd.arg("--parallel").arg(jobs.to_string());
    }
    if config.verbose {
        println!("CMake install command: {:?}", cmake_install_cmd);
    }
    let install_output = cmake_install_cmd.output().with_context(|| "Failed to execute CMake install command")?;
    if !install_output.status.success() {
        return Err(anyhow!(
            "CMake install command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
            install_output.status, String::from_utf8_lossy(&install_output.stderr), String::from_utf8_lossy(&install_output.stdout)
        ));
    }
    if config.verbose {
        print!("CMake install stdout:\n{}", String::from_utf8_lossy(&install_output.stdout));
        if !install_output.stderr.is_empty() { eprintln!("CMake install stderr:\n{}", String::from_utf8_lossy(&install_output.stderr)); }
    }
    
    // CMake Package Step
    if config.verbose {
        println!("Running CMake package step...");
    }
    let mut cmake_package_cmd = Command::new(&cmake_exe);
    cmake_package_cmd.current_dir(&config.build_dir);
    cmake_package_cmd.arg("--build").arg(".").arg("--target").arg("package");
    if let Some(jobs) = config.jobs {
        cmake_package_cmd.arg("--parallel").arg(jobs.to_string());
    }
    if config.verbose {
        println!("CMake package command: {:?}", cmake_package_cmd);
    }
    let package_output = cmake_package_cmd.output().with_context(|| "Failed to execute CMake package command")?;
    if !package_output.status.success() {
        return Err(anyhow!(
            "CMake package command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
            package_output.status, String::from_utf8_lossy(&package_output.stderr), String::from_utf8_lossy(&package_output.stdout)
        ));
    }
    if config.verbose {
        print!("CMake package stdout:\n{}", String::from_utf8_lossy(&package_output.stdout));
        if !package_output.stderr.is_empty() { eprintln!("CMake package stderr:\n{}", String::from_utf8_lossy(&package_output.stderr)); }
        println!("CMake package step completed successfully.");
    }

    copy_packages(config)?;

    // Wheel building logic
    if config.wheel {
        if config.verbose {
            println!("Wheel package creation requested for rocminfo.");
        }
        // The original script calls build_wheel "$ROCMINFO_BUILD_DIR" "$PROJ_NAME".
        // This implies setup.py might be in ROCMINFO_SRC_ROOT or build_wheel handles paths.
        // We'll assume setup.py is in config.source_root.

        let python_exe = which("python3").or_else(|_| which("python"))
            .map_err(|e| anyhow::anyhow!("python3 or python executable not found in PATH for wheel build: {}", e))?;
        
        if config.verbose {
            println!("Found python executable at: {:?}", python_exe);
            println!("Assuming setup.py is in {:?}", config.source_root);
        }

        let setup_py_path = config.source_root.join("setup.py");
        if !setup_py_path.exists() {
            // It's possible rocminfo doesn't have a setup.py.
            // The original script's build_wheel might handle this.
            // For now, we'll print a warning if verbose, or just skip.
            if config.verbose {
                println!("Warning: setup.py not found at {:?}. Skipping wheel build.", setup_py_path);
            }
            // Assuming it's not a fatal error if setup.py is missing for rocminfo
        } else {
            let mut wheel_cmd = Command::new(python_exe);
            wheel_cmd.current_dir(&config.source_root); // Run setup.py from its location
            wheel_cmd.arg("setup.py").arg("bdist_wheel");

            // Define wheel output directory
            let wheel_dist_dir = config.build_dir.join("wheelhouse");
            fs::create_dir_all(&wheel_dist_dir)
                .with_context(|| format!("Failed to create directory for wheel output: {:?}", wheel_dist_dir))?;
            wheel_cmd.arg("--dist-dir").arg(&wheel_dist_dir);

            if config.verbose {
                println!("Executing wheel command: {:?}", wheel_cmd);
            }

            let wheel_output = wheel_cmd.output()
                .with_context(|| format!("Failed to execute python setup.py bdist_wheel. Ensure python and setuptools are installed and setup.py exists at {:?}", config.source_root))?;
            
            if !wheel_output.status.success() {
                return Err(anyhow::anyhow!(
                    "Python wheel command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
                    wheel_output.status, String::from_utf8_lossy(&wheel_output.stderr), String::from_utf8_lossy(&wheel_output.stdout)
                ));
            }

            if config.verbose {
                print!("Python wheel command stdout:\n{}", String::from_utf8_lossy(&wheel_output.stdout));
                if !wheel_output.stderr.is_empty() { eprintln!("Python wheel command stderr:\n{}", String::from_utf8_lossy(&wheel_output.stderr)); }
                println!("Python wheel(s) for rocminfo created successfully in {:?}", wheel_dist_dir);
                // TODO: Original script's build_wheel might copy this to a final package location.
            }
        }
    }
    
    println!("rocminfo-builder: Build operations (including packaging/wheel if requested) completed.");
    Ok(())
}

// New handle_outdir function
fn handle_outdir(config: &AppConfig, pkg_to_print: &str) -> Result<()> {
    if config.verbose {
        println!("Outdir action selected for package type: {}", pkg_to_print);
    }
    match pkg_to_print.to_lowercase().as_str() {
        "deb" => {
            println!("{}", config.deb_package_dir.display());
        }
        "rpm" => {
            println!("{}", config.rpm_package_dir.display());
        }
        _ => {
            return Err(anyhow!("Invalid package type \"{}\" provided for --outdir-target. Use 'deb' or 'rpm'.", pkg_to_print));
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    if cli_args.verbose {
        println!("Raw CLI arguments: {:#?}", &cli_args); 
    }
    
    // Handle --outdir-target action first, as it's an informational command that exits.
    if let Some(ref pkg_to_print) = cli_args.outdir_target {
        let temp_cli_args_for_outdir = cli_args.clone(); 
        let config_for_outdir = AppConfig::try_from_args(temp_cli_args_for_outdir)?;
        return handle_outdir(&config_for_outdir, pkg_to_print);
    }
    
    let config = AppConfig::try_from_args(cli_args)?; 

    if config.verbose {
        println!("Resolved AppConfig: {:#?}", &config);
        if config.address_sanitizer_enabled {
             println!("Note: --address-sanitizer active. ASan env vars and CMake flags will be applied during build if applicable.");
        }
    }
    
    if config.clean {
        return handle_clean(&config); 
    }
    
    handle_build(&config)?;
    
    if config.wheel && config.verbose {
        println!("Note: --wheel flag was specified, but rocminfo typically does not produce a Python wheel. This flag may have no effect for this component.");
    }

    println!("rocminfo-builder (Rust) - Operation complete.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*; // Import items from the parent module (main.rs)
    use std::fs as std_fs; // Renamed to avoid conflict with the module name if main.rs is in a directory named 'fs'

    // Helper to create a basic CliArgs for testing AppConfig
    fn basic_cli_args(source_root_path: PathBuf) -> CliArgs {
        CliArgs {
            source_root: source_root_path,
            install_prefix: None,
            build_dir: None,
            package_root: None,
            rocm_libpatch_version: "0".to_string(),
            clean: false,
            release: false,
            static_libs: false,
            address_sanitizer: false, // Corrected field name
            wheel: false,
            gpu_list: None,
            jobs: None,
            package_type: "all".to_string(), // Added this field
            outdir_target: None,
            verbose: false,
        }
    }

    #[test]
    fn test_app_config_defaults() {
        let temp_dir = std::env::temp_dir();
        let dummy_source_root = temp_dir.join("test_rocminfo_source_defaults");
        std_fs::create_dir_all(&dummy_source_root).unwrap();

        let mut cli_args = basic_cli_args(dummy_source_root.clone());
        // Field name is 'address_sanitizer' in CliArgs
        cli_args.address_sanitizer = false; 

        let config_result = AppConfig::try_from_args(cli_args);
        assert!(config_result.is_ok());
        if let Ok(config) = config_result {
            assert_eq!(config.source_root, dummy_source_root);
            assert_eq!(config.build_dir, dummy_source_root.join("build").join("rocminfo-builder"));
            let expected_package_root = dummy_source_root.join("dist");
            assert_eq!(config.package_root, expected_package_root);
            assert_eq!(config.install_prefix, expected_package_root.join("rocm"));
            assert_eq!(config.deb_package_dir, expected_package_root.join("deb").join("rocminfo"));
            assert_eq!(config.rpm_package_dir, expected_package_root.join("rpm").join("rocminfo"));
            assert_eq!(config.build_type_cmake, "Debug");
            assert_eq!(config.rocrts_bld_type_cmake, "debug");
            assert_eq!(config.build_shared_libs_cmake, "ON");
            assert!(!config.address_sanitizer_enabled);
        }
        
        std_fs::remove_dir_all(&dummy_source_root).unwrap(); // Clean up
    }

    #[test]
    fn test_app_config_release_flag() {
        let temp_dir = std::env::temp_dir();
        let dummy_source_root = temp_dir.join("test_rocminfo_source_release");
        std_fs::create_dir_all(&dummy_source_root).unwrap();
        
        let mut cli_args = basic_cli_args(dummy_source_root.clone());
        cli_args.release = true;
        cli_args.address_sanitizer = false; 

        let config = AppConfig::try_from_args(cli_args).unwrap();
        assert_eq!(config.build_type_cmake, "RelWithDebInfo");
        assert_eq!(config.rocrts_bld_type_cmake, "rel");
        
        std_fs::remove_dir_all(&dummy_source_root).unwrap(); // Clean up
    }

    #[test]
    fn test_app_config_static_libs_flag() {
        let temp_dir = std::env::temp_dir();
        let dummy_source_root = temp_dir.join("test_rocminfo_source_static");
        std_fs::create_dir_all(&dummy_source_root).unwrap();

        let mut cli_args = basic_cli_args(dummy_source_root.clone());
        cli_args.static_libs = true;
        cli_args.address_sanitizer = false;

        let config = AppConfig::try_from_args(cli_args).unwrap();
        assert_eq!(config.build_shared_libs_cmake, "OFF");

        std_fs::remove_dir_all(&dummy_source_root).unwrap(); // Clean up
    }
    
    #[test]
    fn test_app_config_custom_paths() {
        let temp_dir = std::env::temp_dir();
        let dummy_source_root = temp_dir.join("test_rocminfo_source_custom");
        let custom_build_dir = temp_dir.join("custom_build");
        let custom_package_root = temp_dir.join("custom_dist");
        let custom_install_prefix = temp_dir.join("custom_install");

        std_fs::create_dir_all(&dummy_source_root).unwrap();
        // No need to create custom_build_dir, custom_package_root, custom_install_prefix for AppConfig test itself

        let mut cli_args = basic_cli_args(dummy_source_root.clone());
        cli_args.build_dir = Some(custom_build_dir.clone());
        cli_args.package_root = Some(custom_package_root.clone());
        cli_args.install_prefix = Some(custom_install_prefix.clone());
        cli_args.address_sanitizer = false; 

        let config = AppConfig::try_from_args(cli_args).unwrap();
        assert_eq!(config.build_dir, custom_build_dir);
        assert_eq!(config.package_root, custom_package_root);
        assert_eq!(config.install_prefix, custom_install_prefix);
        
        std_fs::remove_dir_all(&dummy_source_root).unwrap(); // Clean up
    }
}
```
