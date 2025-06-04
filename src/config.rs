use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::fs;
use std::env;
use serde::Deserialize; // Added for config file parsing

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

    #[arg(long, value_name = "DEPTH", help = "Maximum depth to search for CMake projects relative to source_dir (0 for source_dir itself, 1 for immediate subdirs, etc.).")]
    pub project_search_depth: Option<usize>,

    #[arg(long = "source-dir", short = 'S', value_name = "PATH", help = "Specify a source directory to search for projects. Can be used multiple times. If not provided, defaults to the parent directory of rocm-cmake.")]
    pub source_dirs: Vec<PathBuf>,

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
    pub source_dirs: Vec<PathBuf>, // Changed from source_dir: PathBuf
    pub build_type: String,
    pub cmake_args: Vec<String>,
    pub jobs: Option<usize>,
    pub project_search_depth: usize,
}

const DEFAULT_PROJECT_SEARCH_DEPTH: usize = 1;

impl Config {
    pub fn from_cli(cli: Cli) -> Result<Self> {
        let current_dir = std::env::current_dir().context("Failed to get current directory")?;

        // --- Load Configuration File ---
        const CONFIG_FILE_NAME: &str = ".rocm_build.toml";
        let mut loaded_config_values: Option<AppConfigFile> = None;

        let mut potential_config_paths: Vec<PathBuf> = Vec::new();
        // 1. Current directory
        potential_config_paths.push(current_dir.join(CONFIG_FILE_NAME));
        // 2. User's home directory
        if let Some(home_dir_path) = home::home_dir() {
            potential_config_paths.push(home_dir_path.join(CONFIG_FILE_NAME));
        } else {
            debug!("Could not determine user's home directory. Skipping search for config file there.");
        }

        debug!("Searching for configuration file '{}' in potential locations: {:?}", CONFIG_FILE_NAME, potential_config_paths);
        for config_path_candidate in potential_config_paths {
            if config_path_candidate.exists() { // Check before calling load_config_file
                match load_config_file(&config_path_candidate) {
                    Ok(Some(parsed_config)) => {
                        info!("Loaded configuration from: {}", config_path_candidate.display());
                        loaded_config_values = Some(parsed_config);
                        break;
                    }
                    Ok(None) => {
                        // load_config_file handles its own logging for parse errors or empty file
                    }
                    Err(e) => {
                        warn!("Unexpected error trying to load config file {}: {}. Continuing without it.", config_path_candidate.display(), e);
                    }
                }
            }
        }
        if loaded_config_values.is_none() {
            info!("No configuration file '{}' found or loaded from search paths.", CONFIG_FILE_NAME);
        }

        // --- End Load Configuration File ---
        
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

        // --- Resolve rocm_cmake_path and its parent (initial_source_dir_for_rocm_cmake) ---
        // This order of precedence: Env Var -> Config File -> Search
        let mut determined_rocm_cmake_path: Option<PathBuf> = None;
        let mut initial_source_dir_for_rocm_cmake: Option<PathBuf> = None;
        let mut how_rocm_cmake_path_determined = "search"; // Default assumption

        // 1. Environment Variable
        const ROCM_CMAKE_ENV_VAR: &str = "ROCM_CMAKE_PATH";
        debug!("Checking for {} environment variable...", ROCM_CMAKE_ENV_VAR);
        if let Ok(env_path_str) = env::var(ROCM_CMAKE_ENV_VAR) {
            if !env_path_str.is_empty() {
                let env_rcp = PathBuf::from(env_path_str);
                if env_rcp.is_dir() && env_rcp.file_name().map_or(false, |name| name == "rocm-cmake") {
                    determined_rocm_cmake_path = Some(env_rcp.clone());
                    initial_source_dir_for_rocm_cmake = env_rcp.parent().map(|p| p.to_path_buf());
                    how_rocm_cmake_path_determined = "environment variable";
                    if initial_source_dir_for_rocm_cmake.is_none() {
                        warn!("Parent of ROCM_CMAKE_PATH ('{}') from env var could not be determined. Using path itself as source directory.", env_rcp.display());
                        initial_source_dir_for_rocm_cmake = Some(env_rcp.clone());
                    }
                } else {
                    warn!("{} environment variable is set to '{}', but this is not a valid 'rocm-cmake' directory. Ignoring.", ROCM_CMAKE_ENV_VAR, env_rcp.display());
                }
            } else {
                debug!("{} environment variable is set but empty. Ignoring.", ROCM_CMAKE_ENV_VAR);
            }
        } else {
            debug!("{} environment variable not set.", ROCM_CMAKE_ENV_VAR);
        }

        // 2. Configuration File (only if not found by env var)
        if determined_rocm_cmake_path.is_none() {
            debug!("Checking for 'rocm_cmake_path' in loaded configuration file...");
            if let Some(app_config) = &loaded_config_values {
                if let Some(config_rcp_relative) = &app_config.rocm_cmake_path {
                    let config_rcp = if config_rcp_relative.is_absolute() {
                        config_rcp_relative.clone()
                    } else {
                        // Assuming relative paths in config are relative to current working directory.
                        // This could be improved if the config file's actual path is stored and used as base.
                        current_dir.join(config_rcp_relative)
                    };

                    info!("Found 'rocm_cmake_path: {}' in config file. Validating...", config_rcp.display());
                    if config_rcp.is_dir() && config_rcp.file_name().map_or(false, |name| name == "rocm-cmake") {
                        determined_rocm_cmake_path = Some(config_rcp.clone());
                        initial_source_dir_for_rocm_cmake = config_rcp.parent().map(|p| p.to_path_buf());
                        how_rocm_cmake_path_determined = "configuration file";
                        if initial_source_dir_for_rocm_cmake.is_none() {
                             warn!("Parent of 'rocm_cmake_path' ('{}') from config file could not be determined. Using path itself as source directory.", config_rcp.display());
                             initial_source_dir_for_rocm_cmake = Some(config_rcp.clone());
                        }
                    } else {
                        warn!("Path '{}' for 'rocm_cmake_path' from configuration file is not a valid 'rocm-cmake' directory. Ignoring.", config_rcp.display());
                    }
                } else {
                    debug!("'rocm_cmake_path' not specified in loaded configuration file.");
                }
            } else {
                debug!("No configuration file was loaded. Skipping check for 'rocm_cmake_path' in it.");
            }
        }

        // 3. Directory Search (lowest precedence)
        if determined_rocm_cmake_path.is_none() {
            debug!("Searching for 'rocm-cmake' directory starting from current directory and parents...");
            how_rocm_cmake_path_determined = "directory search";
            let mut current_search_dir = current_dir.clone();
            for i in 0..6 {
                let potential_rcp = current_search_dir.join("rocm-cmake");
                debug!("Checking for 'rocm-cmake' in: {}", current_search_dir.display());
                if potential_rcp.is_dir() {
                    determined_rocm_cmake_path = Some(potential_rcp);
                    initial_source_dir_for_rocm_cmake = Some(current_search_dir);
                    break;
                }
                if i == 5 { debug!("Reached max search depth for 'rocm-cmake'."); break; }
                if let Some(parent) = current_search_dir.parent() {
                    current_search_dir = parent.to_path_buf();
                } else {
                    debug!("No more parent directories to search for 'rocm-cmake'.");
                    break;
                }
            }
        }

        let final_rocm_cmake_path = determined_rocm_cmake_path.ok_or_else(||
            anyhow!("'rocm-cmake' directory not found. Searched via environment variable ('{}'), config file ('{}'), and directory scan.", ROCM_CMAKE_ENV_VAR, CONFIG_FILE_NAME)
        )?;
        let rocm_cmake_parent_dir = initial_source_dir_for_rocm_cmake.expect("rocm_cmake_parent_dir should be set if final_rocm_cmake_path was resolved");

        info!("Resolved 'rocm-cmake' path to: {} (determined by {}).", final_rocm_cmake_path.display(), how_rocm_cmake_path_determined);

        // Determine final source_dirs for project searching
        let resolved_source_dirs: Vec<PathBuf>;
        if !cli.source_dirs.is_empty() {
            info!("Using user-specified source directories via --source-dir CLI argument.");
            resolved_source_dirs = cli.source_dirs.iter().map(|p_rel| {
                let p_abs = if p_rel.is_absolute() { p_rel.clone() } else { current_dir.join(p_rel) };
                debug!("Effective source directory (from CLI) for project search: {}", p_abs.display());
                if !p_abs.exists() {
                     warn!("User-specified source directory (from CLI) {} does not exist.", p_abs.display());
                }
                p_abs
            }).collect();
        } else if let Some(cfg_source_dirs) = loaded_config_values.as_ref().and_then(|cfg| cfg.default_source_dirs.as_ref()) {
            if !cfg_source_dirs.is_empty() {
                info!("Using 'default_source_dirs' from configuration file.");
                resolved_source_dirs = cfg_source_dirs.iter().map(|p_cfg| {
                    let p_abs = if p_cfg.is_absolute() { p_cfg.clone() } else { current_dir.join(p_cfg) };
                    debug!("Effective source directory (from config) for project search: {}", p_abs.display());
                    if !p_abs.exists() {
                        warn!("Source directory '{}' from config file does not exist.", p_abs.display());
                    }
                    p_abs
                }).collect();
            } else {
                info!("'default_source_dirs' in config file is present but empty. Defaulting source directory to parent of rocm-cmake: {}", rocm_cmake_parent_dir.display());
                resolved_source_dirs = vec![rocm_cmake_parent_dir.clone()];
            }
        } else {
            info!("No --source-dir CLI argument and no 'default_source_dirs' in config. Defaulting source directory to parent of rocm-cmake: {}", rocm_cmake_parent_dir.display());
            resolved_source_dirs = vec![rocm_cmake_parent_dir.clone()];
        }

        // Resolve project_search_depth: CLI > Config File > Default
        let resolved_project_search_depth = cli.project_search_depth.or_else(|| {
            loaded_config_values.as_ref().and_then(|cfg| {
                cfg.default_project_search_depth.map(|depth| {
                    info!("Using 'default_project_search_depth: {}' from configuration file.", depth);
                    depth
                })
            })
        }).unwrap_or_else(|| {
            info!("No --project-search-depth CLI option or config value. Using default project search depth ({}).", DEFAULT_PROJECT_SEARCH_DEPTH);
            DEFAULT_PROJECT_SEARCH_DEPTH
        });
        debug!("Effective project search depth set to: {}", resolved_project_search_depth);

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
            source_dirs: resolved_source_dirs, // Use the new resolved_source_dirs
            build_type: cli.build_type.clone(),
            cmake_args: cli.cmake_args.clone(),
            jobs: cli.jobs,
            project_search_depth: resolved_project_search_depth,
        })
    }

    pub fn get_package_build_dir(&self, package_name: &str) -> PathBuf {
        self.build_dir.join(package_name)
    }

    pub fn get_package_install_dir(&self, package_name: &str) -> Option<PathBuf> {
        self.install_dir.as_ref().map(|idir| idir.join(package_name))
    }
}

