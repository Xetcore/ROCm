use clap::Parser;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result, anyhow}; 
use std::env; // For current_dir
use std::fs;
use std::process::Command;
use which::which;
use glob::glob;
use fs_extra::dir::CopyOptions; // Used by copy_packages, ensure it's relevant

#[derive(Parser, Debug, Clone)] // Added Clone for main function logic
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Path to the rocm-cmake source directory.
    #[arg(long, value_name = "PATH")]
    rocm_cmake_root: PathBuf,

    /// Optional: Specify the build directory.
    /// Defaults to "<rocm-cmake-root>/build/rocm-cmake-builder".
    #[arg(long, value_name = "PATH")]
    build_dir: Option<PathBuf>,

    /// Optional: Specify the root directory for packages.
    /// Defaults to "<rocm-cmake-root>/dist".
    #[arg(long, value_name = "PATH")]
    package_root: Option<PathBuf>,

    /// Optional: Install prefix (influences CMAKE_INSTALL_PREFIX).
    /// Defaults to "<package_root>/rocm".
    #[arg(long, value_name = "PATH")]
    install_prefix: Option<PathBuf>,

    /// Optional: Version patch number for CPack (ROCM_LIBPATCH_VERSION). Defaults to "0".
    #[arg(long, value_name = "VERSION", default_value = "0")]
    rocm_libpatch_version: String,

    /// Clean output and delete all intermediate work.
    #[arg(long, short = 'c')]
    clean: bool,

    /// Build a release version of the package (default is debug).
    #[arg(long, short = 'r')]
    release: bool,

    /// Build static lib (.a) instead of dynamic/shared (.so).
    #[arg(long, short = 's')]
    static_libs: bool,

    /// Enable address sanitizer (acknowledged and ignored, for compatibility).
    #[arg(long, short = 'a')]
    address_sanitizer: bool,
    
    /// Specify packaging format (e.g. "deb", "rpm", "all"). Influences which packages are copied.
    #[arg(long, short = 'p', value_name = "TYPE", default_value = "all")]
    package_type: String,

    /// Optional: Print path of output directory for specified package type (deb, rpm) and exit.
    #[arg(long)]
    outdir_target: Option<String>,

    /// Creates a python wheel package (if applicable).
    #[arg(long, short = 'w')]
    wheel: bool,

    /// Enable verbose logging.
    #[arg(long, short = 'v', action = clap::ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug)]
struct AppConfig {
    rocm_cmake_root: PathBuf,
    install_prefix: PathBuf, // Added
    build_dir: PathBuf,
    package_root: PathBuf,
    deb_package_dir: PathBuf, 
    rpm_package_dir: PathBuf, 
    rocm_libpatch_version: String, // Added
    clean: bool,
    build_type_cmake: String, 
    build_shared_libs_cmake: String, 
    package_type_to_build: String,
    wheel: bool, // Added
    verbose: bool,
    // address_sanitizer_enabled: bool, // Not explicitly used by rocm-cmake build itself
}

