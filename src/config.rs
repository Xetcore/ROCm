use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use log::debug;
use std::path::{Path, PathBuf};
use std::fs;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, value_name = "DIR", default_value = "./build", help = "Directory for build artifacts")]
    pub build_dir: PathBuf,

    #[arg(short, long, value_name = "DIR", help = "Optional installation directory for built packages")]
    pub install_dir: Option<PathBuf>,

    #[arg(short, long, value_name = "PACKAGE", help = "Specific packages to target (comma-separated or multiple times)")]
    pub packages: Vec<String>,

    #[arg(long, default_value_t = false, help = "Enable verbose output")]
    pub verbose: bool,

    #[arg(long, value_name = "TYPE", default_value = "Release", help = "CMake build type (e.g., Release, Debug)")]
    pub build_type: String,

    #[arg(long = "cmake-arg", value_name = "ARG", help = "Custom arguments to pass to CMake configure (e.g., -DVAR=VAL). Can be used multiple times.")]
    pub cmake_args: Vec<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Builds the specified ROCm packages
    Build,
    /// Cleans the build and (optionally) install directories
    Clean,
    /// Prints the output directory for specified packages
    Outdir {
        #[arg(required = true, value_name = "PACKAGE", help = "Specific packages to get output directory for (comma-separated or multiple times)")]
        packages: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub build_dir: PathBuf,
    pub install_dir: Option<PathBuf>,
    pub packages: Vec<String>,
    pub rocm_cmake_path: PathBuf,
    pub source_dir: PathBuf, // Root of the rocm-cmake checkout
    pub build_type: String,
    pub cmake_args: Vec<String>,
}

impl Config {
    pub fn from_cli(cli: Cli) -> Result<Self> {
        let current_dir = std::env::current_dir().context("Failed to get current directory")?;
        
        let build_dir = if cli.build_dir.is_absolute() {
            cli.build_dir
        } else {
            current_dir.join(cli.build_dir)
        };
        debug!("Resolved build directory: {}", build_dir.display());

        let install_dir = cli.install_dir.map(|p| {
            if p.is_absolute() {
                p
            } else {
                current_dir.join(p)
            }
        });
        if let Some(ref idir) = install_dir {
            debug!("Resolved install directory: {}", idir.display());
        }

        // Assuming rocm-cmake is in the current directory or a subdirectory
        // A more robust solution might involve searching or a config file
        let source_dir = current_dir.clone();
        let rocm_cmake_path = source_dir.join("rocm-cmake"); 
        if !rocm_cmake_path.exists() || !rocm_cmake_path.is_dir() {
            // Attempt to find rocm-cmake in parent directories up to a certain limit
            let mut search_dir = current_dir.clone();
            let mut found_rocm_cmake = false;
            for _ in 0..5 { // Limit search depth
                if search_dir.join("rocm-cmake").is_dir() {
                    rocm_cmake_path = search_dir.join("rocm-cmake");
                    source_dir = search_dir;
                    found_rocm_cmake = true;
                    break;
                }
                if let Some(parent) = search_dir.parent() {
                    search_dir = parent.to_path_buf();
                } else {
                    break;
                }
            }
            if !found_rocm_cmake {
                 return Err(anyhow!("'rocm-cmake' directory not found in current or parent directories. Please run from the root of the rocm-cmake project or ensure it's a subdirectory."));
            }
        }
        debug!("Determined source directory: {}", source_dir.display());
        debug!("Using rocm-cmake path: {}", rocm_cmake_path.display());


        fs::create_dir_all(&build_dir)
            .with_context(|| format!("Failed to create build directory: {}", build_dir.display()))?;
        if let Some(ref idir) = install_dir {
            fs::create_dir_all(idir)
                .with_context(|| format!("Failed to create install directory: {}", idir.display()))?;
        }

        Ok(Config {
            build_dir,
            install_dir,
            packages: cli.packages,
            rocm_cmake_path,
            source_dir,
            build_type: cli.build_type.clone(),
            cmake_args: cli.cmake_args.clone(),
        })
    }

    pub fn get_package_build_dir(&self, package_name: &str) -> PathBuf {
        self.build_dir.join(package_name)
    }

    pub fn get_package_install_dir(&self, package_name: &str) -> Option<PathBuf> {
        self.install_dir.as_ref().map(|idir| idir.join(package_name))
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Config, Commands};
    use clap::Parser;
    use std::path::PathBuf;

