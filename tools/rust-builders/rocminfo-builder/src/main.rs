use clap::Parser;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result, anyhow}; 
use std::env; 
use std::fs; 
use std::process::Command; 
use which::which; 
use glob::glob; 

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

    /// Specify packaging format to copy (e.g. "deb", "rpm", "all").
    #[arg(long, value_name = "TYPE", default_value = "all")]
    package_type: String,

    /// Optional: Print path of output directory for specified package type (deb, rpm) and exit.
    #[arg(long)]
    outdir_target: Option<String>,

    /// Optional: RPATH for ROCm executables.
    #[arg(long, value_name = "RPATH_STR")]
    rocm_exe_rpath: Option<String>,

    /// Optional: RPATH for ROCm shared libraries.
    #[arg(long, value_name = "RPATH_STR")]
    rocm_lib_rpath: Option<String>,

    /// Optional: RPATH for ROCm ASan shared libraries.
    #[arg(long, value_name = "RPATH_STR")]
    rocm_asan_lib_rpath: Option<String>,

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
    rocm_exe_rpath: String,
    rocm_lib_rpath: String,
    rocm_asan_lib_rpath: String,
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

        // RPATH defaults based on install_prefix
        let default_lib_rpath = format!("{}/lib", install_prefix.display());
        let rocm_exe_rpath = args.rocm_exe_rpath.unwrap_or_else(|| default_lib_rpath.clone());
        let rocm_lib_rpath = args.rocm_lib_rpath.unwrap_or_else(|| default_lib_rpath.clone());
        let rocm_asan_lib_rpath = args.rocm_asan_lib_rpath.unwrap_or_else(|| format!("{}/lib/asan", install_prefix.display()));

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
            rocm_exe_rpath,
            rocm_lib_rpath,
            rocm_asan_lib_rpath,
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

fn get_rocm_cmake_params(config: &AppConfig) -> Vec<String> {
    let mut params: Vec<String> = Vec::new();
    let prefix_path_str = format!("{}/llvm;{}", 
                                   config.install_prefix.display(), 
                                   config.install_prefix.display());
    params.push(format!("-DCMAKE_PREFIX_PATH={}", prefix_path_str));
    params.push(format!("-DCMAKE_BUILD_TYPE={}", config.build_type_cmake));
    params.push("-DCMAKE_VERBOSE_MAKEFILE=1".to_string());
    let cpack_generator = "DEB;RPM"; 
    params.push(format!("-DCPACK_GENERATOR={}", cpack_generator));
    params.push("-DCMAKE_INSTALL_RPATH_USE_LINK_PATH=FALSE".to_string());
    params.push(format!("-DROCM_PATCH_VERSION={}", config.rocm_libpatch_version));
    params.push(format!("-DCMAKE_INSTALL_PREFIX={}", config.install_prefix.display()));
    params.push(format!("-DCPACK_PACKAGING_INSTALL_PREFIX={}", config.install_prefix.display()));
    if config.verbose {
        println!("get_rocm_cmake_params generated: {:?}", params);
    }
    params
}

// Helper for get_rocm_common_cmake_params
fn get_cmake_path_internal(rocm_install_path: &Path, asan_enabled: bool) -> PathBuf {
    let mut cmake_path_suffix = PathBuf::from("lib").join("cmake");
    if asan_enabled {
        // as per compute_utils.sh: getCmakePath()
        cmake_path_suffix = PathBuf::from("lib").join("asan").join("cmake");
    }
    rocm_install_path.join(cmake_path_suffix)
}

