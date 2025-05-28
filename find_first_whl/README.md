# Find First Wheel (`find-first-whl`)

`find-first-whl` is a command-line utility written in Rust that searches a specified directory for files matching a given pattern, selects the alphabetically first match, and appends its base filename to a specified output file.

This tool was created as a Rust replacement for a small shell script used in Azure DevOps pipelines.

## Features

-   Searches a directory for files.
-   Filters files based on a name pattern (defaults to `*.whl`).
-   Selects the alphabetically first matching file if multiple matches are found.
-   Appends the base name of the selected file to a specified output file.
-   Verbose mode for detailed logging.
-   Cross-platform (wherever Rust can compile).

## Usage

### Command-Line Arguments

```
find-first-whl [OPTIONS] --search-dir <PATH> --output-file <PATH>
```

**Options:**

*   `-d, --search-dir <PATH>`:
    The directory to search for files. (Required)

*   `-o, --output-file <PATH>`:
    The file to append the found filename to. This file will be created if it doesn't exist. (Required)

*   `-p, --pattern <PATTERN>`:
    The file matching pattern.
    - If the pattern starts with `*.` (e.g., `*.whl`, `*.txt`), it matches files ending with that extension.
    - Otherwise, it performs a simple "contains" match on the filename.
    (Default: `*.whl`)

*   `-v, --verbose`:
    Enable verbose logging to print more details about the process.

*   `-h, --help`:
    Print help information.

*   `-V, --version`:
    Print version information.

### Examples

1.  **Find a `.whl` file and record its name:**
    ```bash
    find-first-whl --search-dir ./my_artifacts/dist --output-file ./found_wheels.txt
    ```
    This will search `./my_artifacts/dist` for `*.whl` files. If `example-1.0.0-py3-none-any.whl` is found first alphabetically, `example-1.0.0-py3-none-any.whl` will be appended to `./found_wheels.txt`.

2.  **Find a specific text file with verbose output:**
    ```bash
    find-first-whl -d ./logs -o ./results.txt -p "error_log_*.txt" -v
    ```
    This will search `./logs` for files containing `error_log_` and ending with `.txt` (due to current simple pattern logic for `*.ext`), print verbose information, and append the name of the first match to `results.txt`.

## Build Instructions

To build the utility:

1.  Ensure you have Rust and Cargo installed (see [rustup.rs](https://rustup.rs/)).
2.  Navigate to the `find_first_whl` project directory.
3.  Run the build command:
    ```bash
    cargo build --release
    ```
    The compiled binary will be located at `target/release/find-first-whl`.

## Development Notes

- The current pattern matching is basic:
    - `*.ext` matches files ending with `.ext`.
    - Other patterns perform a substring containment check.
    - For true glob support, a crate like `glob` would be a future enhancement.
- The file search with `walkdir` is currently configured to be non-recursive (`max_depth(1)`), searching only the top-level of the specified `search-dir`. This mimics the original shell script's `find . -maxdepth 1 ...` or `find . -type f ... | head` behavior. It can be made recursive by adjusting `max_depth` in the code.
```
