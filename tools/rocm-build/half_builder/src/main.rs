use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use log::{info, error};

mod build_half_logic;
mod clean_half_logic;
mod config_half;
mod outdir_half_logic;
mod utils_half;

use config_half::{HalfCli, HalfCommands, HalfConfig};

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = HalfCli::parse();

    let config = match HalfConfig::from_cli(cli.clone()) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Configuration error: {}", e);
            return Err(e);
        }
    };

    info!("Half Build Tool");
    info!("ROCm Path: {}", config.rocm_path.display());
    info!("Half Source Directory: {}", config.half_src_dir.display());
    info!("Output Directory: {}", config.output_dir.display());
    if cli.verbose {
        info!("Verbose mode enabled.");
    }


    match cli.command {
        HalfCommands::Build { release, address_sanitizer, static_libs, package_type_copy, wheel } => {
            info!("Executing build command...");
            if let Err(e) = build_half_logic::run_build(&config, release, address_sanitizer, static_libs, &package_type_copy, wheel) {
                error!("Build failed: {}", e);
                return Err(e);
            }
            info!("Build completed successfully.");
        }
        HalfCommands::Clean => {
            info!("Executing clean command...");
            if let Err(e) = clean_half_logic::run_clean(&config) {
                error!("Clean failed: {}", e);
                return Err(e);
            }
            info!("Clean completed successfully.");
        }
        HalfCommands::Outdir { pkg_type } => {
            info!("Executing outdir command for package type: {:?}", pkg_type);
            if let Err(e) = outdir_half_logic::run_outdir(&config, &pkg_type) {
                error!("Outdir failed: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}
