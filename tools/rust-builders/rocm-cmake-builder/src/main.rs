use clap::Parser;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result, anyhow}; 
use std::fs;
use std::process::Command; // Used for running external commands
use which::which; // Used to find executables like cmake, python3
use glob::glob; // Used by copy_packages
use fs_extra::dir::CopyOptions; // Used by copy_packages, ensure it's relevant or remove if copy_packages changes

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    #[arg(long, value_name = "PATH")]
    rocm_cmake_root: PathBuf,
    #[arg(long, value_name = "PATH")]
    build_dir: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    package_root: Option<PathBuf>,
    #[arg(long, short = 'c')]
    clean: bool,
    #[arg(long, short = 'r')]
    release: bool,
    #[arg(long, short = 's')]
    static_libs: bool,
    #[arg(long, short = 'a')]
    address_sanitizer: bool,
    #[arg(long, short = 'p', value_name = "TYPE", default_value = "all")]
    package_type: String,
    #[arg(long)]
    outdir_target: Option<String>,
    #[arg(long, short = 'w')]
    wheel: bool, 
    #[arg(long, short = 'v', action = clap::ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug)]
struct AppConfig {
    rocm_cmake_root: PathBuf,
    build_dir: PathBuf,
    package_root: PathBuf,
    deb_package_dir: PathBuf, 
    rpm_package_dir: PathBuf, 
    clean: bool,
    build_type_cmake: String, 
    build_shared_libs_cmake: String, 
    package_type_to_build: String,
    wheel: bool, 
    verbose: bool,
}

impl AppConfig {
    fn try_from_args(args: CliArgs) -> Result<Self> {
        let rocm_cmake_root = args.rocm_cmake_root.canonicalize()
            .with_context(|| format!("Failed to find or access rocm-cmake-root path: {:?}", args.rocm_cmake_root))?;

        let build_dir_default = rocm_cmake_root.join("build").join("rocm-cmake-builder");
        let build_dir = args.build_dir
            .map(|p| if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) }.canonicalize().ok().unwrap_or(p) )
            .unwrap_or(build_dir_default);
            
        let package_root_default = rocm_cmake_root.join("dist");
        let package_root = args.package_root
            .map(|p| if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) }.canonicalize().ok().unwrap_or(p) )
            .unwrap_or(package_root_default);

        let deb_package_dir = package_root.join("deb").join("rocm-cmake");
        let rpm_package_dir = package_root.join("rpm").join("rocm-cmake");

        Ok(AppConfig {
            rocm_cmake_root,
            build_dir,
            package_root,
            deb_package_dir,
            rpm_package_dir,
            clean: args.clean,
            build_type_cmake: if args.release { "Release".to_string() } else { "Debug".to_string() },
            build_shared_libs_cmake: if args.static_libs { "OFF".to_string() } else { "ON".to_string() },
            package_type_to_build: args.package_type.to_lowercase(),
            wheel: args.wheel,
            verbose: args.verbose,
        })
    }
}

