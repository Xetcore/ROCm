use clap::Parser;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use std::env; // For current_dir
use std::fs; 
use std::process::Command; // Added for running external commands
use which::which; // Added to find cmake executable

// CliArgs struct (as defined in the previous step)
/// Rust equivalent of the build_rocminfo.sh script.
/// Handles building and packaging of the rocminfo utility.
#[derive(Parser, Debug, Clone)] 
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

    /// Optional: Print path of output directory for specified package type (deb, rpm) and exit.
    #[arg(long)]
    outdir_target: Option<String>, 

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
    // Placeholder - to be implemented based on compute_utils.sh
    Vec::new()
}

fn get_rocm_common_cmake_params(_config: &AppConfig) -> Vec<String> {
    // Placeholder - to be implemented based on compute_utils.sh
    Vec::new()
}

fn handle_build(config: &AppConfig) -> Result<()> {
    if config.verbose {
        println!("Build operation selected.");
        println!("Ensuring build directory exists: {:?}", config.build_dir);
    }

    fs::create_dir_all(&config.build_dir)
        .with_context(|| format!("Failed to create build directory: {:?}", config.build_dir))?;

    let cmake_exe = which("cmake").map_err(|e| anyhow::anyhow!("cmake executable not found in PATH: {}", e))?;
    if config.verbose {
        println!("Found cmake executable at: {:?}", cmake_exe);
    }

    // CMake Configure Step
    if config.verbose {
        println!("Running CMake configure step...");
    }
    let mut cmake_configure_cmd = Command::new(&cmake_exe);
    cmake_configure_cmd.current_dir(&config.build_dir);
    cmake_configure_cmd.arg(&config.source_root); // Source directory
    
    // Add params from placeholder functions
    cmake_configure_cmd.args(get_rocm_cmake_params(config));
    cmake_configure_cmd.args(get_rocm_common_cmake_params(config));

    // Add core CMake arguments
    cmake_configure_cmd.arg(format!("-DCMAKE_BUILD_TYPE={}", config.build_type_cmake));
    cmake_configure_cmd.arg(format!("-DBUILD_SHARED_LIBS={}", config.build_shared_libs_cmake));
    cmake_configure_cmd.arg(format!("-DCMAKE_INSTALL_PREFIX={}", config.install_prefix.display()));
    cmake_configure_cmd.arg(format!("-DROCRTST_BLD_TYPE={}", config.rocrts_bld_type_cmake));
    
    // CPack version variables from original script
    cmake_configure_cmd.arg("-DCPACK_PACKAGE_VERSION_MAJOR=1"); // Hardcoded in script
    cmake_configure_cmd.arg(format!("-DCPACK_PACKAGE_VERSION_MINOR={}", config.rocm_libpatch_version));
    cmake_configure_cmd.arg("-DCPACK_PACKAGE_VERSION_PATCH=0"); // Hardcoded in script
    
    cmake_configure_cmd.arg("-DCMAKE_SKIP_BUILD_RPATH=TRUE"); // Hardcoded

    if let Some(ref gpu_list_param) = config.gpu_list_cmake {
        cmake_configure_cmd.arg(gpu_list_param);
    }

    if config.address_sanitizer_enabled {
        if config.verbose {
            println!("Address sanitizer enabled. Setting CMake flag and environment variables for CMake process.");
        }
        cmake_configure_cmd.arg("-DENABLE_ADDRESS_SANITIZER=ON"); // Assumed CMake flag
        // cmake_configure_cmd.env("ASAN_OPTIONS", "detect_leaks=0"); // Example
    }
    
    if config.verbose {
        println!("CMake configure command: {:?}", cmake_configure_cmd);
    }
    let configure_output = cmake_configure_cmd.output().with_context(|| "Failed to execute CMake configure command")?;
    if !configure_output.status.success() {
        return Err(anyhow::anyhow!(
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
        return Err(anyhow::anyhow!(
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
    if let Some(jobs) = config.jobs { // Original script uses $MAKEARG for install too
        cmake_install_cmd.arg("--parallel").arg(jobs.to_string());
    }
    if config.verbose {
        println!("CMake install command: {:?}", cmake_install_cmd);
    }
    let install_output = cmake_install_cmd.output().with_context(|| "Failed to execute CMake install command")?;
    if !install_output.status.success() {
        return Err(anyhow::anyhow!(
            "CMake install command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
            install_output.status, String::from_utf8_lossy(&install_output.stderr), String::from_utf8_lossy(&install_output.stdout)
        ));
    }
    if config.verbose {
        print!("CMake install stdout:\n{}", String::from_utf8_lossy(&install_output.stdout));
        if !install_output.stderr.is_empty() { eprintln!("CMake install stderr:\n{}", String::from_utf8_lossy(&install_output.stderr)); }
    }

    println!("Build and install operations completed.");
    // Packaging and other steps will be added in later phases.
    Ok(())
}

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
            // Corrected the anyhow! macro usage
            return Err(anyhow::anyhow!("Invalid package type \"{}\" provided for --outdir-target. Use 'deb' or 'rpm'.", pkg_to_print));
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    if cli_args.verbose {
        println!("Raw CLI arguments: {:#?}", &cli_args); 
    }
    
    let config = AppConfig::try_from_args(cli_args.clone()) // Clone cli_args for AppConfig
        .with_context(|| "Failed to initialize application configuration")?;

    if config.verbose {
        println!("Resolved AppConfig: {:#?}", &config);
        if config.address_sanitizer_enabled {
             println!("Note: --address-sanitizer active. ASan env vars and CMake flags will be applied during build.");
        }
    }
    
    // Handle --outdir-target action first as it exits immediately
    if let Some(ref pkg_to_print) = cli_args.outdir_target { 
        // No need to create a separate config_for_outdir if AppConfig is already available
        return handle_outdir(&config, pkg_to_print);
    }
    
    if config.clean {
        return handle_clean(&config); 
    }
    
    handle_build(&config)?;
    
    println!("rocminfo-builder (Rust) - Operation complete.");
    Ok(())
}

```
