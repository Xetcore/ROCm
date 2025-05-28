use clap::Parser;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir; // Added for directory traversal
use anyhow::{Context, Result, anyhow}; // Added anyhow for error handling

/// Finds the first file matching a pattern in a directory and appends its basename to an output file.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// The directory to search for files.
    #[arg(long, short = 'd', value_name = "PATH")]
    search_dir: PathBuf,

    /// The file to append the found filename to.
    #[arg(long, short = 'o', value_name = "PATH")]
    output_file: PathBuf,

    /// The file matching pattern (e.g., "*.whl", "file_*.txt").
    /// Note: This basic version uses ends_with for "*.ext" patterns.
    /// For true globbing, a crate like `glob` would be needed.
    #[arg(long, short = 'p', value_name = "PATTERN", default_value = "*.whl")]
    pattern: String,

    /// Enable verbose logging.
    #[arg(long, short = 'v', action = clap::ArgAction::SetTrue)]
    verbose: bool,
}

fn main() {
    let args = CliArgs::parse();

    if args.verbose {
        println!("Searching in directory: {:?}", args.search_dir);
        println!("Output file: {:?}", args.output_file);
        println!("Using pattern: {}", args.pattern);
    }

    if let Err(e) = run(args) {
        eprintln!("Error: {:?}", e); // Changed to print full error chain with :?
        std::process::exit(1);
    }
}

fn run(args: CliArgs) -> Result<()> {
    if !args.search_dir.is_dir() {
        return Err(anyhow::anyhow!("Search path {:?} is not a directory or does not exist.", args.search_dir));
    }

    let mut matching_files: Vec<PathBuf> = Vec::new();

    // Iterate over entries in the search directory.
    // min_depth(1) avoids processing the search_dir itself.
    // max_depth(1) restricts search to the top-level directory, not subdirectories.
    for entry in WalkDir::new(&args.search_dir).min_depth(1).max_depth(1) {
        let entry = entry.with_context(|| format!("Failed to read entry in {:?}", args.search_dir))?;
        if entry.file_type().is_file() {
            let file_name = entry.file_name().to_string_lossy();
            
            // Basic pattern matching logic:
            // If pattern starts with "*.", treat it as an extension match (e.g., "*.whl").
            // Otherwise, perform a simple substring containment check.
            // This is a simplification; for true glob patterns, a crate like `glob` would be more robust.
            let matches_pattern = if args.pattern.starts_with("*.") {
                // Extract the extension part from the pattern (e.g., "whl" from "*.whl").
                let extension = &args.pattern[2..]; 
                file_name.ends_with(extension)
            } else {
                // Perform a simple contains check if not an extension-style pattern.
                file_name.contains(&args.pattern)
            };

            if matches_pattern {
                if args.verbose {
                    println!("Found potential match: {:?}", entry.path());
                }
                matching_files.push(entry.path().to_path_buf());
            }
        }
    }

    if matching_files.is_empty() {
        if args.verbose {
            println!("No files found matching pattern '{}' in {:?}", args.pattern, args.search_dir);
        }
        // As per the original Python script's behavior, exit gracefully if no files are found.
        return Ok(()); 
    }

    // Sort the collected files alphabetically to ensure a deterministic "first" file.
    // This makes the behavior consistent if multiple files match the pattern.
    matching_files.sort();

    // Select the first file from the sorted list.
    let first_file_path = &matching_files[0];
    // Extract the filename (basename) from the full path.
    // This is what will be written to the output file.
    let first_file_name = first_file_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Failed to get filename from path: {:?}", first_file_path))?
        .to_string_lossy();

    if args.verbose {
        println!("Selected file (first alphabetically): {:?}", first_file_path);
        println!("Extracted filename to write: {}", first_file_name);
    }

    // Open the output file in append mode.
    // If the file doesn't exist, it will be created.
    // If it exists, the new filename will be added to the end.
    let mut output_file = OpenOptions::new()
        .create(true) // Create the file if it doesn't exist.
        .append(true) // Append to the file if it exists, otherwise write at the beginning.
        .open(&args.output_file)
        .with_context(|| format!("Failed to open or create output file: {:?}", args.output_file))?;

    // Write the extracted filename followed by a newline character to the output file.
    writeln!(output_file, "{}", first_file_name)
        .with_context(|| format!("Failed to write filename to output file: {:?}", args.output_file))?;

    if args.verbose {
        println!("Successfully wrote filename '{}' to {:?}", first_file_name, args.output_file);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    // Helper function to encapsulate the pattern matching logic for testing,
    // mirroring the logic within the `run` function.
    fn check_pattern_logic(filename: &str, pattern: &str) -> bool {
        if pattern.is_empty() {
            return false; // Define behavior for empty pattern: does not match anything.
        }
        if pattern.starts_with("*.") {
            // Handles "*.ext"
            if pattern.len() < 3 { // e.g., pattern is just "*."
                // Define behavior: could match files ending with '.', or be invalid.
                // Let's say it matches files literally ending with a dot if pattern is "*."
                return filename.ends_with(&pattern[1..]);
            }
            let extension = &pattern[2..];
            filename.ends_with(extension)
        } else {
            // Handles "contains"
            filename.contains(pattern)
        }
    }

    #[test]
    fn test_extension_pattern() {
        assert!(check_pattern_logic("example.whl", "*.whl"));
        assert!(!check_pattern_logic("example.txt", "*.whl"));
        assert!(check_pattern_logic("archive.tar.gz", "*.tar.gz"));
        assert!(!check_pattern_logic("archive.tar", "*.tar.gz"));
        assert!(check_pattern_logic("my.file.dots.ext", "*.ext"));
    }

    #[test]
    fn test_contains_pattern() {
        assert!(check_pattern_logic("my_file_name.txt", "file_name"));
        assert!(check_pattern_logic("another.zip", "another"));
        assert!(!check_pattern_logic("widget.dll", "foo"));
    }

    #[test]
    fn test_edge_case_patterns() {
        // Empty filename
        assert!(!check_pattern_logic("", "*.whl"));
        assert!(!check_pattern_logic("", "any"));

        // Empty pattern (defined to match nothing)
        assert!(!check_pattern_logic("example.whl", ""));

        // Pattern "*."
        assert!(check_pattern_logic("file.ending.with.", "*."));
        assert!(!check_pattern_logic("filewithoutdot", "*."));
        
        // Pattern "*.a" (short extension)
        assert!(check_pattern_logic("file.a", "*.a"));
        assert!(!check_pattern_logic("file.b", "*.a"));

        // Pattern is the filename itself
        assert!(check_pattern_logic("exact_match", "exact_match"));
        assert!(check_pattern_logic("exact_match.ext", "*.ext")); // ensure it still uses ext logic
    }

    #[test]
    fn test_pattern_similar_to_filename() {
        assert!(check_pattern_logic("file.whl.bak", "*.whl.bak"));
        assert!(check_pattern_logic("some.bashrc", "*.bashrc"));
        // This case depends on the definition if `*.bashrc` should only match if `.bashrc` is the *final* extension.
        // Current `ends_with` logic for "*.ext" means it will match.
        assert!(check_pattern_logic("myfile.bashrc.backup", "*.bashrc")); // This would be false
        assert!(!check_pattern_logic("myfile.backup.bashrc", "*.bashrc")); // This would be true
    }
}
