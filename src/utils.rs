use anyhow::{Context, Result, anyhow};
use log::{debug, error, info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use walkdir::WalkDir;

/// Runs a command and checks its exit status.
pub fn run_command(mut command: Command, description: &str) -> Result<()> {
    debug!("Running command for {}: {:?}", description, command);
    let status = command
        .status()
        .with_context(|| format!("Failed to execute command for {}", description))?;

    if !status.success() {
        error!(
            "Command for {} failed with status: {}",
            description, status
        );
        Err(anyhow!("Command failed: {}", description))
    } else {
        info!("Successfully executed command for {}", description);
        Ok(())
    }
}

/// Identifies CMake-based project directories within a given source directory,
/// up to a specified search depth.
/// Optionally excludes a specific path (e.g., rocm-cmake itself).
pub fn find_cmake_projects(
    source_dir: &Path,
    exclude_path: Option<&Path>,
    search_depth: usize,
) -> Result<Vec<PathBuf>> {
    debug!(
        "Searching for CMake projects in: {} up to depth {} (excluding {:?})",
        source_dir.display(),
        search_depth,
        exclude_path
    );
    let mut projects = Vec::new();

    for entry_result in WalkDir::new(source_dir)
        .max_depth(search_depth) // Corrected: Use search_depth directly
        .filter_entry(|e| {
            if let Some(ex_path) = exclude_path {
                if e.path() == ex_path {
                    debug!("Excluding path due to filter_entry: {}", e.path().display());
                    return false;
                }
            }
            true
        })
    {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                warn!("Error accessing entry during CMake project search: {}", e);
                continue;
            }
        };

        let path = entry.path();

        if path.is_dir() {
            let cmakelists_path = path.join("CMakeLists.txt");
            if cmakelists_path.is_file() {
                // Ensure not to add duplicates if symlinks or other structures cause multiple listings.
                // PathBuf should be comparable and hashable for this.
                let project_path_buf = path.to_path_buf();
                if !projects.contains(&project_path_buf) {
                    debug!("Found CMake project at: {}", path.display());
                    projects.push(project_path_buf);
                }
            }
        }
    }

    if projects.is_empty() {
        info!(
            "No CMake projects found in {} up to depth {}.",
            source_dir.display(),
            search_depth
        );
    }
    Ok(projects)
}


#[derive(PartialEq, Eq, Debug)]
pub enum SelectedPurpose {
    Build,
    Clean,
    Outdir,
}

/// Checks if a package should be processed based on the user's selection.
/// If `selected_packages` is empty, it means "all packages" are implicitly selected.
/// Otherwise, only packages explicitly listed are selected.
pub fn is_package_selected(selected_packages: &[String], package_name: &str, purpose: SelectedPurpose) -> bool {
    if selected_packages.is_empty() {
        debug!("No specific packages selected for {:?}, processing all (including '{}').", purpose, package_name);
        return true; // No specific packages listed, so all are considered selected
    }
    let normalized_package_name = package_name.trim_end_matches('/');
    let is_selected = selected_packages.iter().any(|s| s.trim_end_matches('/') == normalized_package_name);
    if is_selected {
        debug!("Package '{}' is selected for {:?}.", package_name, purpose);
    } else {
        debug!("Package '{}' is NOT selected for {:?}.", package_name, purpose);
    }
    is_selected
}

#[cfg(test)]
mod tests {
    use super::find_cmake_projects;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    // Helper function to create mock projects
    fn create_project_at(base_path: &Path, project_name_rel: &str) -> PathBuf {
        let project_dir = base_path.join(project_name_rel);
        fs::create_dir_all(&project_dir).unwrap();
        fs::File::create(project_dir.join("CMakeLists.txt")).unwrap();
        project_dir
    }

    #[test]
    fn test_find_depth_0_source_is_project() {
        let temp_root_holder = tempdir().unwrap();
        let temp_root = temp_root_holder.path();
        // Source dir itself is a project
        fs::File::create(temp_root.join("CMakeLists.txt")).unwrap();
        create_project_at(temp_root, "subproject1"); // Deeper, should not be found at depth 0

        let projects = find_cmake_projects(temp_root, None, 0).unwrap();
        assert_eq!(projects.len(), 1, "Projects found: {:?}", projects);
        assert!(projects.contains(&temp_root.to_path_buf()));
    }