// --- Config File Handling ---

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppConfigFile {
    pub rocm_cmake_path: Option<PathBuf>,
    pub default_project_search_depth: Option<usize>,
    pub default_source_dirs: Option<Vec<PathBuf>>,
}

pub(crate) fn load_config_file(config_path: &Path) -> Result<Option<AppConfigFile>> {
    if !config_path.exists() {
        debug!("Configuration file not found at: {}", config_path.display());
        return Ok(None);
    }
    if !config_path.is_file() {
        warn!("Configuration path exists but is not a file: {}. Ignoring.", config_path.display());
        return Ok(None);
    }

    debug!("Attempting to load configuration file from: {}", config_path.display());
    match fs::read_to_string(config_path) {
        Ok(content) => {
            if content.trim().is_empty() {
                debug!("Configuration file '{}' is empty. Proceeding as if no config values set.", config_path.display());
                return Ok(Some(AppConfigFile::default())); // Treat empty file as empty config
            }
            match toml::from_str::<AppConfigFile>(&content) {
                Ok(parsed_config) => {
                    info!("Successfully loaded and parsed configuration from: {}", config_path.display());
                    Ok(Some(parsed_config))
                }
                Err(e) => {
                    warn!("Failed to parse TOML configuration from '{}': {}. Proceeding as if config file had no usable values.", config_path.display(), e);
                    Ok(None) // Gracefully ignore parse errors, treat as if no config values found
                }
            }
        }
        Err(e) => {
            warn!("Failed to read configuration file '{}': {}. Proceeding as if config file had no usable values.", config_path.display(), e);
            Ok(None) // Gracefully ignore read errors
        }
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
    // Note: These tests implicitly test the default source_dirs logic (parent of rocm-cmake)
    // New tests will be needed for explicit --source-dir usage.
    #[test]
    fn test_rocm_cmake_path_from_env_valid() {
        run_env_test(|test_env_path| {
            let mock_rocm_cmake_dir = test_env_path.join("rocm-cmake");
            fs::create_dir_all(&mock_rocm_cmake_dir).unwrap();

            env::set_var("ROCM_CMAKE_PATH", mock_rocm_cmake_dir.to_str().unwrap());

            let cli = Cli::parse_from(["mytool", "build"]); // No --source-dir, so should default
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, mock_rocm_cmake_dir);
            assert_eq!(config.source_dirs, vec![test_env_path.to_path_buf()]); // Default source dir

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

            assert_eq!(config.rocm_cmake_path, discoverable_rocm_cmake);
            assert_eq!(config.source_dirs, vec![test_env_path.to_path_buf()]); // Default source dir

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
            assert_eq!(config.source_dirs, vec![discoverable_parent.to_path_buf()]);

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
            assert_eq!(config.source_dirs, vec![test_env_path.to_path_buf()]);
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
            assert_eq!(config.source_dirs, vec![temp_path_root.to_path_buf()]);

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

    // --- Tests for Project Search Depth ---
    #[test]
    fn test_default_project_search_depth() {
        run_env_test(|test_env_path| {
            // Ensure rocm-cmake dir exists for Config::from_cli to pass basic checks
            // run_env_test sets current_dir to test_env_path.
            fs::create_dir_all(test_env_path.join("rocm-cmake")).unwrap();

            let cli = Cli::parse_from(["mytool", "build"]);
            assert_eq!(cli.project_search_depth, None); // Default is None in Cli
            let config = Config::from_cli(cli).expect("Config from CLI failed");
            // DEFAULT_PROJECT_SEARCH_DEPTH is 1, defined in the outer scope.
            assert_eq!(config.project_search_depth, super::DEFAULT_PROJECT_SEARCH_DEPTH);
        });
    }

    #[test]
    fn test_custom_project_search_depth() {
        run_env_test(|test_env_path| {
            fs::create_dir_all(test_env_path.join("rocm-cmake")).unwrap();

            let cli = Cli::parse_from(["mytool", "--project-search-depth", "3", "build"]);
            assert_eq!(cli.project_search_depth, Some(3));
            let config = Config::from_cli(cli).expect("Config from CLI failed");
            assert_eq!(config.project_search_depth, 3);
        });
    }

    // --- Tests for Multiple Source Directories ---
    #[test]
    fn test_config_source_dirs_default() {
        run_env_test(|temp_path| {
            let rocm_cmake_dir = temp_path.join("rocm-cmake");
            fs::create_dir_all(&rocm_cmake_dir).unwrap();

            let cli = Cli::parse_from(["mytool", "build"]); // No --source-dir args
            let config = Config::from_cli(cli).expect("Config from CLI failed for default source_dirs");

            assert_eq!(config.source_dirs.len(), 1, "Default source_dirs should have one entry");
            assert_eq!(config.source_dirs[0], temp_path.to_path_buf(), "Default source_dir should be parent of rocm-cmake");
            assert_eq!(config.rocm_cmake_path, rocm_cmake_dir, "rocm_cmake_path should be correctly identified");
        });
    }

    #[test]
    fn test_config_source_dirs_one_absolute() {
        run_env_test(|temp_path| {
            let rocm_cmake_dir = temp_path.join("rocm-cmake");
            fs::create_dir_all(&rocm_cmake_dir).unwrap();

            let user_src_dir_holder = tempfile::tempdir().unwrap(); // Create a separate temp dir for user source
            let user_src_path = user_src_dir_holder.path().to_path_buf();
            fs::create_dir_all(&user_src_path).unwrap(); // Ensure it actually exists

            let cli = Cli::parse_from([
                "mytool",
                "--source-dir",
                user_src_path.to_str().unwrap(),
                "build",
            ]);
            let config = Config::from_cli(cli).expect("Config from CLI failed for one absolute source_dir");

            assert_eq!(config.source_dirs.len(), 1, "source_dirs should have one entry");
            assert_eq!(config.source_dirs[0], user_src_path, "source_dirs should contain the user-specified absolute path");
        });
    }

    #[test]
    fn test_config_source_dirs_one_relative() {
        run_env_test(|temp_path| { // CWD for the test is temp_path
            let rocm_cmake_dir = temp_path.join("rocm-cmake");
            fs::create_dir_all(&rocm_cmake_dir).unwrap();

            let relative_src_dirname = "my_projects_subdir";
            let user_src_dir_abs = temp_path.join(relative_src_dirname);
            fs::create_dir_all(&user_src_dir_abs).unwrap();

            let cli = Cli::parse_from(["mytool", "--source-dir", relative_src_dirname, "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed for one relative source_dir");

            assert_eq!(config.source_dirs.len(), 1, "source_dirs should have one entry");
            assert_eq!(config.source_dirs[0], user_src_dir_abs, "Relative source_dir should be resolved to absolute path");
        });
    }

    #[test]
    fn test_config_source_dirs_multiple() {
        run_env_test(|temp_path| {
            let rocm_cmake_dir = temp_path.join("rocm-cmake");
            fs::create_dir_all(&rocm_cmake_dir).unwrap();

            let user_src_dir1_abs_holder = tempfile::tempdir().unwrap();
            let user_src_dir1_abs = user_src_dir1_abs_holder.path().to_path_buf();
            fs::create_dir_all(&user_src_dir1_abs).unwrap();

            let relative_src_dirname = "my_other_projects";
            let user_src_dir2_abs = temp_path.join(relative_src_dirname); // Relative to temp_path (CWD)
            fs::create_dir_all(&user_src_dir2_abs).unwrap();

            let cli = Cli::parse_from([
                "mytool",
                "--source-dir",
                user_src_dir1_abs.to_str().unwrap(), // Absolute path
                "--source-dir",
                relative_src_dirname, // Relative path
                "build",
            ]);
            let config = Config::from_cli(cli).expect("Config from CLI failed for multiple source_dirs");

            assert_eq!(config.source_dirs.len(), 2, "source_dirs should have two entries");

            let mut sorted_dirs = config.source_dirs.clone();
            sorted_dirs.sort();

            let mut expected_dirs = vec![user_src_dir1_abs, user_src_dir2_abs];
            expected_dirs.sort();

            assert_eq!(sorted_dirs, expected_dirs, "Configured source_dirs do not match expected");
        });
    }

    #[test]
    fn test_config_source_dirs_user_specified_non_existent() {
        run_env_test(|temp_path| {
            let rocm_cmake_dir = temp_path.join("rocm-cmake");
            fs::create_dir_all(&rocm_cmake_dir).unwrap();

            let non_existent_dir = temp_path.join("i_do_not_exist");
            // Do NOT create non_existent_dir

            let cli = Cli::parse_from([
                "mytool",
                "--source-dir",
                non_existent_dir.to_str().unwrap(),
                "build",
            ]);
            // Config::from_cli should still succeed, but log a warning (tested by observation)
            let config = Config::from_cli(cli).expect("Config::from_cli failed even with non-existent source dir");

            assert_eq!(config.source_dirs.len(), 1);
            assert_eq!(config.source_dirs[0], non_existent_dir);
            // The run_build/run_clean logic will later skip this directory with a warning.
        });
    }

    // --- Tests for rocm_cmake_path determination with Config File ---

    // Helper to create a config file for tests
    fn create_temp_config_file(dir: &Path, content: &str) -> PathBuf {
        let config_path = dir.join(super::CONFIG_FILE_NAME); // Use const from outer scope
        fs::write(&config_path, content).unwrap();
        config_path
    }

    #[test]
    fn test_rocm_path_env_over_config_over_search() {
        run_env_test(|temp_path| {
            // 1. Env var path (highest priority)
            let env_parent = temp_path.join("env_ver_parent");
            fs::create_dir_all(&env_parent).unwrap();
            let env_rocm_cmake = env_parent.join("rocm-cmake");
            fs::create_dir_all(&env_rocm_cmake).unwrap();
            env::set_var("ROCM_CMAKE_PATH", env_rocm_cmake.to_str().unwrap());

            // 2. Config file path
            let config_parent = temp_path.join("config_ver_parent");
            fs::create_dir_all(&config_parent).unwrap();
            let config_rocm_cmake = config_parent.join("rocm-cmake");
            fs::create_dir_all(&config_rocm_cmake).unwrap();
            create_temp_config_file(temp_path, &format!("rocm_cmake_path = \"{}\"", config_rocm_cmake.display()));

            // 3. Search path (lowest priority)
            let search_parent = temp_path.join("search_ver_parent");
            fs::create_dir_all(&search_parent).unwrap();
            let search_rocm_cmake = search_parent.join("rocm-cmake");
            fs::create_dir_all(&search_rocm_cmake).unwrap();
            // To make this discoverable by search, we'd need to make `search_parent` the CWD,
            // or make `temp_path` itself the parent of a `rocm-cmake` dir.
            // For this test, `run_env_test` sets CWD to `temp_path`. So, create search_rocm_cmake directly in temp_path.
            let direct_search_rocm_cmake = temp_path.join("rocm-cmake");
            fs::create_dir_all(&direct_search_rocm_cmake).unwrap();


            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, env_rocm_cmake);
            assert_eq!(config.source_dirs[0], env_parent);
            env::remove_var("ROCM_CMAKE_PATH");
        });
    }

    #[test]
    fn test_rocm_path_config_over_search() {
        run_env_test(|temp_path| {
            env::remove_var("ROCM_CMAKE_PATH");

            // 1. Config file path
            let config_parent = temp_path.join("config_ver_parent");
            fs::create_dir_all(&config_parent).unwrap();
            let config_rocm_cmake = config_parent.join("rocm-cmake");
            fs::create_dir_all(&config_rocm_cmake).unwrap();
            create_temp_config_file(temp_path, &format!("rocm_cmake_path = \"{}\"", config_rocm_cmake.display()));

            // 2. Search path (directly in temp_path, which is CWD)
            let search_rocm_cmake = temp_path.join("rocm-cmake");
            fs::create_dir_all(&search_rocm_cmake).unwrap();

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, config_rocm_cmake);
            assert_eq!(config.source_dirs[0], config_parent);
        });
    }

    #[test]
    fn test_rocm_path_search_fallback() {
        run_env_test(|temp_path| {
            env::remove_var("ROCM_CMAKE_PATH");
            create_temp_config_file(temp_path, "# Empty config or no rocm_cmake_path key");

            let search_rocm_cmake = temp_path.join("rocm-cmake");
            fs::create_dir_all(&search_rocm_cmake).unwrap();

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, search_rocm_cmake);
            assert_eq!(config.source_dirs[0], temp_path.to_path_buf());
        });
    }

    #[test]
    fn test_rocm_path_config_invalid_path_falls_back_to_search() {
        run_env_test(|temp_path| {
            env::remove_var("ROCM_CMAKE_PATH");
            let non_existent_path = temp_path.join("non_existent_rocm_cmake");
            // Config file is created in temp_path (CWD for test)
            create_temp_config_file(temp_path, &format!("rocm_cmake_path = \"{}\"", non_existent_path.display()));

            let search_rocm_cmake = temp_path.join("rocm-cmake");
            fs::create_dir_all(&search_rocm_cmake).unwrap();

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, search_rocm_cmake);
            assert_eq!(config.source_dirs[0], temp_path.to_path_buf());
        });
    }

    #[test]
    fn test_rocm_path_config_relative_path() {
        run_env_test(|temp_path| { // CWD is temp_path
            env::remove_var("ROCM_CMAKE_PATH");

            let relative_rcm_path_str = "my/relative/rocm-cmake"; // Relative to CWD (temp_path)
            let absolute_rcm_path = temp_path.join(relative_rcm_path_str);
            fs::create_dir_all(&absolute_rcm_path).unwrap();

            // Config file is created in temp_path (CWD)
            create_temp_config_file(temp_path, &format!("rocm_cmake_path = \"{}\"", relative_rcm_path_str));

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.rocm_cmake_path, absolute_rcm_path);
            assert_eq!(config.source_dirs[0], absolute_rcm_path.parent().unwrap().to_path_buf());
        });
    }

    #[test]
    fn test_rocm_path_not_found_any_method() {
        run_env_test(|temp_path| {
            env::remove_var("ROCM_CMAKE_PATH");
            create_temp_config_file(temp_path, ""); // Empty config file

            // No rocm-cmake directory created for search either

            let cli = Cli::parse_from(["mytool", "build"]);
            let config_result = Config::from_cli(cli);

            assert!(config_result.is_err(), "Expected an error when rocm-cmake path is not found by any method");
            if let Err(e) = config_result {
                assert!(e.to_string().contains("'rocm-cmake' directory not found"));
            }
        });
    }

    // --- Tests for Config File Precedence ---

    // Tests for project_search_depth precedence
    #[test]
    fn test_depth_cli_over_config_over_default() {
        run_env_test(|temp_path| {
            fs::create_dir_all(temp_path.join("rocm-cmake")).unwrap();
            create_temp_config_file(temp_path, "default_project_search_depth = 5");

            let cli = Cli::parse_from(["mytool", "--project-search-depth", "10", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");
            assert_eq!(config.project_search_depth, 10, "CLI value should override config and default");
        });
    }

    #[test]
    fn test_depth_config_over_default() {
        run_env_test(|temp_path| {
            fs::create_dir_all(temp_path.join("rocm-cmake")).unwrap();
            create_temp_config_file(temp_path, "default_project_search_depth = 5");

            let cli = Cli::parse_from(["mytool", "build"]); // No CLI for depth
            let config = Config::from_cli(cli).expect("Config from CLI failed");
            assert_eq!(config.project_search_depth, 5, "Config value should override default");
        });
    }

    #[test]
    fn test_depth_default_used() {
        run_env_test(|temp_path| {
            fs::create_dir_all(temp_path.join("rocm-cmake")).unwrap();
            create_temp_config_file(temp_path, "# No default_project_search_depth in config");

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");
            assert_eq!(config.project_search_depth, super::DEFAULT_PROJECT_SEARCH_DEPTH, "Default value should be used");
        });
    }

    // Tests for source_dirs precedence
    #[test]
    fn test_src_dirs_cli_over_config_over_default() {
        run_env_test(|temp_path| {
            let rocm_cmake_parent_for_default = temp_path.join("rcm_default_parent");
            fs::create_dir_all(rocm_cmake_parent_for_default.join("rocm-cmake")).unwrap();
            // Temporarily set ROCM_CMAKE_PATH to control where the default parent is,
            // otherwise it defaults to temp_path itself if rocm-cmake is created there.
            env::set_var("ROCM_CMAKE_PATH", rocm_cmake_parent_for_default.join("rocm-cmake").to_str().unwrap());

            let cli_src_dir = temp_path.join("cli_src");
            fs::create_dir_all(&cli_src_dir).unwrap();

            let config_src_dir_rel = "config_src_relative";
            let config_src_dir_abs = temp_path.join(config_src_dir_rel);
            fs::create_dir_all(&config_src_dir_abs).unwrap();
            create_temp_config_file(temp_path, &format!("default_source_dirs = [\"{}\"]", config_src_dir_rel));

            let cli = Cli::parse_from(["mytool", "--source-dir", cli_src_dir.to_str().unwrap(), "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.source_dirs.len(), 1);
            assert_eq!(config.source_dirs[0], cli_src_dir, "CLI specified source_dir should be used");

            env::remove_var("ROCM_CMAKE_PATH");
        });
    }

    #[test]
    fn test_src_dirs_config_one_path_over_default() {
        run_env_test(|temp_path| {
            let rocm_cmake_parent_for_default = temp_path.join("rcm_default_parent");
            fs::create_dir_all(rocm_cmake_parent_for_default.join("rocm-cmake")).unwrap();
            env::set_var("ROCM_CMAKE_PATH", rocm_cmake_parent_for_default.join("rocm-cmake").to_str().unwrap());

            let config_src_dir_rel = "config_src_dir";
            let config_src_dir_abs = temp_path.join(config_src_dir_rel);
            fs::create_dir_all(&config_src_dir_abs).unwrap();
            create_temp_config_file(temp_path, &format!("default_source_dirs = [\"{}\"]", config_src_dir_rel));

            let cli = Cli::parse_from(["mytool", "build"]); // No --source-dir CLI
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.source_dirs.len(), 1);
            assert_eq!(config.source_dirs[0], config_src_dir_abs, "Config specified source_dir should be used");
            env::remove_var("ROCM_CMAKE_PATH");
        });
    }

    #[test]
    fn test_src_dirs_config_multiple_paths_over_default() {
        run_env_test(|temp_path| {
            let rocm_cmake_parent_for_default = temp_path.join("rcm_default_parent");
            fs::create_dir_all(rocm_cmake_parent_for_default.join("rocm-cmake")).unwrap();
            env::set_var("ROCM_CMAKE_PATH", rocm_cmake_parent_for_default.join("rocm-cmake").to_str().unwrap());

            let cfg_src1_rel = "cfg_projects1";
            let cfg_src1_abs = temp_path.join(cfg_src1_rel);
            fs::create_dir_all(&cfg_src1_abs).unwrap();

            // For absolute path in config, create it outside temp_path to ensure it's truly absolute handling
            let cfg_src2_abs_holder = tempfile::tempdir().unwrap();
            let cfg_src2_abs = cfg_src2_abs_holder.path().to_path_buf();
            fs::create_dir_all(&cfg_src2_abs).unwrap();


            create_temp_config_file(temp_path, &format!("default_source_dirs = [\"{}\", \"{}\"]", cfg_src1_rel, cfg_src2_abs.display().to_string().replace("\\", "/")));

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.source_dirs.len(), 2);
            let mut expected_dirs = vec![cfg_src1_abs, cfg_src2_abs];
            expected_dirs.sort();
            let mut actual_dirs = config.source_dirs.clone();
            actual_dirs.sort();
            assert_eq!(actual_dirs, expected_dirs, "Config specified source_dirs not matching");
            env::remove_var("ROCM_CMAKE_PATH");
        });
    }

    #[test]
    fn test_src_dirs_config_empty_list_falls_back_to_default() {
        run_env_test(|temp_path| {
            let rocm_cmake_parent_for_default = temp_path.join("rcm_default_parent");
            fs::create_dir_all(rocm_cmake_parent_for_default.join("rocm-cmake")).unwrap();
            env::set_var("ROCM_CMAKE_PATH", rocm_cmake_parent_for_default.join("rocm-cmake").to_str().unwrap());

            create_temp_config_file(temp_path, "default_source_dirs = []"); // Empty list

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.source_dirs.len(), 1);
            assert_eq!(config.source_dirs[0], rocm_cmake_parent_for_default, "Should use default rocm-cmake parent");
            env::remove_var("ROCM_CMAKE_PATH");
        });
    }

    #[test]
    fn test_src_dirs_default_used() {
        run_env_test(|temp_path| {
            let rocm_cmake_parent_for_default = temp_path.join("rcm_default_parent");
            fs::create_dir_all(rocm_cmake_parent_for_default.join("rocm-cmake")).unwrap();
            env::set_var("ROCM_CMAKE_PATH", rocm_cmake_parent_for_default.join("rocm-cmake").to_str().unwrap());

            create_temp_config_file(temp_path, "# No default_source_dirs key in config");

            let cli = Cli::parse_from(["mytool", "build"]);
            let config = Config::from_cli(cli).expect("Config from CLI failed");

            assert_eq!(config.source_dirs.len(), 1);
            assert_eq!(config.source_dirs[0], rocm_cmake_parent_for_default, "Should use default rocm-cmake parent");
            env::remove_var("ROCM_CMAKE_PATH");
        });
    }
}
