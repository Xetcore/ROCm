# rocminfo Builder (Rust) - `rocminfo-builder`

`rocminfo-builder` is a command-line utility written in Rust designed to replace the functionality of the `build_rocminfo.sh` shell script. It handles the building, packaging, and installation of the `rocminfo` utility.

**Note:** This utility is currently in Phase 3 of development. It supports cleaning, building (configure, build, install), packaging (deb/rpm), Python wheel creation (if `setup.py` exists), and querying output directories. Full replication of `rocm_cmake_params()` and `rocm_common_cmake_params()` from `compute_utils.sh` is still pending.

## Features

-   Cleaning of build directories, configured package directories, and the installed `rocminfo` binary.
-   Configuration of `rocminfo` via CMake, with support for various build options.
-   Building and installing `rocminfo` using CMake, with parallel job execution.
-   Packaging `rocminfo` into DEB and RPM packages using CPack (via CMake).
-   Copying specified package types (deb, rpm, or all) to structured package directories.
-   Building Python wheel packages via `python setup.py bdist_wheel` (if `setup.py` is present in source root), with output to `<build-dir>/wheelhouse/`.
-   Querying output paths for specific package types (`--outdir-target`).
-   Support for Address Sanitizer (ASan) via CMake flags (environment variable setup for ASan runtime is noted as pending full `compute_utils.sh` logic).
-   Passing GPU list and CPack versioning information to CMake.
-   CLI arguments for specifying source/build/package/install paths, build type (debug/release), library type (static/shared), package filtering, verbosity, and job control.
-   Improved error reporting by showing output from failed external commands (CMake, Python).

## Usage

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
    Optional: Specify the root directory for packages.
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
    Creates a Python wheel package using `python setup.py bdist_wheel` (if `setup.py` exists in `--source-root`). Output is to `<build-dir>/wheelhouse/`.

*   `--gpu-list <LIST>`:
    Optional: Comma-separated list of GPUs to target (passed to CMake as `-DGPU_LIST=<LIST>`).

*   `--jobs <N>`:
    Optional: Number of parallel jobs for the CMake build step (`--parallel N`).

*   `--package-type <TYPE>`:
    Optional: Specify packaging format to copy after CPack (e.g., "deb", "rpm", "all").
    (Default: "all")

*   `--outdir-target <PKG_TYPE>`:
    Optional: Print path of output directory for specified package type (deb, rpm) and exit.
    Example: `--outdir-target deb`

*   `-v, --verbose`:
    Enable verbose logging.

*   `-h, --help`: Print help information.
*   `-V, --version`: Print version information.

### Examples

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

4.  **Build, package DEBs and RPMs, and create a wheel (if setup.py exists):**
    ```bash
    rocminfo-builder           --source-root /path/to/rocminfo-source           --package-type all           --wheel           -v
    ```

5.  **Get the output directory for DEB packages:**
    ```bash
    rocminfo-builder           --source-root /path/to/rocminfo-source           --outdir-target deb
    ```
    Output might be: `/path/to/rocminfo-source/dist/deb/rocminfo` (assuming default package_root)

6.  **Build with Address Sanitizer and specific GPU list:**
    ```bash
    rocminfo-builder           --source-root /path/to/rocminfo-source           --address-sanitizer           --gpu-list "gfx900;gfx906"           -v
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

## Development Notes

-   The CMake parameters generated by `get_rocm_cmake_params()` and `get_rocm_common_cmake_params()` are currently placeholders (empty). Full replication of the original script's logic from `compute_utils.sh` is a key remaining item. This might affect build reproducibility if `rocminfo` relies on specific parameters from these functions.
-   Address Sanitizer (`--address-sanitizer`):
    -   Currently sets `-DENABLE_ADDRESS_SANITIZER=ON` (assumed flag name) for CMake.
    -   Full replication of `compute_utils.sh:set_asan_env_vars` (which would set runtime environment variables like `ASAN_OPTIONS` for child processes like CTest) is pending.
-   Python wheel support (`--wheel`):
    -   Invokes `python setup.py bdist_wheel --dist-dir <build-dir>/wheelhouse/`.
    -   Requires `python3` (or `python`) and `setuptools` in PATH.
    -   Checks for `setup.py` in `--source-root`; skips wheel build if not found (with a verbose message).
    -   The original `build_wheel` from `compute_utils.sh` might have additional logic for copying the wheel to a final packaging location; this is not yet implemented by the Rust tool.
-   Error handling shows `stdout`/`stderr` from failed external commands.
-   The tool relies on `cmake`, `python3` (or `python` if for wheel), and standard build tools to be in the system PATH.
```
