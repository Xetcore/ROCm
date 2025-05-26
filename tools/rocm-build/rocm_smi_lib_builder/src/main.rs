use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use log::{info, error, warn};
use std::env;

mod build_smi_logic;
mod clean_smi_logic;
mod config_smi;
mod outdir_smi_logic;
mod utils_smi;

use config_smi::{RocmSmiCli, RocmSmiCommands, RocmSmiConfig};

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let mut cli = RocmSmiCli::parse();

    // Handle ENABLE_ADDRESS_SANITIZER env var influencing the cli.enable_address_sanitizer flag
    if let Ok(val) = env::var("ENABLE_ADDRESS_SANITIZER") {
        if !val.is_empty() && val != "0" && !cli.enable_address_sanitizer {
            warn!("ENABLE_ADDRESS_SANITIZER environment variable is set, overriding --enable-address-sanitizer to true.");
            cli.enable_address_sanitizer = true;
        }
    }
    
    // Handle ENABLE_STATIC_BUILDS env var
    if let Ok(val) = env::var("ENABLE_STATIC_BUILDS") {
        if !val.is_empty() && val != "0" && !cli.static_libs {
             warn!("ENABLE_STATIC_BUILDS environment variable is set, overriding --static-libs to true.");
            cli.static_libs = true;
        }
    }


    let mut config = match RocmSmiConfig::from_cli(cli.clone()) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Configuration error: {}", e);
            return Err(e);
        }
    };

    info!("ROCm SMI Lib Build Tool");
    info!("ROCm Path: {}", config.rocm_path.display());
    info!("ROCm SMI Lib Source Directory: {}", config.smi_src_dir.display());
    info!("Output Directory: {}", config.output_dir.display());
    if cli.verbose {
        info!("Verbose mode enabled.");
    }
    if cli.build_32_bit {
        info!("32-bit build mode enabled.");
        config.enable_32bit_build(); // Update config for 32-bit
    }
    if cli.enable_address_sanitizer {
        info!("AddressSanitizer (ASAN) enabled.");
        // Specific ASAN setup might be handled in build_smi_logic or by cmake flags
    }
     if cli.static_libs {
        info!("Static library build enabled.");
    }


    match cli.command {
        RocmSmiCommands::Build => {
            info!("Executing build command...");
            if let Err(e) = build_smi_logic::run_build(&config) {
                error!("Build failed: {}", e);
                return Err(e);
            }
            info!("Build completed successfully.");
        }
        RocmSmiCommands::Clean => {
            info!("Executing clean command...");
            if let Err(e) = clean_smi_logic::run_clean(&config) {
                error!("Clean failed: {}", e);
                return Err(e);
            }
            info!("Clean completed successfully.");
        }
        RocmSmiCommands::Outdir { pkg_type } => {
            info!("Executing outdir command for package type: {:?}", pkg_type);
            if let Err(e) = outdir_smi_logic::run_outdir(&config, &pkg_type) {
                error!("Outdir failed: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}
