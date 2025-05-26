use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use log::{info, error};

mod build_logic;
mod clean_logic;
mod config;
mod outdir_logic;
mod utils;

use config::{Cli, Commands, Config};

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    let config = match Config::from_cli(cli.clone()) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Configuration error: {}", e);
            return Err(e);
        }
    };

    info!("ROCm Build Tool");
    info!("Build Directory: {}", config.build_dir.display());
    if let Some(install_dir) = &config.install_dir {
        info!("Install Directory: {}", install_dir.display());
    }
    if !config.packages.is_empty() {
        info!("Target Packages: {:?}", config.packages);
    }


    match cli.command {
        Commands::Build => {
            info!("Executing build command...");
            if let Err(e) = build_logic::run_build(&config) {
                error!("Build failed: {}", e);
                return Err(e);
            }
            info!("Build completed successfully.");
        }
        Commands::Clean => {
            info!("Executing clean command...");
            if let Err(e) = clean_logic::run_clean(&config) {
                error!("Clean failed: {}", e);
                return Err(e);
            }
            info!("Clean completed successfully.");
        }
        Commands::Outdir { packages } => {
            info!("Executing outdir command for packages: {:?}", packages);
            if let Err(e) = outdir_logic::run_outdir(&config, &packages) {
                error!("Outdir failed: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}
