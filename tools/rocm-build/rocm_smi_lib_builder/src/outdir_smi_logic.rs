use anyhow::{Result, anyhow};
use log::info;

use crate::config_smi::{RocmSmiConfig, PackageType};

pub fn run_outdir(config: &RocmSmiConfig, pkg_type: &PackageType) -> Result<()> {
    if pkg_type.as_str().is_empty() { 
        return Err(anyhow!("Package type for 'outdir' cannot be empty."));
    }

    info!("Printing package output directory for 'rocm-smi-lib', package type: {:?}", pkg_type);

    let package_output_dir = config.get_package_output_dir_for_type(pkg_type);
    
    // The original script's `outdir` just prints the path.
    // Build command will create it.
    println!("{}", package_output_dir.display());

    Ok(())
}