impl AppConfig {
    fn try_from_args(args: CliArgs) -> Result<Self> {
        let current_dir = env::current_dir().context("Failed to get current directory")?;
        let rocm_cmake_root = args.rocm_cmake_root.canonicalize()
            .with_context(|| format!("Failed to find or access rocm-cmake-root path: {:?}", args.rocm_cmake_root))?;

        // Build Directory: CLI arg or default, then ensure absolute.
        let build_dir_resolved = args.build_dir
            .map(|p| if p.is_absolute() { p } else { current_dir.join(&p) })
            .unwrap_or_else(|| rocm_cmake_root.join("build").join("rocm-cmake-builder"));
        let build_dir = if !build_dir_resolved.is_absolute() { current_dir.join(build_dir_resolved) } else { build_dir_resolved };
        // No assert!(build_dir.is_absolute()) needed here as it's for an output dir that might not exist.
            
        // Package Root Directory: CLI arg, then OUT_DIR env, then default, then ensure absolute.
        let package_root_resolved = match args.package_root {
            Some(p) => if p.is_absolute() { p } else { current_dir.join(&p) },
            None => {
                match env::var("OUT_DIR").ok() {
                    Some(out_dir_str) => {
                        let out_dir_env = PathBuf::from(out_dir_str);
                        if args.verbose { println!("Using OUT_DIR env var for package_root: {:?}", out_dir_env); }
                        // OUT_DIR is typically an output path, attempt canonicalize but fallback to using path directly if it doesn't exist.
                        out_dir_env.canonicalize().unwrap_or(out_dir_env) 
                    }
                    None => rocm_cmake_root.join("dist"),
                }
            }
        };
        let package_root = if !package_root_resolved.is_absolute() { current_dir.join(package_root_resolved) } else { package_root_resolved };


        // Install Prefix Directory: CLI arg, then ROCM_INSTALL_PATH env, then ROCM_PATH env, then default, then ensure absolute.
        let install_prefix_resolved = match args.install_prefix {
            Some(p) => if p.is_absolute() { p } else { current_dir.join(&p) },
            None => {
                match env::var("ROCM_INSTALL_PATH").ok() {
                    Some(rocm_install_str) => {
                        let rocm_install_env = PathBuf::from(rocm_install_str);
                        if args.verbose { println!("Using ROCM_INSTALL_PATH env var for install_prefix: {:?}", rocm_install_env); }
                        rocm_install_env.canonicalize().unwrap_or(rocm_install_env)
                    }
                    None => match env::var("ROCM_PATH").ok() {
                        Some(rocm_path_str) => {
                            let rocm_path_env = PathBuf::from(rocm_path_str);
                            if args.verbose { println!("Using ROCM_PATH env var for install_prefix: {:?}", rocm_path_env); }
                            rocm_path_env.canonicalize().unwrap_or(rocm_path_env)
                        }
                        None => package_root.join("rocm"), // Use the resolved package_root here
                    }
                }
            }
        };
        let install_prefix = if !install_prefix_resolved.is_absolute() { current_dir.join(install_prefix_resolved) } else { install_prefix_resolved };


        let deb_package_dir = package_root.join("deb").join("rocm-cmake");
        let rpm_package_dir = package_root.join("rpm").join("rocm-cmake");

        Ok(AppConfig {
            rocm_cmake_root,
            install_prefix,
            build_dir,
            package_root,
            deb_package_dir,
            rpm_package_dir,
            rocm_libpatch_version: args.rocm_libpatch_version, // Initialized
            clean: args.clean,
            build_type_cmake: if args.release { "Release".to_string() } else { "Debug".to_string() },
            build_shared_libs_cmake: if args.static_libs { "OFF".to_string() } else { "ON".to_string() },
            package_type_to_build: args.package_type.to_lowercase(),
            wheel: args.wheel, // Initialized
            verbose: args.verbose,
            // address_sanitizer_enabled: args.address_sanitizer, // Keep if needed for common params
        })
    }
}

