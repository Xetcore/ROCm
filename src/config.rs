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
        })
    }

    pub fn get_package_build_dir(&self, package_name: &str) -> PathBuf {
        self.build_dir.join(package_name)
    }

    pub fn get_package_install_dir(&self, package_name: &str) -> Option<PathBuf> {
        self.install_dir.as_ref().map(|idir| idir.join(package_name))
    }
}
