use anyhow::{Result, anyhow};
use log::info;

use crate::config_half::{HalfConfig, PackageType};

pub fn run_outdir(config: &HalfConfig, pkg_type: &PackageType) -> Result<()> {
    if pkg_type.as_str().is_empty() { // Should not happen with enum
        return Err(anyhow!("Package type for 'outdir' cannot be empty."));
    }

    info!("Printing package output directory for 'half', package type: {:?}", pkg_type);

    let package_output_dir = config.get_package_output_dir_for_type(pkg_type);
    
    // Ensure the directory path is created before printing, as some tools might expect it to exist.
    // However, the original script's `outdir` just prints the path.
    // Let's stick to printing, and build command will create it.
    // std::fs::create_dir_all(&package_output_dir).with_context(|| {
    //     format!("Failed to create package output directory: {}", package_output_dir.display())
    // })?;

    println!("{}", package_output_dir.display());

    Ok(())
}
