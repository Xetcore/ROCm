# ROCm CMake Builder (Rust) - `rocm-cmake-builder`

`rocm-cmake-builder` is a command-line utility written in Rust designed to replace the functionality of the `build_rocm-cmake.sh` shell script. It handles the building and packaging of the `rocm-cmake` component.

**Note:** This utility is currently in Phase 1 of development. It supports cleaning and basic build (configure and install) operations. Packaging, wheel creation, and other advanced features will be added in subsequent phases.

## Phase 1 Features

-   Cleaning of build and package directories.
-   Configuration of `rocm-cmake` via CMake.
-   Building and installing `rocm-cmake` using CMake.
-   CLI arguments for specifying paths, build type (debug/release), and library type (static/shared).

## Usage (Phase 1)

### Command-Line Arguments

```
rocm-cmake-builder [OPTIONS] --rocm-cmake-root <PATH>
```

**Options:**

*   `--rocm-cmake-root <PATH>`:
    Path to the `rocm-cmake` source directory. (Required)

*   `--build-dir <PATH>`:
    Optional: Specify the build directory.
    Defaults to `<rocm-cmake-root>/build/rocm-cmake-builder`.

*   `--package-root <PATH>`:
    Optional: Specify the root directory for future packages (not fully used in Phase 1 for output, but directories are created/cleaned).
    Defaults to `<rocm-cmake-root>/dist`.

*   `-c, --clean`:
    Clean output and delete all intermediate work (build and package directories). If this flag is present, build actions are skipped.

*   `-r, --release`:
    Build a release version of `rocm-cmake` (sets `CMAKE_BUILD_TYPE=Release`). Default is debug.

*   `-s, --static-libs`:
    Configure `rocm-cmake` to build static libraries (sets `BUILD_SHARED_LIBS=OFF`). Default is shared libraries.
    (Note: The CLI argument name in the code is `static_libs`, which is slightly different from the shell script's `-s, --static`. This README reflects the Rust implementation.)


*   `-a, --address-sanitizer`:
    Enable address sanitizer. (Currently acknowledged and ignored, for compatibility with the original script's options).

*   `-v, --verbose`:
    Enable verbose logging.

*   `-h, --help`:
    Print help information.

*   `-V, --version`:
    Print version information.

### Examples (Phase 1)

1.  **Clean existing build artifacts:**
    Assume `rocm-cmake` source is in `../rocm-cmake-src`.
    ```bash
    rocm-cmake-builder --rocm-cmake-root ../rocm-cmake-src --clean 
    ```
    Or, if `rocm-cmake` is in the current directory:
    ```bash
    rocm-cmake-builder --rocm-cmake-root . --clean
    ```

2.  **Build `rocm-cmake` (Debug, Shared Libraries):**
    ```bash
    rocm-cmake-builder --rocm-cmake-root /path/to/rocm-cmake-source -v
    ```

3.  **Build `rocm-cmake` (Release, Static Libraries) with a custom build directory:**
    ```bash
    rocm-cmake-builder       --rocm-cmake-root /path/to/rocm-cmake-source       --build-dir ./my-custom-build       --release       --static-libs       -v
    ```

## Build Instructions for `rocm-cmake-builder`

To build this utility itself:

1.  Ensure you have Rust and Cargo installed (see [rustup.rs](https://rustup.rs/)).
2.  Navigate to the `tools/rust-builders/rocm-cmake-builder` project directory.
3.  Run the build command:
    ```bash
    cargo build --release
    ```
    The compiled binary will be located at `target/release/rocm-cmake-builder`.

## Development Notes (Phase 1)

-   This phase focuses on the core `clean` and `build` (configure & install) logic.
-   Packaging (`cmake --build . --package`), specific package file copying, `--outdir` functionality, and `--wheel` creation are planned for Phase 2 and 3.
-   The CMake parameters used are currently minimal. Full replication of `rocm_cmake_params()` from `compute_utils.sh` is part of Phase 3.
```