    // Helper function to create a dummy rocm-cmake directory for tests
    // to prevent Config::from_cli from failing when it checks for this directory.
    fn setup_dummy_rocm_cmake_dir() -> tempfile::TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("rocm-cmake")).unwrap();
        // Change current directory to the temp_dir so that Config::from_cli can find rocm-cmake
        // std::env::set_current_dir(temp_dir.path()).unwrap(); // This can cause issues with test parallelism
        temp_dir
    }


    #[test]
    fn test_default_build_type() {
        let _dummy_dir_guard = setup_dummy_rocm_cmake_dir();
        let current_dir_before_test = std::env::current_dir().unwrap();
        std::env::set_current_dir(_dummy_dir_guard.path()).unwrap();

        let cli = Cli::parse_from(["mytool", "build"]);
        assert_eq!(cli.build_type, "Release");
        let config = Config::from_cli(cli).unwrap();
        assert_eq!(config.build_type, "Release");

        std::env::set_current_dir(current_dir_before_test).unwrap();
    }

    #[test]
    fn test_custom_build_type() {
        let _dummy_dir_guard = setup_dummy_rocm_cmake_dir();
        let current_dir_before_test = std::env::current_dir().unwrap();
        std::env::set_current_dir(_dummy_dir_guard.path()).unwrap();

        let cli = Cli::parse_from(["mytool", "--build-type", "Debug", "build"]);
        assert_eq!(cli.build_type, "Debug");
        let config = Config::from_cli(cli).unwrap();
        assert_eq!(config.build_type, "Debug");

        std::env::set_current_dir(current_dir_before_test).unwrap();
    }

    #[test]
    fn test_no_cmake_args() {
        let _dummy_dir_guard = setup_dummy_rocm_cmake_dir();
        let current_dir_before_test = std::env::current_dir().unwrap();
        std::env::set_current_dir(_dummy_dir_guard.path()).unwrap();

        let cli = Cli::parse_from(["mytool", "build"]);
        assert!(cli.cmake_args.is_empty());
        let config = Config::from_cli(cli).unwrap();
        assert!(config.cmake_args.is_empty());

        std::env::set_current_dir(current_dir_before_test).unwrap();
    }

    #[test]
    fn test_single_cmake_arg() {
        let _dummy_dir_guard = setup_dummy_rocm_cmake_dir();
        let current_dir_before_test = std::env::current_dir().unwrap();
        std::env::set_current_dir(_dummy_dir_guard.path()).unwrap();

        let cli = Cli::parse_from(["mytool", "--cmake-arg", "-DVAR1=VAL1", "build"]);
        assert_eq!(cli.cmake_args, vec!["-DVAR1=VAL1"]);
        let config = Config::from_cli(cli).unwrap();
        assert_eq!(config.cmake_args, vec!["-DVAR1=VAL1"]);

        std::env::set_current_dir(current_dir_before_test).unwrap();
    }

    #[test]
    fn test_multiple_cmake_args() {
        let _dummy_dir_guard = setup_dummy_rocm_cmake_dir();
        let current_dir_before_test = std::env::current_dir().unwrap();
        std::env::set_current_dir(_dummy_dir_guard.path()).unwrap();

        let cli = Cli::parse_from([
            "mytool",
            "--cmake-arg",
            "-DVAR1=VAL1",
            "--cmake-arg",
            "-DVAR2=VAL2",
            "build",
        ]);
        assert_eq!(cli.cmake_args, vec!["-DVAR1=VAL1", "-DVAR2=VAL2"]);
        let config = Config::from_cli(cli).unwrap();
        assert_eq!(config.cmake_args, vec!["-DVAR1=VAL1", "-DVAR2=VAL2"]);

        std::env::set_current_dir(current_dir_before_test).unwrap();
    }

    // Test that default build_dir is correctly handled by Config::from_cli
    // and that the current directory for the test does not affect it if it's relative.
    #[test]
    fn test_default_build_dir_resolution() {
        let original_current_dir = std::env::current_dir().unwrap();

        let temp_project_root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp_project_root.path().join("rocm-cmake")).unwrap();

        // Change current dir to simulate running from project root
        std::env::set_current_dir(temp_project_root.path()).unwrap();

        let cli = Cli::parse_from(["mytool", "build"]); // Uses default "./build"
        let config = Config::from_cli(cli).unwrap();

        let expected_build_dir = temp_project_root.path().join("build");
        assert_eq!(config.build_dir, expected_build_dir);

        // Restore original current directory
        std::env::set_current_dir(original_current_dir).unwrap();
    }

    // Test that an absolute build_dir is correctly handled.
    #[test]
    fn test_absolute_build_dir_resolution() {
        let original_current_dir = std::env::current_dir().unwrap();
        let temp_project_root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp_project_root.path().join("rocm-cmake")).unwrap();
        std::env::set_current_dir(temp_project_root.path()).unwrap();

        let abs_build_dir_temp = tempfile::tempdir().unwrap();
        let abs_build_path = abs_build_dir_temp.path().to_path_buf();

        let cli = Cli::parse_from(["mytool", "--build-dir", abs_build_path.to_str().unwrap(), "build"]);
        let config = Config::from_cli(cli).unwrap();

        assert_eq!(config.build_dir, abs_build_path);

        std::env::set_current_dir(original_current_dir).unwrap();
    }

     // Test that a relative build_dir is correctly handled and made absolute.
    #[test]
    fn test_relative_build_dir_resolution() {
        let original_current_dir = std::env::current_dir().unwrap();
        let temp_project_root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp_project_root.path().join("rocm-cmake")).unwrap();
        std::env::set_current_dir(temp_project_root.path()).unwrap();

        let cli = Cli::parse_from(["mytool", "--build-dir", "my_custom_build", "build"]);
        let config = Config::from_cli(cli).unwrap();

        let expected_build_dir = temp_project_root.path().join("my_custom_build");
        assert_eq!(config.build_dir, expected_build_dir);

        std::env::set_current_dir(original_current_dir).unwrap();
    }
}
