# ROCm CMake Builder (Rust) - `rocm_cmake_builder`

This command-line tool is a Rust-based replacement for the original `build_rocm-cmake.sh` shell script. It is designed to build the `rocm-cmake` component, which provides CMake modules for ROCm projects.

## Prerequisites

1.  **Rust Toolchain**: Ensure you have Rust and Cargo installed. You can get them from [rustup.rs](https://rustup.rs/).
2.  **ROCm Environment**: A ROCm installation is required to actually build the `rocm-cmake` component, as this tool invokes `cmake` which will search for ROCm paths and libraries. This tool itself can be compiled without a ROCm installation, but it won't be able to perform a build.
3.  **Standard Build Tools**: `cmake`, a C/C++ compiler, and `make` (or `ninja`) must be available in your PATH. For package creation, `rpmbuild` (for RPMs) or `dpkg-dev` (for DEBs) might be needed by CPack (invoked via CMake).

## How to Build This Tool

Navigate to the directory containing this tool's `Cargo.toml` (e.g., `tools/rocm-build/rust_rocm_cmake_builder/`) and run:

```bash
cargo build
```

For an optimized release build (recommended for general use):

```bash
cargo build --release
```

The executable will be located at `target/debug/rocm_cmake_builder` or `target/release/rocm_cmake_builder`.

## Usage

The basic command structure is:

```bash
rocm_cmake_builder [GLOBAL_OPTIONS] <SUBCOMMAND> [SUBCOMMAND_OPTIONS]
```

### Global Options

These options can influence multiple subcommands or the overall setup:

*   `--rocm-path <PATH>`: Root path for the ROCm installation.
    *   Environment variable: `ROCM_PATH`
    *   Default: `/opt/rocm`
*   `--rocm-cmake-root <PATH>`: Path to the `rocm-cmake` source directory (the directory containing the top-level `CMakeLists.txt` for `rocm-cmake`).
    *   Environment variable: `ROCM_CMAKE_ROOT`
    *   Default: `./cmake` (assumes running from the ROCm repository root)
*   `--cpack-generator <GENERATOR_STRING>`: Types of packages CPack should generate (e.g., "DEB;RPM", "DEB").
    *   Environment variable: `CPACKGEN`
    *   Default: `DEB;RPM`
*   `--rocm-patch-version <VERSION>`: ROCm patch version string.
    *   Environment variable: `ROCM_LIBPATCH_VERSION`
    *   Default: `0`
*   `--output-dir <PATH>`: Base output directory for all build artifacts and packages.
    *   Environment variable: `OUT_DIR`
    *   Default: `./output`

### Subcommands

#### `build`

Builds the `rocm-cmake` component.

**Options:**

*   `-r, --release`: Configure for a release build (sets `CMAKE_BUILD_TYPE=Release`). Default is a debug build.
*   `-a, --address-sanitizer`: Acknowledged but ignored for `rocm-cmake` builds.
*   `-s, --static-libs`: Corresponds to `BUILD_SHARED_LIBS=OFF` in CMake. Default is `ON` (shared libs).
*   `-w, --wheel`: Acknowledged, but wheel packaging is likely not applicable to `rocm-cmake` and is currently skipped.
*   `--package-type-copy <TYPE>`: Filters which package types are copied from the build directory (e.g., `deb`, `rpm`, `all`). This depends on what CPack generates via `--cpack-generator`.
    *   Default: `all`

**Example:**

```bash
./target/release/rocm_cmake_builder --output-dir ./my_build_output build --release --package-type-copy deb
```

#### `clean`

Removes the build and package directories for `rocm-cmake` within the specified `--output-dir`.

**Example:**

```bash
./target/release/rocm_cmake_builder --output-dir ./my_build_output clean
```

#### `outdir`

Prints the absolute path to the package output directory for a specific package type.

**Options:**

*   `--pkg-type <TYPE>`: The package type (`deb` or `rpm`).
    *   Default: `deb`

**Example:**

```bash
./target/release/rocm_cmake_builder --output-dir ./my_build_output outdir --pkg-type rpm
```

## Environment Variables

The tool can also be configured using these environment variables (CLI options take precedence):

*   `ROCM_PATH`
*   `ROCM_CMAKE_ROOT`
*   `CPACKGEN`
*   `ROCM_LIBPATCH_VERSION`
*   `OUT_DIR`

This tool aims to replicate the core functionality of the original `build_rocm-cmake.sh` script with the benefits of a Rust application.
```
