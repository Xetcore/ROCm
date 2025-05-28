use clap::Parser;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result, anyhow}; // For Result type and context
use std::fs;
use std::process::Command; // Added for running external commands
use which::which; // Added to find cmake executable

// CliArgs struct (as before)
/// Rust equivalent of the build_rocm-cmake.sh script.
/// Handles building and packaging of the rocm-cmake component.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Path to the rocm-cmake source directory.
    #[arg(long, value_name = "PATH")]
    rocm_cmake_root: PathBuf,

    /// Optional: Specify the build directory.
    /// Defaults to "<rocm-cmake-root>/build/rocm-cmake-builder"
    #[arg(long, value_name = "PATH")]
    build_dir: Option<PathBuf>,

    /// Optional: Specify the root directory for packages.
    /// Defaults to "<rocm-cmake-root>/dist"
    #[arg(long, value_name = "PATH")]
    package_root: Option<PathBuf>,

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

    /// Enable verbose logging.
    #[arg(long, short = 'v', action = clap::ArgAction::SetTrue)]
    verbose: bool,
}


// AppConfig struct (as before)
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
    verbose: bool,
}

impl AppConfig {
    fn try_from_args(args: CliArgs) -> Result<Self> {
        let rocm_cmake_root = args.rocm_cmake_root.canonicalize()
            .with_context(|| format!("Failed to find or access rocm-cmake-root path: {:?}", args.rocm_cmake_root))?;

        let build_dir_default = rocm_cmake_root.join("build").join("rocm-cmake-builder");
        let build_dir = args.build_dir
            .map(|p| if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) }.canonicalize().ok().unwrap_or(p) ) // keep as is if canonicalize fails for optional paths
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
            verbose: args.verbose,
        })
    }
}

// handle_clean function (as before)
fn handle_clean(config: &AppConfig) -> Result<()> {
    if config.verbose {
        println!("Cleaning operation selected.");
        println!("Attempting to remove build directory: {:?}", config.build_dir);
        println!("Attempting to remove DEB package directory: {:?}", config.deb_package_dir);
        println!("Attempting to remove RPM package directory: {:?}", config.rpm_package_dir);
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
    
    println!("Clean operation completed.");
    Ok(())
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
    cmake_configure_cmd.arg(config.rocm_cmake_root.as_os_str()); // Source directory
    
    // Add basic CMake arguments
    cmake_configure_cmd.arg(format!("-DCMAKE_BUILD_TYPE={}", config.build_type_cmake));
    cmake_configure_cmd.arg(format!("-DBUILD_SHARED_LIBS={}", config.build_shared_libs_cmake));
    // Hardcoded arguments from the original script
    cmake_configure_cmd.arg("-DCPACK_SET_DESTDIR=OFF");
    cmake_configure_cmd.arg("-DROCM_DISABLE_LDCONFIG=ON");
    
    // TODO: Add logic for rocm_cmake_params() here in a later phase.
    // For now, these are the minimal parameters.

    if config.verbose {
        println!("CMake configure command: {:?}", cmake_configure_cmd);
    }
    let configure_status = cmake_configure_cmd.status()
        .with_context(|| "Failed to execute CMake configure command")?;
    if !configure_status.success() {
        return Err(anyhow::anyhow!("CMake configure command failed with status: {}", configure_status));
    }
    if config.verbose {
        println!("CMake configure step completed successfully.");
    }

    // CMake Build & Install Step
    if config.verbose {
        println!("Running CMake build and install step...");
    }
    let mut cmake_build_install_cmd = Command::new(&cmake_exe);
    cmake_build_install_cmd.current_dir(&config.build_dir);
    cmake_build_install_cmd.arg("--build").arg(".");
    cmake_build_install_cmd.arg("--target").arg("install");
    // The original script uses `cmake --build . -- install` (two dashes for install)
    // but common practice for build+target is `cmake --build . --target install`.
    // Let's stick to the latter as it's more standard.

    if config.verbose {
        println!("CMake build & install command: {:?}", cmake_build_install_cmd);
    }
    let build_install_status = cmake_build_install_cmd.status()
        .with_context(|| "Failed to execute CMake build and install command")?;
    if !build_install_status.success() {
        return Err(anyhow::anyhow!("CMake build and install command failed with status: {}", build_install_status));
    }

    println!("Build and install operations completed.");
    // Packaging and other steps will be added in later phases.
    Ok(())
}

fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    if cli_args.verbose {
        println!("Raw CLI arguments: {:#?}", cli_args);
    }
    if cli_args.address_sanitizer && cli_args.verbose {
        println!("Note: --address-sanitizer flag is acknowledged but currently ignored in this Rust version.");
    }
    
    let config = AppConfig::try_from_args(cli_args)?;

    if config.verbose {
        println!("Resolved AppConfig: {:#?}", config);
    }

    if config.clean {
        return handle_clean(&config); 
    }

    // If not cleaning, proceed to build (and later, other actions)
    handle_build(&config)?;
    
    println!("ROCM CMake Builder (Rust) - Operation complete.");
    Ok(())
}
```