    #[test]
    fn test_find_depth_0_source_not_project() {
        let temp_root_holder = tempdir().unwrap();
        let temp_root = temp_root_holder.path();
        create_project_at(temp_root, "subproject1"); // Should not be found at depth 0

        let projects = find_cmake_projects(temp_root, None, 0).unwrap();
        // Only source_dir itself is checked at depth 0. If it's not a project, none should be found.
        assert!(projects.is_empty(), "Projects found: {:?}", projects);
    }

    #[test]
    fn test_find_depth_1() {
        let temp_root_holder = tempdir().unwrap();
        let temp_root = temp_root_holder.path();
        // Project in source_dir itself (depth 0)
        fs::File::create(temp_root.join("CMakeLists.txt")).unwrap();
        // Project in immediate subdir (depth 1)
        let subproject1 = create_project_at(temp_root, "subproject1");
        // Project deeper, should not be found at depth 1
        let sub_sub_project_base = subproject1.join("sub_sub1");
        create_project_at(&sub_sub_project_base, "deep_project"); // actual path: temp_root/subproject1/sub_sub1/deep_project

        let projects = find_cmake_projects(temp_root, None, 1).unwrap();
        assert_eq!(projects.len(), 2, "Projects found: {:?}", projects);
        assert!(projects.contains(&temp_root.to_path_buf()));
        assert!(projects.contains(&subproject1));
        assert!(!projects.contains(&sub_sub_project_base.join("deep_project")));
    }

    #[test]
    fn test_find_depth_2() {
        let temp_root_holder = tempdir().unwrap();
        let temp_root = temp_root_holder.path();
        fs::File::create(temp_root.join("CMakeLists.txt")).unwrap(); // Proj0 (depth 0)
        let subproject1 = create_project_at(temp_root, "subproject1"); // Proj1 (depth 1)
        let sub_sub_project_path = subproject1.join("sub_sub1");
        let deep_project = create_project_at(&sub_sub_project_path, "deep_project"); // Proj2 (depth 2)

        let subproject2 = create_project_at(temp_root, "subproject2"); // Proj3 (depth 1)

        let projects = find_cmake_projects(temp_root, None, 2).unwrap();

        assert_eq!(projects.len(), 4, "Projects found: {:?}", projects);
        assert!(projects.contains(&temp_root.to_path_buf()));
        assert!(projects.contains(&subproject1));
        assert!(projects.contains(&subproject2));
        assert!(projects.contains(&deep_project));
    }

    #[test]
    fn test_find_with_exclude() {
        let temp_root_holder = tempdir().unwrap();
        let temp_root = temp_root_holder.path();
        let project_a = create_project_at(temp_root, "projectA");

        let project_b_excluded_parent = temp_root.join("projectB_parent");
        fs::create_dir_all(&project_b_excluded_parent).unwrap();
        // This project is inside projectB_parent, and projectB_parent will be excluded.
        let project_b_inside_excluded_parent = create_project_at(&project_b_excluded_parent, "projectB_actual");

        // Test excluding the parent directory of project_b
        let projects_v1 = find_cmake_projects(temp_root, Some(&project_b_excluded_parent), 2).unwrap();
        assert_eq!(projects_v1.len(), 1, "Projects_v1 found: {:?}", projects_v1);
        assert!(projects_v1.contains(&project_a));
        assert!(!projects_v1.contains(&project_b_inside_excluded_parent));

        // Test excluding the specific projectB_actual directory itself.
        // This should also work because filter_entry will skip it.
        let project_c_sibling_of_b_parent = create_project_at(temp_root, "projectC");
        let projects_v2 = find_cmake_projects(temp_root, Some(&project_b_inside_excluded_parent), 2).unwrap();
        assert_eq!(projects_v2.len(), 2, "Projects_v2 found: {:?}, excluded: {:?}", projects_v2, project_b_inside_excluded_parent.display());
        assert!(projects_v2.contains(&project_a));
        assert!(projects_v2.contains(&project_c_sibling_of_b_parent)); // projectC should be found
        assert!(!projects_v2.contains(&project_b_inside_excluded_parent)); // projectB actual still excluded
        // projectB_parent itself is not a cmake project (no CMakeLists.txt directly in it), so it won't be listed.
    }
}
