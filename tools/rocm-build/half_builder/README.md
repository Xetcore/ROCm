# Half Builder (Rust) - `half_builder`

This command-line tool is a Rust-based replacement for the original `build_half.sh` shell script. It is designed to build the `half` library, a header-only library providing half-precision floating-point types.

## Prerequisites

1.  **Rust Toolchain**: Ensure you have Rust and Cargo installed. You can get them from [rustup.rs](https://rustup.rs/).
2.  **ROCm Environment**: A ROCm installation is required for the CMake build process to correctly find ROCm paths for installation of the `half` headers. This tool itself can be compiled without a ROCm installation, but it won't be able to perform a full build and install into a ROCm tree.
3.  **Standard Build Tools**: `cmake`, a C/C++ compiler (though not strictly for compiling `half` itself, CMake might require it), and `make` (or `ninja`) must be available in your PATH. For package creation, `rpmbuild` (for RPMs) or `dpkg-dev` (for DEBs) might be needed by CPack (invoked via CMake).
4.  **Half Library Sources**: The source code for the `half` library must be available.

## How to Build This Tool

Navigate to the directory containing this tool's `Cargo.toml` (e.g., `tools/rocm-build/half_builder/`) and run:

```bash
cargo build
```

For an optimized release build (recommended for general use):

```bash
cargo build --release
```

The executable will be located at `target/debug/half_builder` or `target/release/half_builder`.

## Usage

The basic command structure is:

```bash
half_builder [GLOBAL_OPTIONS] <SUBCOMMAND>
```

### Global Options

These options can influence multiple subcommands or the overall setup:

*   `--rocm-path <PATH>`: Root path for the ROCm installation (e.g., where `half` headers will be installed).
    *   Environment variable: `ROCM_PATH`
    *   Default: `/opt/rocm`
*   `--output-dir <PATH>`: Base output directory for all build artifacts and packages.
    *   Environment variable: `OUT_DIR`
    *   Default: `./output`
*   `--half-src-dir <PATH>`: Path to the `half` library source directory.
    *   Environment variable: `HALF_SRC_DIR`
    *   Default: `./half` (assumes running from a directory containing `half` sources, or a symlink)
*   `--cpack-generator <GENERATOR_STRING>`: Types of packages CPack should generate (e.g., "DEB;RPM", "DEB").
    *   Environment variable: `CPACKGEN`
    *   Default: `DEB;RPM`
*   `--proc <NUM_PROCS>`: Number of parallel processes for the CMake build step (e.g., `cmake --build . -- -j<NUM_PROCS>`).
    *   Environment variable: `PROC`
    *   Default: Number of logical CPUs on the system.
*   `--enable-static-builds`: If specified, the tool will print a message and exit, as static builds are not applicable to the `half` library via this script.
    *   Environment variable: `ENABLE_STATIC_BUILDS`
*   `--enable-address-sanitizer`: If specified, ASAN-related environment variable setup is conceptually acknowledged (as in the original script), but it does not affect the `half` library's CMake configuration for packaging.
    *   Environment variable: `ENABLE_ADDRESS_SANITIZER`

### Subcommands

#### `build`

Builds and packages the `half` library.

**Example:**

```bash
./target/release/half_builder --half-src-dir /path/to/half/sources build
```

#### `clean`

Removes the build and package directories for `half` within the specified `--output-dir`.

**Example:**

```bash
./target/release/half_builder --output-dir ./my_build_output clean
```

#### `outdir`

Prints the absolute path to the package output directory for a specific package type.

**Options:**

*   `--pkg-type <TYPE>`: The package type (`deb` or `rpm`).
    *   Default: `deb`

**Example:**

```bash
./target/release/half_builder --output-dir ./my_build_output outdir --pkg-type rpm
```

## Environment Variables

The tool can also be configured using these environment variables (CLI options take precedence):

*   `ROCM_PATH`
*   `OUT_DIR`
*   `HALF_SRC_DIR`
*   `CPACKGEN`
*   `PROC`
*   `ENABLE_STATIC_BUILDS`
*   `ENABLE_ADDRESS_SANITIZER`

This tool aims to replicate the core functionality of the original `build_half.sh` script for the header-only `half` library.
```