fn handle_clean(config: &AppConfig) -> Result<()> {
    if config.verbose {
        println!("Cleaning operation selected.");
        println!("Attempting to remove build directory: {:?}", config.build_dir);
        println!("Attempting to remove DEB package directory: {:?}", config.deb_package_dir);
        println!("Attempting to remove RPM package directory: {:?}", config.rpm_package_dir);
        let wheel_dist_dir = config.build_dir.join("wheelhouse"); // build_dir, not package_root for wheelhouse
        println!("Attempting to remove wheel distribution directory: {:?}", wheel_dist_dir);
    }

    if config.build_dir.exists() {
        fs::remove_dir_all(&config.build_dir)
            .with_context(|| format!("Failed to remove build directory: {:?}", config.build_dir))?;
        if config.verbose {
            println!("Successfully removed build directory: {:?}", config.build_dir);
        }
    } else if config.verbose {
        println!("Build directory {:?} does not exist. Nothing to remove.", config.build_dir);
    }

    if config.deb_package_dir.exists() {
        fs::remove_dir_all(&config.deb_package_dir)
            .with_context(|| format!("Failed to remove DEB package directory: {:?}", config.deb_package_dir))?;
        if config.verbose {
            println!("Successfully removed DEB package directory: {:?}", config.deb_package_dir);
        }
    } else if config.verbose {
        println!("DEB package directory {:?} does not exist. Nothing to remove.", config.deb_package_dir);
    }

    if config.rpm_package_dir.exists() {
        fs::remove_dir_all(&config.rpm_package_dir)
            .with_context(|| format!("Failed to remove RPM package directory: {:?}", config.rpm_package_dir))?;
        if config.verbose {
            println!("Successfully removed RPM package directory: {:?}", config.rpm_package_dir);
        }
    } else if config.verbose {
        println!("RPM package directory {:?} does not exist. Nothing to remove.", config.rpm_package_dir);
    }
    
    let wheel_dist_dir = config.build_dir.join("wheelhouse");
    if wheel_dist_dir.exists() {
        fs::remove_dir_all(&wheel_dist_dir)
            .with_context(|| format!("Failed to remove wheel distribution directory: {:?}", wheel_dist_dir))?;
        if config.verbose {
            println!("Successfully removed wheel distribution directory: {:?}", wheel_dist_dir);
        }
    } else if config.verbose {
        println!("Wheel distribution directory {:?} does not exist. Nothing to remove.", wheel_dist_dir);
    }

    println!("Clean operation completed.");
    Ok(())
}