fn get_rocm_common_cmake_params(config: &AppConfig) -> Vec<String> {
    let mut params: Vec<String> = Vec::new();

    // Conditional CPack debug flags
    if config.build_type_cmake == "RelWithDebInfo" { // Original script uses BUILD_TYPE for this, which maps to CMAKE_BUILD_TYPE
        params.push("-DCPACK_RPM_DEBUGINFO_PACKAGE=TRUE".to_string());
        params.push("-DCPACK_DEBIAN_DEBUGINFO_PACKAGE=TRUE".to_string());
        params.push("-DCPACK_RPM_INSTALL_WITH_EXEC=TRUE".to_string());
    }

    // General ROCm Parameters
    params.push("-DROCM_DEP_ROCMCORE=ON".to_string()); // Assuming this is always ON for rocminfo
    params.push(format!("-DCMAKE_EXE_LINKER_FLAGS_INIT=-Wl,--enable-new-dtags,--build-id=sha1,--rpath,{}", config.rocm_exe_rpath));
    params.push(format!("-DCMAKE_SHARED_LINKER_FLAGS_INIT=-Wl,--enable-new-dtags,--build-id=sha1,--rpath,{}", config.rocm_lib_rpath));
    params.push("-DFILE_REORG_BACKWARD_COMPATIBILITY=OFF".to_string());
    params.push("-DCPACK_RPM_PACKAGE_RELOCATABLE=ON".to_string());
    params.push("-DCPACK_SET_DESTDIR=OFF".to_string()); 
    params.push("-DINCLUDE_PATH_COMPATIBILITY=OFF".to_string());

    // ASan Specific Parameters
    if config.address_sanitizer_enabled {
        let asan_lib_dir = "lib/asan"; 
        let cmake_path_for_asan = get_cmake_path_internal(&config.install_prefix, true);

        params.push(format!("-DCMAKE_INSTALL_LIBDIR={}", asan_lib_dir));
        
        let asan_prefix_path_str = format!("{};{}/lib/asan;{}/llvm;{}",
                                           cmake_path_for_asan.display(),
                                           config.install_prefix.display(),
                                           config.install_prefix.display(),
                                           config.install_prefix.display());
        params.push(format!("-DCMAKE_PREFIX_PATH={}", asan_prefix_path_str));
        params.push("-DENABLE_ASAN_PACKAGING=true".to_string()); 
        params.push(format!("-DCMAKE_SHARED_LINKER_FLAGS_INIT=-Wl,--enable-new-dtags,--build-id=sha1,--rpath,{}", config.rocm_asan_lib_rpath));
    } else {
        params.push("-DCMAKE_INSTALL_LIBDIR=lib".to_string());
    }

    if config.verbose {
        println!("get_rocm_common_cmake_params generated: {:?}", params);
    }
    params
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
    
    let rocm_params = get_rocm_cmake_params(config);
    if !rocm_params.is_empty() {
        if config.verbose {
            println!("Adding rocm_cmake_params: {:?}", rocm_params);
        }
        cmake_configure_cmd.args(rocm_params);
    }
    
    let common_params = get_rocm_common_cmake_params(config);
    if !common_params.is_empty() {
        if config.verbose {
            println!("Adding rocm_common_cmake_params: {:?}", common_params);
        }
        cmake_configure_cmd.args(common_params);
    }
    
    // These are more specific and should ideally be set after the general ones
    // to ensure they take precedence if there are overlaps (though less likely for these).
    cmake_configure_cmd.arg(format!("-DBUILD_SHARED_LIBS={}", config.build_shared_libs_cmake));
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
        cmake_configure_cmd.arg("-DENABLE_ADDRESS_SANITIZER=ON"); 
        // TODO: Replicate compute_utils.sh:set_asan_env_vars if needed for child processes (e.g., CTest).
    }
    
    if config.verbose {
        println!("Final CMake configure command: {:?}", cmake_configure_cmd);
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
        let python_exe = which("python3").or_else(|_| which("python"))
            .map_err(|e| anyhow::anyhow!("python3 or python executable not found in PATH for wheel build: {}", e))?;
        
        if config.verbose {
            println!("Found python executable at: {:?}", python_exe);
            println!("Assuming setup.py is in {:?}", config.source_root);
        }

        let setup_py_path = config.source_root.join("setup.py");
        if !setup_py_path.exists() {
            if config.verbose {
                println!("Warning: setup.py not found at {:?}. Skipping wheel build.", setup_py_path);
            }
        } else {
            let mut wheel_cmd = Command::new(python_exe);
            wheel_cmd.current_dir(&config.source_root);
            wheel_cmd.arg("setup.py").arg("bdist_wheel");
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
            }
        }
    }
    
    println!("rocminfo-builder: Build operations (including packaging/wheel if requested) completed.");
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
    use super::*; 
    use std::fs as std_fs; 

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
            address_sanitizer: false, 
            wheel: false,
            gpu_list: None,
            jobs: None,
            package_type: "all".to_string(), 
            outdir_target: None,
            rocm_exe_rpath: None, // Added for tests
            rocm_lib_rpath: None, // Added for tests
            rocm_asan_lib_rpath: None, // Added for tests
            verbose: false,
        }
    }

    #[test]
    fn test_app_config_defaults() {
        let temp_dir = std::env::temp_dir();
        let dummy_source_root = temp_dir.join("test_rocminfo_source_defaults_full"); // New name to avoid conflict
        std_fs::create_dir_all(&dummy_source_root).unwrap();

        let cli_args = basic_cli_args(dummy_source_root.clone());
        let config_result = AppConfig::try_from_args(cli_args);
        assert!(config_result.is_ok());
        if let Ok(config) = config_result {
            assert_eq!(config.source_root, dummy_source_root);
            assert_eq!(config.build_dir, dummy_source_root.join("build").join("rocminfo-builder"));
            let expected_package_root = dummy_source_root.join("dist");
            assert_eq!(config.package_root, expected_package_root);
            let expected_install_prefix = expected_package_root.join("rocm");
            assert_eq!(config.install_prefix, expected_install_prefix);
            assert_eq!(config.deb_package_dir, expected_package_root.join("deb").join("rocminfo"));
            assert_eq!(config.rpm_package_dir, expected_package_root.join("rpm").join("rocminfo"));
            assert_eq!(config.build_type_cmake, "Debug");
            assert_eq!(config.rocrts_bld_type_cmake, "debug");
            assert_eq!(config.build_shared_libs_cmake, "ON");
            assert!(!config.address_sanitizer_enabled);
            // Check RPATH defaults
            let default_lib_rpath = format!("{}/lib", expected_install_prefix.display());
            assert_eq!(config.rocm_exe_rpath, default_lib_rpath);
            assert_eq!(config.rocm_lib_rpath, default_lib_rpath);
            assert_eq!(config.rocm_asan_lib_rpath, format!("{}/lib/asan", expected_install_prefix.display()));
        }
        
        std_fs::remove_dir_all(&dummy_source_root).unwrap();
    }

    #[test]
    fn test_app_config_custom_rpaths() {
        let temp_dir = std::env::temp_dir();
        let dummy_source_root = temp_dir.join("test_rocminfo_source_custom_rpaths");
        std_fs::create_dir_all(&dummy_source_root).unwrap();

        let mut cli_args = basic_cli_args(dummy_source_root.clone());
        cli_args.rocm_exe_rpath = Some("/custom/exe/rpath".to_string());
        cli_args.rocm_lib_rpath = Some("/custom/lib/rpath".to_string());
        cli_args.rocm_asan_lib_rpath = Some("/custom/asan/rpath".to_string());

        let config = AppConfig::try_from_args(cli_args).unwrap();
        assert_eq!(config.rocm_exe_rpath, "/custom/exe/rpath");
        assert_eq!(config.rocm_lib_rpath, "/custom/lib/rpath");
        assert_eq!(config.rocm_asan_lib_rpath, "/custom/asan/rpath");
        
        std_fs::remove_dir_all(&dummy_source_root).unwrap();
    }
}
```
