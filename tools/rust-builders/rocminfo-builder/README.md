# rocminfo Builder (Rust) - `rocminfo-builder`

`rocminfo-builder` is a command-line utility written in Rust designed to replace the functionality of the `build_rocminfo.sh` shell script. It handles the building, (eventual) packaging, and installation of the `rocminfo` utility.

**Note:** This utility is currently in Phase 1 of development. It supports cleaning and basic build (configure and install) operations. Packaging, wheel creation, and other advanced features will be added in subsequent phases.

## Phase 1 Features

-   Cleaning of build directories and the installed `rocminfo` binary.
-   Configuration of `rocminfo` via CMake.
-   Building and installing `rocminfo` using CMake.
-   CLI arguments for specifying paths, build type (debug/release), library type (static/shared), ASan, GPU list, and job parallelism.

## Usage (Phase 1)

### Command-Line Arguments

```
rocminfo-builder [OPTIONS] --source-root <PATH>
```

**Required:**

*   `--source-root <PATH>`: Path to the `rocminfo` source directory.

**Options:**

*   `--install-prefix <PATH>`:
    Optional: Install prefix for `rocminfo`.
    Defaults to `<package_root>/rocm`. `package_root` itself defaults to `<source_root>/dist`.
    So, default install prefix is effectively `<source_root>/dist/rocm`.

*   `--build-dir <PATH>`:
    Optional: Specify the build directory.
    Defaults to `<source-root>/build/rocminfo-builder`.

*   `--package-root <PATH>`:
    Optional: Specify the root directory for future packages (not fully used for output in Phase 1, but directories might be created/cleaned).
    Defaults to `<source_root>/dist`.

*   `--rocm-libpatch-version <VERSION>`:
    Optional: Version patch number for CPack (used in CMake's CPack settings).
    (Default: "0")

*   `-c, --clean`:
    Clean output: removes build directory, configured package directories, and the installed `rocminfo` binary from `<install-prefix>/bin/rocminfo`. If this flag is present, build actions are skipped.

*   `-r, --release`:
    Build a release version of `rocminfo`. Default is debug.
    (Sets `CMAKE_BUILD_TYPE=RelWithDebInfo` and `ROCRTST_BLD_TYPE=rel`).

*   `-s, --static-libs`:
    Configure `rocminfo` to build static libraries (sets `BUILD_SHARED_LIBS=OFF`). Default is shared libraries.

*   `-a, --address-sanitizer`:
    Enable Address Sanitizer. This will set `-DENABLE_ADDRESS_SANITIZER=ON` for CMake (assumption, actual flag might differ based on `rocminfo`'s CMake) and is intended to also set necessary environment variables for the build process.

*   `-w, --wheel`:
    Creates a Python wheel package (if applicable for `rocminfo`). (Functionality to be fully implemented in Phase 3).

*   `--gpu-list <LIST>`:
    Optional: Comma-separated list of GPUs to target (passed to CMake as `-DGPU_LIST=<LIST>`).

*   `--jobs <N>`:
    Optional: Number of parallel jobs for the CMake build step (`--parallel N`).

*   `-v, --verbose`:
    Enable verbose logging.

*   `-h, --help`: Print help information.
*   `-V, --version`: Print version information.

### Examples (Phase 1)

1.  **Clean existing build artifacts and installed binary:**
    (Assuming `rocminfo` source is in `./rocm/rocminfo-src` and default install prefix is used)
    ```bash
    rocminfo-builder --source-root ./rocm/rocminfo-src --clean 
    ```

2.  **Build `rocminfo` (Debug, Shared Libraries, default install prefix):**
    ```bash
    rocminfo-builder --source-root /path/to/rocminfo-source -v
    ```

3.  **Build `rocminfo` (Release, Static Libraries) with a custom install prefix and 4 jobs:**
    ```bash
    rocminfo-builder       --source-root /path/to/rocminfo-source       --install-prefix ./my-install-area       --release       --static-libs       --jobs 4       -v
    ```

## Build Instructions for `rocminfo-builder`

To build this utility itself:

1.  Ensure you have Rust and Cargo installed (see [rustup.rs](https://rustup.rs/)).
2.  Navigate to the `tools/rust-builders/rocminfo-builder` project directory.
3.  Run the build command:
    ```bash
    cargo build --release
    ```
    The compiled binary will be located at `target/release/rocminfo-builder`.

## Development Notes (Phase 1)

-   This phase focuses on core `clean` and `build` (configure & install) logic.
-   Packaging (`cmake --build . --target package`), specific package file copying, `--outdir-target` functionality, and full `--wheel` creation are planned for later phases.
-   The CMake parameters from `get_rocm_cmake_params()` and `get_rocm_common_cmake_params()` are currently placeholders (empty). Full replication of the original script's logic from `compute_utils.sh` is a key remaining item.
-   ASan support currently sets a CMake flag; full environment variable setup (like `ASAN_OPTIONS`) via `compute_utils.sh`'s `set_asan_env_vars` needs to be replicated if specific options are required for child processes.
```