// Copied from rocminfo-builder
fn get_rocm_cmake_params(config: &AppConfig) -> Vec<String> {
    let mut params: Vec<String> = Vec::new();

    // Using config.install_prefix which is now part of AppConfig for rocm-cmake-builder
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

    if config.verbose {
        println!("Running CMake configure step...");
    }
    let mut cmake_configure_cmd = Command::new(&cmake_exe);
    cmake_configure_cmd.current_dir(&config.build_dir);
    cmake_configure_cmd.arg(config.rocm_cmake_root.as_os_str()); // Source directory
    
    // Add params from get_rocm_cmake_params()
    let rocm_params = get_rocm_cmake_params(config);
    if !rocm_params.is_empty() {
        if config.verbose {
            println!("Adding rocm_cmake_params: {:?}", rocm_params);
        }
        cmake_configure_cmd.args(rocm_params);
    }
    
    // Add specific flags for rocm-cmake build after common ones
    cmake_configure_cmd.arg(format!("-DBUILD_SHARED_LIBS={}", config.build_shared_libs_cmake));
    cmake_configure_cmd.arg("-DCPACK_SET_DESTDIR=OFF"); // From original build_rocm-cmake.sh
    cmake_configure_cmd.arg("-DROCM_DISABLE_LDCONFIG=ON"); // From original build_rocm-cmake.sh
    
    if config.verbose {
        println!("CMake configure command: {:?}", cmake_configure_cmd);
    }
    let configure_output = cmake_configure_cmd.output()
        .with_context(|| "Failed to execute CMake configure command")?;
    if !configure_output.status.success() {
        let stderr_output = String::from_utf8_lossy(&configure_output.stderr);
        let stdout_output = String::from_utf8_lossy(&configure_output.stdout);
        return Err(anyhow::anyhow!(
            "CMake configure command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
            configure_output.status, stderr_output, stdout_output
        ));
    }
    if config.verbose {
        println!("CMake configure stdout:\n{}", String::from_utf8_lossy(&configure_output.stdout));
        if !configure_output.stderr.is_empty() {
             eprintln!("CMake configure stderr:\n{}", String::from_utf8_lossy(&configure_output.stderr));
        }
        println!("CMake configure step completed successfully.");
    }

    if config.verbose {
        println!("Running CMake build (install) step..."); // Original script does `cmake --build . -- install`
    }
    let mut cmake_build_install_cmd = Command::new(&cmake_exe);
    cmake_build_install_cmd.current_dir(&config.build_dir);
    // The original script uses `cmake --build . -- install` (two dashes for install)
    // which seems to be an alias for `--build . --target install` for some generators.
    // Let's use the more explicit `--target install`.
    cmake_build_install_cmd.arg("--build").arg(".").arg("--target").arg("install");
    if config.verbose {
        println!("CMake build (install) command: {:?}", cmake_build_install_cmd);
    }
    let build_install_output = cmake_build_install_cmd.output()
        .with_context(|| "Failed to execute CMake build (install) command")?;
    if !build_install_output.status.success() {
        let stderr_output = String::from_utf8_lossy(&build_install_output.stderr);
        let stdout_output = String::from_utf8_lossy(&build_install_output.stdout);
        return Err(anyhow::anyhow!(
            "CMake build (install) command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
            build_install_output.status, stderr_output, stdout_output
        ));
    }
    if config.verbose {
        println!("CMake build (install) stdout:\n{}", String::from_utf8_lossy(&build_install_output.stdout));
        if !build_install_output.stderr.is_empty() {
            eprintln!("CMake build (install) stderr:\n{}", String::from_utf8_lossy(&build_install_output.stderr));
        }
        println!("CMake build (install) step completed successfully.");
    }

    if config.verbose {
        println!("Running CMake package step...");
    }
    let mut cmake_package_cmd = Command::new(&cmake_exe);
    cmake_package_cmd.current_dir(&config.build_dir);
    cmake_package_cmd.arg("--build").arg(".").arg("--target").arg("package");
    if config.verbose {
        println!("CMake package command: {:?}", cmake_package_cmd);
    }
    let package_output = cmake_package_cmd.output()
        .with_context(|| "Failed to execute CMake package command")?;
    if !package_output.status.success() {
        let stderr_output = String::from_utf8_lossy(&package_output.stderr);
        let stdout_output = String::from_utf8_lossy(&package_output.stdout);
        return Err(anyhow::anyhow!(
            "CMake package command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
            package_output.status, stderr_output, stdout_output
        ));
    }
    if config.verbose {
        println!("CMake package stdout:\n{}", String::from_utf8_lossy(&package_output.stdout));
        if !package_output.stderr.is_empty() {
            eprintln!("CMake package stderr:\n{}", String::from_utf8_lossy(&package_output.stderr));
        }
        println!("CMake package step completed successfully.");
    }

    copy_packages(config)?;
    
    if config.wheel {
        if config.verbose {
            println!("Wheel package creation requested.");
        }
        
        let python_exe = which("python3").or_else(|_| which("python"))
            .map_err(|e| anyhow::anyhow!("python3 or python executable not found in PATH: {}. Ensure Python is installed and in PATH.", e))?;
        
        if config.verbose {
            println!("Found python executable at: {:?}", python_exe);
            let setup_py_path = config.rocm_cmake_root.join("setup.py");
            println!("Checking for setup.py at: {:?}", setup_py_path);
            if !setup_py_path.exists() {
                 println!("Warning: setup.py not found at {:?}. Skipping wheel build for rocm-cmake.", setup_py_path);
                 // For rocm-cmake, not having setup.py might be expected. Do not error out.
            } else {
                let mut wheel_cmd = Command::new(python_exe);
                wheel_cmd.current_dir(&config.rocm_cmake_root); 
                wheel_cmd.arg("setup.py").arg("bdist_wheel");
                let wheel_dist_dir = config.build_dir.join("wheelhouse"); 
                fs::create_dir_all(&wheel_dist_dir)
                    .with_context(|| format!("Failed to create wheel distribution directory: {:?}", wheel_dist_dir))?;
                wheel_cmd.arg("--dist-dir").arg(&wheel_dist_dir);

                if config.verbose {
                    println!("Executing wheel command: {:?}", wheel_cmd);
                }
                let wheel_output = wheel_cmd.output()
                    .with_context(|| format!("Failed to execute python setup.py bdist_wheel for rocm-cmake. Ensure python, setuptools, and wheel are installed."))?;
                if !wheel_output.status.success() {
                    let stderr_output = String::from_utf8_lossy(&wheel_output.stderr);
                    let stdout_output = String::from_utf8_lossy(&wheel_output.stdout);
                    return Err(anyhow::anyhow!(
                        "Python wheel command for rocm-cmake failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
                        wheel_output.status, stderr_output, stdout_output
                    ));
                }
                if config.verbose {
                    println!("Python wheel stdout:\n{}", String::from_utf8_lossy(&wheel_output.stdout));
                    if !wheel_output.stderr.is_empty() {
                        eprintln!("Python wheel stderr:\n{}", String::from_utf8_lossy(&wheel_output.stderr));
                    }
                    println!("Python wheel for rocm-cmake created successfully in {:?}", wheel_dist_dir);
                }
            }
        }
    }

    println!("ROCM CMake Builder (Rust) - Build operations (including packaging/wheel if requested) completed.");
    Ok(())
}

