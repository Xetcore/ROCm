use anyhow::{Result, anyhow};
use log::info;

use crate::config::Config;

pub fn run_outdir(config: &Config, package_names: &[String]) -> Result<()> {
    if package_names.is_empty() {
        return Err(anyhow!("No packages specified for the 'outdir' command."));
    }

    info!("Printing output directories for packages: {:?}", package_names);

    for package_name in package_names {
        let normalized_name = package_name.trim_end_matches('/');

        let package_build_dir = config.get_package_build_dir(normalized_name);
        println!("{}", package_build_dir.display());

        // Optionally, print install directory if configured, though `outdir` typically refers to build output
        // if let Some(package_install_dir) = config.get_package_install_dir(normalized_name) {
        //     println!("Install dir for {}: {}", normalized_name, package_install_dir.display());
        // }
    }

    Ok(())
}
