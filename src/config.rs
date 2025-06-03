use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::fs;
use std::env;

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

    #[arg(short, long, value_name = "COUNT", help = "Number of parallel jobs for CMake build. (e.g., 8)")]
    pub jobs: Option<usize>,

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
    pub jobs: Option<usize>,
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

        let mut rocm_cmake_path_resolved: Option<PathBuf> = None;
        let mut source_dir_resolved: Option<PathBuf> = None;

        const ROCM_CMAKE_ENV_VAR: &str = "ROCM_CMAKE_PATH";
        debug!("Checking for {} environment variable...", ROCM_CMAKE_ENV_VAR);
        if let Ok(env_path_str) = env::var(ROCM_CMAKE_ENV_VAR) {
            if !env_path_str.is_empty() {
                let env_rcp = PathBuf::from(env_path_str);
                if env_rcp.is_dir() && env_rcp.file_name().map_or(false, |name| name == "rocm-cmake") {
                    info!("Using rocm-cmake from environment variable {}: {}", ROCM_CMAKE_ENV_VAR, env_rcp.display());
                    if let Some(parent_dir) = env_rcp.parent() {
                        rocm_cmake_path_resolved = Some(env_rcp);
                        source_dir_resolved = Some(parent_dir.to_path_buf());
                    } else {
                        warn!("Could not determine parent directory of ROCM_CMAKE_PATH ('{}'). This is unusual. Treating the path itself as the source directory.", env_rcp.display());
                        rocm_cmake_path_resolved = Some(env_rcp.clone());
                        source_dir_resolved = Some(env_rcp);
                    }
                } else {
                    warn!("{} environment variable is set to '{}', but this is not a valid directory named 'rocm-cmake'. Falling back to directory search.", ROCM_CMAKE_ENV_VAR, env_rcp.display());
                }
            } else {
                debug!("{} environment variable is set but empty. Falling back to directory search.", ROCM_CMAKE_ENV_VAR);
            }
        } else {
            debug!("{} environment variable not set. Using directory search.", ROCM_CMAKE_ENV_VAR);
        }

        if rocm_cmake_path_resolved.is_none() {
            debug!("Searching for 'rocm-cmake' directory starting from current directory and parents...");
            let mut current_search_dir = current_dir.clone();
            for i in 0..6 { // Check current directory and up to 5 parent levels
                let potential_rcp = current_search_dir.join("rocm-cmake");
                debug!("Checking for 'rocm-cmake' in: {}", current_search_dir.display());
                if potential_rcp.is_dir() {
                    info!("Found 'rocm-cmake' directory at: {}", potential_rcp.display());
                    rocm_cmake_path_resolved = Some(potential_rcp);
                    source_dir_resolved = Some(current_search_dir);
                    break;
                }
                if i == 5 { // Max depth reached
                    debug!("Reached max search depth for 'rocm-cmake'.");
                    break;
                }
                if let Some(parent) = current_search_dir.parent() {
                    current_search_dir = parent.to_path_buf();
                } else {
                    debug!("No more parent directories to search for 'rocm-cmake'.");
                    break;
                }
            }
        }

        let final_rocm_cmake_path = rocm_cmake_path_resolved.ok_or_else(||
            anyhow!("'rocm-cmake' directory not found. Set the {} environment variable or run from a directory containing 'rocm-cmake', or one of its parent directories.", ROCM_CMAKE_ENV_VAR)
        )?;
        let final_source_dir = source_dir_resolved.expect("source_dir should be set if rocm_cmake_path was resolved");

        debug!("Determined source directory: {}", final_source_dir.display());
        debug!("Using rocm-cmake path: {}", final_rocm_cmake_path.display());

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
            rocm_cmake_path: final_rocm_cmake_path,
            source_dir: final_source_dir,
            build_type: cli.build_type.clone(),
            cmake_args: cli.cmake_args.clone(),
            jobs: cli.jobs,
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
    use std::path::{Path, PathBuf}; // Added Path for run_config_test
    use std::fs; // Added for run_config_test and env var tests
    use std::env; // Added for run_config_test and env var tests
    use tempfile::tempdir; // Added for run_config_test, though setup_dummy_rocm_cmake_dir also uses it
    use std::sync::Mutex; // Added for ENV_TEST_LOCK

    // Mutex to serialize tests that modify environment variables or current directory
    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

    // Helper function to run a test in a controlled environment
    // Ensures that current_dir and specified env vars are reset after the test.
    fn run_env_test<F>(test_fn: F)
    where
        F: FnOnce(&Path), // Pass temp_path (root of test env) to the test function
    {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let temp_dir = tempdir().unwrap();
        let original_current_dir = std::env::current_dir().unwrap();

        // Set current directory to the root of the temp dir for the test
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Create the build directory within the temp_dir ahead of time,
        // because Config::from_cli tries to create it.
        // It uses "./build" by default relative to current_dir if not specified in CLI.
        fs::create_dir_all(temp_dir.path().join("build")).unwrap();

        test_fn(temp_dir.path()); // Execute the actual test logic

        // Teardown: Restore current_dir
        std::env::set_current_dir(original_current_dir).unwrap();
        // Note: Specific env vars set by tests should be unset within the test_fn itself.
    }

    // This existing helper is fine for tests NOT manipulating global state like env vars
    // or needing very specific current_dir setups beyond what it provides.
    // For new tests, especially env var tests, run_env_test is preferred.
    fn setup_dummy_rocm_cmake_dir() -> tempfile::TempDir {
        let temp_dir = tempdir().unwrap();
        // Config::from_cli will search for rocm-cmake in current_dir or parents.
        // This helper creates it directly in the temp_dir.
        // Tests using this then set current_dir to this temp_dir.
        fs::create_dir_all(temp_dir.path().join("rocm-cmake")).unwrap();
        fs::create_dir_all(temp_dir.path().join("build")).unwrap(); // Also pre-create build dir
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
        let temp_project_root = setup_dummy_rocm_cmake_dir(); // Uses helper that creates rocm-cmake inside
        std::env::set_current_dir(temp_project_root.path()).unwrap();

        let cli = Cli::parse_from(["mytool", "--build-dir", "my_custom_build", "build"]);
        let config = Config::from_cli(cli).unwrap();

        let expected_build_dir = temp_project_root.path().join("my_custom_build");
        assert_eq!(config.build_dir, expected_build_dir);

        std::env::set_current_dir(original_current_dir).unwrap();
    }

    // --- Tests for Parallel Job Count ---
    #[test]
    fn test_default_jobs_option() {
        run_env_test(|test_env_path| {
            // Create rocm-cmake inside the test_env_path for Config::from_cli search
            fs::create_dir_all(test_env_path.join("rocm-cmake")).unwrap();

            let cli = Cli::parse_from(["mytool", "build"]);
            assert_eq!(cli.jobs, None);
            let config = Config::from_cli(cli).expect("Config from CLI failed");
            assert_eq!(config.jobs, None);
        });
    }

    #[test]
    fn test_custom_jobs_option() {
        run_env_test(|test_env_path| {
            fs::create_dir_all(test_env_path.join("rocm-cmake")).unwrap();

            let cli = Cli::parse_from(["mytool", "--jobs", "4", "build"]);
            assert_eq!(cli.jobs, Some(4));
            let config = Config::from_cli(cli).expect("Config from CLI failed");
            assert_eq!(config.jobs, Some(4));
        });
    }

    // --- Tests for rocm-cmake Path Determination ---
    #[test]
    fn test_rocm_cmake_path_from_env_valid() {
        run_env_test(|test_env_path| {
            let mock_rocm_cmake_dir = test_env_path.join("rocm-cmake");
            fs::create_dir_all(&mock_rocm_cmake_dir).unwrap();

            env::set_var("ROCM_CMAKE_PATH", mock_rocm_cmake_dir.to_str().unwrap());

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, mock_rocm_cmake_dir);
            assert_eq!(config.source_dir, test_env_path); // Parent of rocm-cmake

            env::remove_var("ROCM_CMAKE_PATH");
        });
    }

    #[test]
    fn test_rocm_cmake_path_from_env_invalid_name_falls_back() {
        run_env_test(|test_env_path| {
            let invalid_env_path = test_env_path.join("not_rocm_cmake_dir"); // Name is not "rocm-cmake"
            fs::create_dir_all(&invalid_env_path).unwrap();
            env::set_var("ROCM_CMAKE_PATH", invalid_env_path.to_str().unwrap());

            // Create a discoverable rocm-cmake for fallback in current dir (test_env_path)
            let discoverable_rocm_cmake = test_env_path.join("rocm-cmake");
            fs::create_dir_all(&discoverable_rocm_cmake).unwrap();

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            // Should ignore env var due to invalid name and use the discovered one
            assert_eq!(config.rocm_cmake_path, discoverable_rocm_cmake);
            assert_eq!(config.source_dir, test_env_path);

            env::remove_var("ROCM_CMAKE_PATH");
        });
    }

    #[test]
    fn test_rocm_cmake_path_from_env_valid_but_not_dir_falls_back() {
        run_env_test(|test_env_path| {
            let invalid_env_path_file = test_env_path.join("rocm-cmake"); // Name is correct
            fs::File::create(&invalid_env_path_file).unwrap(); // Create as a file, not a dir
            env::set_var("ROCM_CMAKE_PATH", invalid_env_path_file.to_str().unwrap());

            // Create a discoverable rocm-cmake for fallback
            let discoverable_parent = test_env_path.join("discoverable_parent");
            fs::create_dir_all(&discoverable_parent).unwrap();
            let discoverable_rocm_cmake = discoverable_parent.join("rocm-cmake");
            fs::create_dir_all(&discoverable_rocm_cmake).unwrap();

            // We need to run from a dir that would allow fallback search to find the discoverable one.
            // Current dir is test_env_path. Fallback search will check test_env_path/rocm-cmake (which is a file)
            // then parents. Let's make the discoverable one in a child dir and run from there.
            // OR, more simply, change current dir to discoverable_parent for this test.
            let original_current_dir = std::env::current_dir().unwrap();
            std::env::set_current_dir(&discoverable_parent).unwrap();


            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, discoverable_rocm_cmake);
            assert_eq!(config.source_dir, discoverable_parent);

            std::env::set_current_dir(original_current_dir).unwrap();
            env::remove_var("ROCM_CMAKE_PATH");
        });
    }


    #[test]
    fn test_rocm_cmake_path_search_current_dir() {
        run_env_test(|test_env_path| {
            env::remove_var("ROCM_CMAKE_PATH");
            let discoverable_rocm_cmake = test_env_path.join("rocm-cmake");
            fs::create_dir_all(&discoverable_rocm_cmake).unwrap();

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, discoverable_rocm_cmake);
            assert_eq!(config.source_dir, test_env_path);
        });
    }

    #[test]
    fn test_rocm_cmake_path_search_parent_dir() {
        run_env_test(|temp_path_root| { // This is the actual root for FS operations.
            env::remove_var("ROCM_CMAKE_PATH");

            // rocm-cmake is in temp_path_root
            let parent_rocm_cmake = temp_path_root.join("rocm-cmake");
            fs::create_dir_all(&parent_rocm_cmake).unwrap();

            // Current directory for the test will be a subdirectory
            let current_subdir = temp_path_root.join("current_subdir");
            fs::create_dir_all(&current_subdir).unwrap();

            // Temporarily change current_dir for this specific test logic
            let original_current_dir = std::env::current_dir().unwrap(); // Should be temp_path_root
            std::env::set_current_dir(&current_subdir).unwrap();

            // Pre-create build dir relative to new current_subdir
            fs::create_dir_all(current_subdir.join("build")).unwrap();


            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, parent_rocm_cmake);
            assert_eq!(config.source_dir, temp_path_root);

            // Restore current_dir to what run_env_test expects for its cleanup
            std::env::set_current_dir(original_current_dir).unwrap();
        });
    }

    #[test]
    fn test_rocm_cmake_path_not_found_returns_err() {
        run_env_test(|_test_env_path| {
            env::remove_var("ROCM_CMAKE_PATH");
            // No "rocm-cmake" directory created anywhere discoverable by default search.
            // Default search starts from current_dir (which is _test_env_path) and goes up.
            // Since _test_env_path is empty (of rocm-cmake), and has no relevant parents outside itself,
            // this should fail.

            let cli = Cli::parse_from(["mytool", "build"]);
            let config_result = Config::from_cli(cli);

            assert!(config_result.is_err());
            if let Err(e) = config_result {
                 assert!(e.to_string().contains("'rocm-cmake' directory not found"));
            }
        });
    }
}
