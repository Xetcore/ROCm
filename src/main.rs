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
    let cli = Cli::parse();

    let default_log_level = if cli.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(Env::default().default_filter_or(default_log_level)).init();

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

#[cfg(test)]
mod tests {
    use crate::config::Cli; // Assuming Cli is in crate::config
    use clap::Parser;

    #[test]
    fn test_log_level_string_verbose() {
        // Cli::parse_from requires an iterator, and a command.
        // Assuming 'build' is a valid subcommand for Cli.
        let cli = Cli::parse_from(["mytool", "--verbose", "build"]);
        let default_log_level = if cli.verbose { "debug" } else { "info" };
        assert_eq!(default_log_level, "debug");
    }

    #[test]
    fn test_log_level_string_not_verbose() {
        let cli = Cli::parse_from(["mytool", "build"]);
        let default_log_level = if cli.verbose { "debug" } else { "info" };
        assert_eq!(default_log_level, "info");
    }
}
