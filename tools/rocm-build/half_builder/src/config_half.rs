use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use log::debug;
use std::path::{Path, PathBuf};
use std::fs;
use std::env;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "Builds the 'half' ROCm library.", long_about = None)]
pub struct HalfCli {
    #[arg(long, value_name = "PATH", env = "ROCM_PATH", default_value = "/opt/rocm", help = "Root path for ROCm installation")]
    pub rocm_path: PathBuf,

    #[arg(long, value_name = "PATH", env = "HALF_SRC_DIR", help = "Path to the 'half' library source directory. Defaults to './rocPRIM/external/half' or './external/rocprim-third-party/half' relative to the current directory if not set.")]
    pub half_src_dir: Option<PathBuf>,

    #[arg(long, value_name = "GENERATOR", env = "CPACKGEN", default_value = "DEB;RPM", help = "CPack generator string (e.g., 'DEB;RPM')")]
    pub cpack_generator: String,
    
    #[arg(long, value_name = "VERSION", env = "ROCM_LIBPATCH_VERSION", default_value = "0", help = "ROCm patch version for packaging")]
    pub rocm_patch_version: String,

    #[arg(long, value_name = "DIR", env = "OUT_DIR", default_value = "./output", help = "Base output directory for build artifacts and packages")]
    pub output_dir: PathBuf,

    #[arg(long, default_value_t = false, help = "Enable verbose output")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: HalfCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HalfCommands {
    /// Builds the 'half' library
    Build {
        #[arg(short, long, default_value_t = false, help = "Configure for a release build (CMAKE_BUILD_TYPE=Release)")]
        release: bool,
        #[arg(short, 'a', long, default_value_t = false, help = "Enable AddressSanitizer (if supported by the project)")]
        address_sanitizer: bool,
        #[arg(short, 's', long, default_value_t = false, help = "Build static libraries (BUILD_SHARED_LIBS=OFF)")]
        static_libs: bool,
        #[arg(long, value_enum, default_value_t = PackageTypeCopy::All, help = "Filter which package types to copy from build directory")]
        package_type_copy: PackageTypeCopy,
        #[arg(short, 'w', long, default_value_t = false, help = "Build Python wheels (if applicable)")]
        wheel: bool, // half does not produce wheels, but keep for consistency if other tools do
    },
    /// Cleans the build and package directories for 'half'
    Clean,
    /// Prints the package output directory for 'half'
    Outdir {
        #[arg(long, value_enum, default_value_t = PackageType::Deb, help = "Specify package type for output directory path")]
        pkg_type: PackageType,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum PackageType {
    Deb,
    Rpm,
}
impl PackageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PackageType::Deb => "deb",
            PackageType::Rpm => "rpm",
        }
    }
}


#[derive(Debug, Clone, ValueEnum)]
pub enum PackageTypeCopy {
    Deb,
    Rpm,
    All,
}

impl PackageTypeCopy {
    pub fn matches(&self, file_name: &str) -> bool {
        match self {
            PackageTypeCopy::All => file_name.ends_with(".deb") || file_name.ends_with(".rpm"),
            PackageTypeCopy::Deb => file_name.ends_with(".deb"),
            PackageTypeCopy::Rpm => file_name.ends_with(".rpm"),
        }
    }
    pub fn as_str_for_glob(&self) -> &'static str {
        match self {
            PackageTypeCopy::All => "*.{deb,rpm}",
            PackageTypeCopy::Deb => "*.deb",
            PackageTypeCopy::Rpm => "*.rpm",
        }
    }
}


#[derive(Debug, Clone)]
pub struct HalfConfig {
    pub rocm_path: PathBuf,
    pub half_src_dir: PathBuf,
    pub cpack_generator: String,
    pub rocm_patch_version: String,
    pub output_dir: PathBuf,
    pub build_dir_half: PathBuf, // Specific build directory for 'half'
    pub package_dir_half: PathBuf, // Specific package directory for 'half'
}

impl HalfConfig {
    pub fn from_cli(cli: HalfCli) -> Result<Self> {
        let current_dir = env::current_dir().context("Failed to get current directory")?;

        let rocm_path = if cli.rocm_path.is_absolute() {
            cli.rocm_path
        } else {
            current_dir.join(cli.rocm_path)
        };
        debug!("Resolved ROCm path: {}", rocm_path.display());

        let half_src_dir = match cli.half_src_dir {
            Some(p) => if p.is_absolute() { p } else { current_dir.join(p) },
            None => {
                // Default logic for half_src_dir:
                // Check ./rocPRIM/external/half then ./external/rocprim-third-party/half
                let path1 = current_dir.join("rocPRIM/external/half");
                if path1.exists() && path1.is_dir() {
                    path1
                } else {
                    let path2 = current_dir.join("external/rocprim-third-party/half");
                    if path2.exists() && path2.is_dir() {
                        path2
                    } else {
                        // Fallback to a simple 'half' directory if the others don't exist.
                        // This might be useful if the tool is run from within a 'half' checkout directly.
                        let path3 = current_dir.join("half");
                        if path3.exists() && path3.is_dir() && path3.join("CMakeLists.txt").exists() {
                            path3
                        } else {
                             return Err(anyhow!(
                                "Default 'half' source directory not found at ./rocPRIM/external/half, ./external/rocprim-third-party/half, or ./half. Please specify with --half-src-dir or ensure it's in one of the default locations."
                            ));
                        }
                    }
                }
            }
        };
        if !half_src_dir.exists() || !half_src_dir.is_dir() {
            return Err(anyhow!("'half' source directory does not exist or is not a directory: {}", half_src_dir.display()));
        }
        if !half_src_dir.join("CMakeLists.txt").exists() {
             return Err(anyhow!("CMakeLists.txt not found in 'half' source directory: {}. Ensure --half-src-dir points to the correct location.", half_src_dir.display()));
        }
        debug!("Resolved 'half' source directory: {}", half_src_dir.display());


        let output_dir = if cli.output_dir.is_absolute() {
            cli.output_dir
        } else {
            current_dir.join(cli.output_dir)
        };
        debug!("Resolved output directory: {}", output_dir.display());

        let build_dir_half = output_dir.join("build/half");
        let package_dir_half = output_dir.join("package/half");

        fs::create_dir_all(&build_dir_half)
            .with_context(|| format!("Failed to create build directory for half: {}", build_dir_half.display()))?;
        fs::create_dir_all(&package_dir_half)
            .with_context(|| format!("Failed to create package directory for half: {}", package_dir_half.display()))?;
        
        Ok(HalfConfig {
            rocm_path,
            half_src_dir,
            cpack_generator: cli.cpack_generator,
            rocm_patch_version: cli.rocm_patch_version,
            output_dir,
            build_dir_half,
            package_dir_half,
        })
    }

    pub fn get_package_output_dir_for_type(&self, pkg_type: &PackageType) -> PathBuf {
        self.package_dir_half.join(pkg_type.as_str())
    }
}