fn copy_packages(config: &AppConfig) -> Result<()> {
    if config.verbose {
        println!("Copying packages...");
    }

    let types_to_copy_str = config.package_type_to_build.to_lowercase();
    let copy_deb = types_to_copy_str == "all" || types_to_copy_str == "deb";
    let copy_rpm = types_to_copy_str == "all" || types_to_copy_str == "rpm";

    if copy_deb {
        fs::create_dir_all(&config.deb_package_dir)
            .with_context(|| format!("Failed to create DEB package directory: {:?}", config.deb_package_dir))?;
        let deb_pattern = config.build_dir.join("rocm-cmake*.deb");
        if config.verbose {
            println!("Searching for DEB packages with pattern: {:?}", deb_pattern.to_string_lossy());
        }
        for entry in glob(&deb_pattern.to_string_lossy())? {
            match entry {
                Ok(path) => {
                    let file_name = path.file_name().ok_or_else(|| anyhow::anyhow!("Failed to get filename from {:?}", path))?;
                    let dest_path = config.deb_package_dir.join(file_name);
                    if config.verbose {
                        println!("Copying {:?} to {:?}", path, dest_path);
                    }
                    fs::copy(&path, &dest_path)
                        .with_context(|| format!("Failed to copy {:?} to {:?}", path, dest_path))?;
                }
                Err(e) => return Err(anyhow::anyhow!(e)),
            }
        }
    }

    if copy_rpm {
        fs::create_dir_all(&config.rpm_package_dir)
            .with_context(|| format!("Failed to create RPM package directory: {:?}", config.rpm_package_dir))?;
        let rpm_pattern = config.build_dir.join("rocm-cmake*.rpm");
         if config.verbose {
            println!("Searching for RPM packages with pattern: {:?}", rpm_pattern.to_string_lossy());
        }
        for entry in glob(&rpm_pattern.to_string_lossy())? {
            match entry {
                Ok(path) => {
                    let file_name = path.file_name().ok_or_else(|| anyhow::anyhow!("Failed to get filename from {:?}", path))?;
                    let dest_path = config.rpm_package_dir.join(file_name);
                     if config.verbose {
                        println!("Copying {:?} to {:?}", path, dest_path);
                    }
                    fs::copy(&path, &dest_path)
                        .with_context(|| format!("Failed to copy {:?} to {:?}", path, dest_path))?;
                }
                Err(e) => return Err(anyhow::anyhow!(e)),
            }
        }
    }
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
    if cli_args.address_sanitizer && cli_args.verbose { 
        println!("Note: --address-sanitizer flag is acknowledged but currently ignored in this Rust version (rocm-cmake build does not typically use ASan).");
    }
    
    if let Some(ref pkg_to_print) = cli_args.outdir_target {
        // Need to clone cli_args to pass to AppConfig::try_from_args for outdir logic
        // as try_from_args consumes its input.
        let config_for_outdir = AppConfig::try_from_args(cli_args.clone())?;
        return handle_outdir(&config_for_outdir, pkg_to_print);
    }
    
    let config = AppConfig::try_from_args(cli_args)?; 

    if config.verbose {
        println!("Resolved AppConfig: {:#?}", &config);
    }

    if config.clean {
        return handle_clean(&config); 
    }
    
    handle_build(&config)?; 
    
    println!("ROCM CMake Builder (Rust) - Operation complete.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*; 
    use std::fs as std_fs; 

    fn basic_cli_args(rocm_cmake_root_path: PathBuf) -> CliArgs {
        CliArgs {
            rocm_cmake_root: rocm_cmake_root_path,
            build_dir: None,
            package_root: None,
            install_prefix: None, // Added
            rocm_libpatch_version: "0".to_string(), // Added
            clean: false,
            release: false,
            static_libs: false,
            address_sanitizer: false,
            package_type: "all".to_string(),
            outdir_target: None,
            wheel: false, // Added
            verbose: false,
        }
    }

    #[test]
    fn test_app_config_defaults() {
        let temp_dir = std::env::temp_dir();
        let dummy_rocm_root = temp_dir.join("test_rocm_cmake_root_defaults");
        std_fs::create_dir_all(&dummy_rocm_root).unwrap();

        let cli_args = basic_cli_args(dummy_rocm_root.clone());
        let config_result = AppConfig::try_from_args(cli_args);
        assert!(config_result.is_ok());
        if let Ok(config) = config_result {
            assert_eq!(config.rocm_cmake_root, dummy_rocm_root);
            assert_eq!(config.build_dir, dummy_rocm_root.join("build").join("rocm-cmake-builder"));
            let expected_package_root = dummy_rocm_root.join("dist");
            assert_eq!(config.package_root, expected_package_root);
            assert_eq!(config.install_prefix, expected_package_root.join("rocm")); // Check default install_prefix
            assert_eq!(config.deb_package_dir, expected_package_root.join("deb").join("rocm-cmake"));
            assert_eq!(config.rpm_package_dir, expected_package_root.join("rpm").join("rocm-cmake"));
            assert_eq!(config.build_type_cmake, "Debug");
            assert_eq!(config.build_shared_libs_cmake, "ON");
            assert_eq!(config.rocm_libpatch_version, "0");
            assert!(!config.wheel);
        }
        
        std_fs::remove_dir_all(&dummy_rocm_root).unwrap();
    }

    #[test]
    fn test_app_config_custom_install_prefix() {
        let temp_dir = std::env::temp_dir();
        let dummy_rocm_root = temp_dir.join("test_rocm_cmake_custom_install");
        std_fs::create_dir_all(&dummy_rocm_root).unwrap();
        let custom_install = temp_dir.join("my_custom_rocm_install");

        let mut cli_args = basic_cli_args(dummy_rocm_root.clone());
        cli_args.install_prefix = Some(custom_install.clone());
        
        let config = AppConfig::try_from_args(cli_args).unwrap();
        assert_eq!(config.install_prefix, custom_install);
        
        std_fs::remove_dir_all(&dummy_rocm_root).unwrap();
    }
     #[test]
    fn test_app_config_wheel_flag() {
        let temp_dir = std::env::temp_dir();
        let dummy_rocm_root = temp_dir.join("test_rocm_cmake_wheel_flag");
        std_fs::create_dir_all(&dummy_rocm_root).unwrap();

        let mut cli_args = basic_cli_args(dummy_rocm_root.clone());
        cli_args.wheel = true;
        
        let config = AppConfig::try_from_args(cli_args).unwrap();
        assert!(config.wheel);
        
        std_fs::remove_dir_all(&dummy_rocm_root).unwrap();
    }
}
```
