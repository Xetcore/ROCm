# ROCm SMI Lib Builder (Rust) - `rocm_smi_lib_builder`

This command-line tool is a Rust-based replacement for the original `build_rocm_smi_lib.sh` shell script. It is designed to build the `rocm-smi-lib` (ROCm System Management Interface library), a compiled C++ library.

## Prerequisites

1.  **Rust Toolchain**: Ensure you have Rust and Cargo installed. You can get them from [rustup.rs](https://rustup.rs/).
2.  **ROCm Environment**: A full ROCm installation is required to build `rocm-smi-lib`. This includes the ROCm toolchain (HIP, clang, etc.) and development libraries.
3.  **Standard Build Tools**: `cmake`, a C/C++ compiler (matching ROCm's, typically clang), and `make` (or `ninja`) must be available in your PATH. For package creation, `rpmbuild` (for RPMs) or `dpkg-dev` (for DEBs) are needed by CPack.
4.  **ROCm SMI Lib Sources**: The source code for `rocm-smi-lib` must be available.

## How to Build This Tool

Navigate to the directory containing this tool's `Cargo.toml` (e.g., `tools/rocm-build/rocm_smi_lib_builder/`) and run:

```bash
cargo build
```

For an optimized release build (recommended for general use):

```bash
cargo build --release
```

The executable will be located at `target/debug/rocm_smi_lib_builder` or `target/release/rocm_smi_lib_builder`.

## Usage

The basic command structure is:

```bash
rocm_smi_lib_builder [GLOBAL_OPTIONS] <SUBCOMMAND> [SUBCOMMAND_OPTIONS]
```

### Global Options

These options configure the build environment and paths:

*   `--rocm-path <PATH>`: Root path for the ROCm installation.
    *   Environment variable: `ROCM_PATH`
    *   Default: `/opt/rocm`
*   `--rocm-smi-lib-root <PATH>`: Path to the `rocm-smi-lib` source directory.
    *   Environment variable: `ROCM_SMI_LIB_ROOT`
    *   **Required.**
*   `--output-dir <PATH>`: Base output directory for all build artifacts and packages.
    *   Environment variable: `OUT_DIR`
    *   Default: `./output`
*   `--cpack-generator <GENERATOR_STRING>`: Types of packages CPack should generate (e.g., "DEB;RPM").
    *   Environment variable: `CPACKGEN`
    *   Default: `DEB;RPM`
*   `--rocm-libpatch-version <VERSION>`: ROCm patch version string (used in CMake package versioning).
    *   Environment variable: `ROCM_LIBPATCH_VERSION`
    *   Default: `0`
*   `--rocm-version <VERSION>`: ROCm version string (e.g., "5.7.0").
    *   Environment variable: `ROCM_VERSION`
    *   Default: `unknown`
*   `--proc <NUM_PROCS>`: Number of parallel processes for CMake/make.
    *   Environment variable: `PROC`
    *   Default: Number of logical CPUs.
*   `--rocm-lib-rpath <RPATH>`, `--rocm-exe-rpath <RPATH>`, `--rocm-asan-lib-rpath <RPATH>`: Specify RPATH settings (consult `compute_utils.sh` for typical values if unsure).
    *   Environment variables: `ROCM_LIB_RPATH`, `ROCM_EXE_RPATH`, `ROCM_ASAN_LIB_RPATH`
    *   Defaults: Empty string.
*   `--gfx-arch <ARCH_STRING>`: GPU architectures for compilation (e.g., "gfx900;gfx906").
    *   Environment variable: `GFX_ARCH`
    *   Default: A common set of recent GFX architectures.

### Subcommands

#### `build`

Builds and packages the `rocm-smi-lib`.

**Options:**

*   `-r, --release`: Configure for a release build (`RelWithDebInfo`). Default is a debug build.
*   `-a, --address-sanitizer`: Enable Address Sanitizer for the build. This sets appropriate environment variables for CMake and its child processes.
*   `-s, --static-libs`: Build static libraries (`.a`) instead of shared/dynamic ones (`.so`).
*   `-w, --wheel`: Attempt to build a Python wheel package (if `rocm-smi-lib`'s CMake is configured for it and `build_wheel` utilities are available).
*   `--32`: Perform a 32-bit build of the library.
*   `--package-type-copy <TYPE>`: Filters which package types are copied from the build directory (e.g., `deb`, `rpm`, `all`).

**Example:**

```bash
./target/release/rocm_smi_lib_builder --rocm-smi-lib-root /path/to/rocm-smi-lib/src build --release -a
```

#### `clean`

Removes the build artifacts, package directories, and installed files for `rocm-smi-lib`.

**Options:**
*   `--32`: Clean 32-bit version artifacts.

**Example:**

```bash
./target/release/rocm_smi_lib_builder clean
```

#### `outdir`

Prints the absolute path to the package output directory for a specific package type.

**Options:**

*   `--pkg-type <TYPE>`: The package type (`deb` or `rpm`). Default: `deb`.
*   `--32`: Specify for 32-bit version output directory.

**Example:**

```bash
./target/release/rocm_smi_lib_builder outdir --pkg-type rpm --32
```

## Environment Variables

The tool can also be configured using these environment variables (CLI options take precedence):

*   `ROCM_PATH`
*   `ROCM_SMI_LIB_ROOT`
*   `OUT_DIR`
*   `CPACKGEN`
*   `ROCM_LIBPATCH_VERSION`
*   `ROCM_VERSION`
*   `PROC`
*   `ROCM_LIB_RPATH`, `ROCM_EXE_RPATH`, `ROCM_ASAN_LIB_RPATH`
*   `GFX_ARCH`

This tool aims to replicate the core functionality of the original `build_rocm_smi_lib.sh` script, providing a more robust and maintainable solution in Rust. It includes logic to generate complex CMake parameters similar to those produced by `rocm_common_cmake_params` and `rocm_cmake_params` found in the original `compute_utils.sh`.
```
