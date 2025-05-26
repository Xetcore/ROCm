use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use log::debug;
use std::path::{Path, PathBuf};
use std::fs;
use std::env;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "Builds the 'rocm-smi-lib' ROCm library.", long_about = None)]
pub struct RocmSmiCli {
    #[arg(long, value_name = "PATH", env = "ROCM_PATH", default_value = "/opt/rocm", help = "Root path for ROCm installation")]
    pub rocm_path: PathBuf,

    #[arg(long, value_name = "PATH", env = "SMI_SRC_DIR", default_value = ".", help = "Path to the 'rocm-smi-lib' source directory (containing CMakeLists.txt)")]
    pub smi_src_dir: PathBuf,

    #[arg(long, value_name = "GENERATOR", env = "CPACKGEN", default_value = "DEB;RPM", help = "CPack generator string (e.g., 'DEB;RPM')")]
    pub cpack_generator: String,
    
    #[arg(long, value_name = "VERSION", env = "ROCM_LIBPATCH_VERSION", default_value = "0", help = "ROCm patch version for packaging")]
    pub rocm_patch_version: String,

    #[arg(long, value_name = "DIR", env = "OUT_DIR", default_value = "./output", help = "Base output directory for build artifacts and packages")]
    pub output_dir: PathBuf,
    
    #[arg(long, default_value_t = false, help = "Enable 32-bit build mode")]
    pub build_32_bit: bool,

    #[arg(long, default_value_t = false, help = "Enable static library builds (BUILD_SHARED_LIBS=OFF)")]
    pub static_libs: bool,
    
    #[arg(long, default_value_t = false, env = "ENABLE_ADDRESS_SANITIZER", help = "Enable AddressSanitizer (ASAN) build")]
    pub enable_address_sanitizer: bool,
    
    #[arg(long, default_value_t = false, help = "Enable verbose output")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: RocmSmiCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum RocmSmiCommands {
    /// Builds the 'rocm-smi-lib' library
    Build,
    /// Cleans the build and package directories for 'rocm-smi-lib'
    Clean,
    /// Prints the package output directory for 'rocm-smi-lib'
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
    pub fn glob_suffix(&self) -> &'static str {
        match self {
            PackageType::Deb => "*.deb",
            PackageType::Rpm => "*.rpm",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RocmSmiConfig {
    pub rocm_path: PathBuf,
    pub smi_src_dir: PathBuf,
    pub cpack_generator: String,
    pub rocm_patch_version: String,
    pub output_dir: PathBuf,
    pub build_dir_smi: PathBuf, 
    pub package_dir_smi: PathBuf,
    pub is_32_bit_build: bool,
    pub static_libs: bool,
    pub enable_address_sanitizer: bool,
    pub cmake_prefix_path: PathBuf, // For dependencies like rocm-cmake
    pub package_name_suffix: String, // e.g. "-rocm-smi-lib64" or "-rocm-smi-lib32"
    pub lib_dir_suffix: String, // Typically "lib" or "lib64"
}

impl RocmSmiConfig {
    pub fn from_cli(cli: RocmSmiCli) -> Result<Self> {
        let current_dir = env::current_dir().context("Failed to get current directory")?;

        let rocm_path = if cli.rocm_path.is_absolute() {
            cli.rocm_path
        } else {
            current_dir.join(cli.rocm_path)
        };
        debug!("Resolved ROCm path: {}", rocm_path.display());

        let smi_src_dir = if cli.smi_src_dir.is_absolute() {
            cli.smi_src_dir.clone()
        } else {
            current_dir.join(cli.smi_src_dir)
        };
        if !smi_src_dir.exists() || !smi_src_dir.is_dir() {
            return Err(anyhow!("'rocm-smi-lib' source directory does not exist or is not a directory: {}", smi_src_dir.display()));
        }
        if !smi_src_dir.join("CMakeLists.txt").exists() {
             return Err(anyhow!("CMakeLists.txt not found in 'rocm-smi-lib' source directory: {}. Ensure --smi-src-dir points to the correct location.", smi_src_dir.display()));
        }
        debug!("Resolved 'rocm-smi-lib' source directory: {}", smi_src_dir.display());
        
        let output_dir = if cli.output_dir.is_absolute() {
            cli.output_dir
        } else {
            current_dir.join(cli.output_dir)
        };
        debug!("Resolved output directory: {}", output_dir.display());

        let build_dir_smi = output_dir.join("build/rocm-smi-lib");
        let package_dir_smi = output_dir.join("package/rocm-smi-lib");

        fs::create_dir_all(&build_dir_smi)
            .with_context(|| format!("Failed to create build directory for rocm-smi-lib: {}", build_dir_smi.display()))?;
        fs::create_dir_all(&package_dir_smi)
            .with_context(|| format!("Failed to create package directory for rocm-smi-lib: {}", package_dir_smi.display()))?;
        
        // Determine cmake_prefix_path (expect rocm-cmake to be in rocm_path or a standard build location)
        // This assumes rocm-cmake was installed into rocm_path or is findable via standard CMake search paths if not specified.
        // A more robust solution might involve searching or using an env var for rocm-cmake build dir.
        let cmake_prefix_path = rocm_path.join("lib/cmake/rocm-cmake"); 
        // An alternative could be `rocm_path.join("lib/cmake/ROCMPackage")` or just `rocm_path` itself
        // if rocm-cmake installed its find modules there. The script used `ROCM_CMAKE_PATH=$ROCM_PATH/lib/cmake/rocm-cmake`.

        let (package_name_suffix, lib_dir_suffix) = if cli.build_32_bit {
            ("-rocm-smi-lib32".to_string(), "lib".to_string()) // Assuming 32-bit libs go to 'lib' not 'lib32'
        } else {
            ("-rocm-smi-lib64".to_string(), "lib64".to_string())
        };


        Ok(RocmSmiConfig {
            rocm_path,
            smi_src_dir,
            cpack_generator: cli.cpack_generator,
            rocm_patch_version: cli.rocm_patch_version,
            output_dir,
            build_dir_smi,
            package_dir_smi,
            is_32_bit_build: cli.build_32_bit,
            static_libs: cli.static_libs,
            enable_address_sanitizer: cli.enable_address_sanitizer,
            cmake_prefix_path,
            package_name_suffix,
            lib_dir_suffix,
        })
    }
    
    // Method to update config for 32-bit mode if it wasn't set initially via CLI
    // This is called from main.rs if the flag is set there.
    pub fn enable_32bit_build(&mut self) {
        if !self.is_32_bit_build { // only update if not already set
            self.is_32_bit_build = true;
            self.package_name_suffix = "-rocm-smi-lib32".to_string();
            self.lib_dir_suffix = "lib".to_string(); // Or "lib32" if that's the target convention
            debug!("Config updated for 32-bit build mode.");
        }
    }


    pub fn get_package_output_dir_for_type(&self, pkg_type: &PackageType) -> PathBuf {
        self.package_dir_smi.join(pkg_type.as_str())
    }
}