fn handle_clean(config: &AppConfig) -> Result<()> {
    if config.verbose {
        println!("Cleaning operation selected.");
        println!("Attempting to remove build directory: {:?}", config.build_dir);
        println!("Attempting to remove DEB package directory: {:?}", config.deb_package_dir);
        println!("Attempting to remove RPM package directory: {:?}", config.rpm_package_dir);
        let wheel_dist_dir = config.build_dir.join("wheelhouse");
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

fn get_rocm_cmake_params(_config: &AppConfig) -> Vec<String> {
    // Placeholder for now, as per Phase 3 requirements.
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
    cmake_configure_cmd.arg(config.rocm_cmake_root.as_os_str());
    let rocm_params = get_rocm_cmake_params(config);
    if !rocm_params.is_empty() {
        if config.verbose {
            println!("Adding rocm_cmake_params: {:?}", rocm_params);
        }
        cmake_configure_cmd.args(rocm_params);
    }
    cmake_configure_cmd.arg(format!("-DCMAKE_BUILD_TYPE={}", config.build_type_cmake));
    cmake_configure_cmd.arg(format!("-DBUILD_SHARED_LIBS={}", config.build_shared_libs_cmake));
    cmake_configure_cmd.arg("-DCPACK_SET_DESTDIR=OFF");
    cmake_configure_cmd.arg("-DROCM_DISABLE_LDCONFIG=ON");
    
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

    // CMake Build & Install Step
    if config.verbose {
        println!("Running CMake build and install step...");
    }
    let mut cmake_build_install_cmd = Command::new(&cmake_exe);
    cmake_build_install_cmd.current_dir(&config.build_dir);
    cmake_build_install_cmd.arg("--build").arg(".").arg("--target").arg("install");
    if config.verbose {
        println!("CMake build & install command: {:?}", cmake_build_install_cmd);
    }
    let build_install_output = cmake_build_install_cmd.output()
        .with_context(|| "Failed to execute CMake build and install command")?;
    if !build_install_output.status.success() {
        let stderr_output = String::from_utf8_lossy(&build_install_output.stderr);
        let stdout_output = String::from_utf8_lossy(&build_install_output.stdout);
        return Err(anyhow::anyhow!(
            "CMake build and install command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
            build_install_output.status, stderr_output, stdout_output
        ));
    }
    if config.verbose {
        println!("CMake build & install stdout:\n{}", String::from_utf8_lossy(&build_install_output.stdout));
        if !build_install_output.stderr.is_empty() {
            eprintln!("CMake build & install stderr:\n{}", String::from_utf8_lossy(&build_install_output.stderr));
        }
        println!("CMake build and install step completed successfully.");
    }

    // CMake Package Step
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
                 return Err(anyhow::anyhow!("setup.py not found at {:?}. Cannot build wheel.", setup_py_path));
            }
        }

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
            .with_context(|| format!("Failed to execute python setup.py bdist_wheel. Ensure python, setuptools, and wheel are installed and setup.py exists at {:?}", config.rocm_cmake_root))?;
        if !wheel_output.status.success() {
            let stderr_output = String::from_utf8_lossy(&wheel_output.stderr);
            let stdout_output = String::from_utf8_lossy(&wheel_output.stdout);
            return Err(anyhow::anyhow!(
                "Python wheel command failed with status: {}.\nStderr:\n{}\nStdout:\n{}",
                wheel_output.status, stderr_output, stdout_output
            ));
        }
        if config.verbose {
            println!("Python wheel stdout:\n{}", String::from_utf8_lossy(&wheel_output.stdout));
            if !wheel_output.stderr.is_empty() {
                eprintln!("Python wheel stderr:\n{}", String::from_utf8_lossy(&wheel_output.stderr));
            }
            println!("Python wheel created successfully in {:?}", wheel_dist_dir);
        }
    }

    println!("Build, install, and packaging operations completed.");
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
        println!("Note: --address-sanitizer flag is acknowledged but currently ignored in this Rust version.");
    }
    
    if let Some(ref pkg_to_print) = cli_args.outdir_target {
        let config_for_outdir = AppConfig::try_from_args(CliArgs { 
            rocm_cmake_root: cli_args.rocm_cmake_root.clone(),
            build_dir: cli_args.build_dir.clone(),
            package_root: cli_args.package_root.clone(),
            clean: false, 
            release: false, 
            static_libs: false, 
            address_sanitizer: false,
            package_type: cli_args.package_type.clone(), 
            outdir_target: None, 
            wheel: false,
            verbose: cli_args.verbose,
        })?;
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
    use super::*; // Import items from the parent module (main.rs)

    #[test]
    fn basic_app_config_defaults() {
        // This test is more of an integration test as it touches the filesystem
        // to create a dummy rocm_cmake_root for canonicalization.
        // It's a basic check that AppConfig::try_from_args can be called.
        
        let temp_dir = std::env::temp_dir();
        let dummy_rocm_root = temp_dir.join("test_rocm_cmake_root");
        fs::create_dir_all(&dummy_rocm_root).unwrap();

        let cli_args = CliArgs {
            rocm_cmake_root: dummy_rocm_root.clone(),
            build_dir: None,
            package_root: None,
            clean: false,
            release: false,
            static_libs: false,
            address_sanitizer: false,
            package_type: "all".to_string(),
            outdir_target: None,
            wheel: false, // Added wheel field
            verbose: false,
        };

        let config_result = AppConfig::try_from_args(cli_args);
        assert!(config_result.is_ok());
        if let Ok(config) = config_result {
            assert_eq!(config.rocm_cmake_root, dummy_rocm_root);
            assert_eq!(config.build_dir, dummy_rocm_root.join("build").join("rocm-cmake-builder"));
            assert_eq!(config.package_root, dummy_rocm_root.join("dist"));
            assert_eq!(config.deb_package_dir, dummy_rocm_root.join("dist").join("deb").join("rocm-cmake"));
            assert_eq!(config.rpm_package_dir, dummy_rocm_root.join("dist").join("rpm").join("rocm-cmake"));
            assert_eq!(config.build_type_cmake, "Debug");
            assert_eq!(config.build_shared_libs_cmake, "ON");
            assert!(!config.wheel); // Check wheel default
        }
        
        fs::remove_dir_all(&dummy_rocm_root).unwrap(); // Clean up
    }

    #[test]
    fn test_release_flag_propagates_to_config() {
        let temp_dir = std::env::temp_dir();
        let dummy_rocm_root = temp_dir.join("test_rocm_cmake_root_release");
        fs::create_dir_all(&dummy_rocm_root).unwrap();

        let cli_args = CliArgs {
            rocm_cmake_root: dummy_rocm_root.clone(),
            release: true, // Test release flag
            // ... other fields with default values
            build_dir: None,
            package_root: None,
            clean: false,
            static_libs: false,
            address_sanitizer: false,
            package_type: "all".to_string(),
            outdir_target: None,
            wheel: false,
            verbose: false,
        };
        let config = AppConfig::try_from_args(cli_args).unwrap();
        assert_eq!(config.build_type_cmake, "Release");
        
        fs::remove_dir_all(&dummy_rocm_root).unwrap(); // Clean up
    }

    #[test]
    fn test_static_libs_flag_propagates_to_config() {
        let temp_dir = std::env::temp_dir();
        let dummy_rocm_root = temp_dir.join("test_rocm_cmake_root_static");
        fs::create_dir_all(&dummy_rocm_root).unwrap();

        let cli_args = CliArgs {
            rocm_cmake_root: dummy_rocm_root.clone(),
            static_libs: true, // Test static_libs flag
            // ... other fields with default values
            build_dir: None,
            package_root: None,
            clean: false,
            release: false,
            address_sanitizer: false,
            package_type: "all".to_string(),
            outdir_target: None,
            wheel: false,
            verbose: false,
        };
        let config = AppConfig::try_from_args(cli_args).unwrap();
        assert_eq!(config.build_shared_libs_cmake, "OFF");

        fs::remove_dir_all(&dummy_rocm_root).unwrap(); // Clean up
    }
    
    #[test]
    fn test_wheel_flag_propagates_to_config() {
        let temp_dir = std::env::temp_dir();
        let dummy_rocm_root = temp_dir.join("test_rocm_cmake_root_wheel");
        fs::create_dir_all(&dummy_rocm_root).unwrap();

        let cli_args = CliArgs {
            rocm_cmake_root: dummy_rocm_root.clone(),
            wheel: true, // Test wheel flag
            build_dir: None,
            package_root: None,
            clean: false,
            release: false,
            static_libs: false,
            address_sanitizer: false,
            package_type: "all".to_string(),
            outdir_target: None,
            verbose: false,
        };
        let config = AppConfig::try_from_args(cli_args).unwrap();
        assert!(config.wheel);

        fs::remove_dir_all(&dummy_rocm_root).unwrap(); // Clean up
    }


    // Placeholder for future tests of get_rocm_cmake_params
    // #[test]
    // fn test_get_rocm_cmake_params_defaults() {
    //     // This will need a mock AppConfig or more setup once implemented
    //     let (config, _cli_args_for_config_only_if_needed_later) = AppConfig::try_from_args(...); // simplified
    //     let params = get_rocm_cmake_params(&config);
    //     // assert!(params.contains(&"-DSOME_DEFAULT_PARAM=ON".to_string()));
    // }
}
```
