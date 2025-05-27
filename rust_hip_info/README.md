# Rust HIP Info (`rust_hip_info`)

This command-line tool is a Rust application that replicates the functionality of a typical `hipInfo` or device query utility. It uses Rust's Foreign Function Interface (FFI) to call functions from the existing HIP (Heterogeneous-compute Interface for Portability) runtime library (`libamdhip64.so`) to discover and display information about available AMD GPUs.

The FFI bindings are generated using `bindgen`.

## Prerequisites

1.  **Rust Toolchain**: Ensure you have Rust and Cargo installed. You can get them from [rustup.rs](https://rustup.rs/).
2.  **HIP SDK (ROCm)**: A ROCm installation including the HIP SDK is required. This provides:
    *   The HIP header files (e.g., `hip/hip_runtime_api.h`) needed by `bindgen` during the build of this tool.
    *   The HIP runtime library (`libamdhip64.so`) which this tool links against and calls at runtime.
    *   The `ROCM_PATH` environment variable should ideally be set (e.g., to `/opt/rocm`). If not, `bindgen` might require explicit include paths.
3.  **Clang**: `bindgen` requires `libclang` to parse C/C++ headers. Ensure Clang is installed. It's usually part of a ROCm installation or can be installed separately.
4.  **Standard Build Tools**: A C compiler (like `gcc` or `clang`) might be needed by Cargo for building some crate dependencies or the `build.rs` script itself.

## How to Build This Tool

Navigate to the directory containing this tool's `Cargo.toml` and run:

```bash
cargo build
```

For an optimized release build:

```bash
cargo build --release
```

During the first build, the `build.rs` script will invoke `bindgen` to generate Rust FFI bindings from the HIP SDK's header files. The `ROCM_PATH` environment variable should be set to help `bindgen` locate these headers. If `ROCM_PATH` is not set, you might need to modify `build.rs` to provide the correct include path to `hip/hip_runtime_api.h`.

The executable will be located at `target/debug/rust_hip_info` or `target/release/rust_hip_info`.

## How to Run

After building, you can run the tool directly:

```bash
./target/release/rust_hip_info
```

## Expected Output

The tool will print information about each detected HIP-compatible GPU, including (but not limited to):

*   Number of HIP devices found.
*   For each device:
    *   Device ID
    *   Device Name
    *   Total Global Memory (e.g., in MB)
    *   Compute Capability (Major.Minor)
    *   Core Clock Rate
    *   Number of Multiprocessors
    *   Other properties available from `hipDeviceProp_t`.
*   Optionally, HIP Runtime and Driver versions.

## Example Output Snippet

```
Number of HIP devices: 1

Device 0:
  Name:                     AMD Radeon RX Graphics
  Total global memory:      8172 MB
  Compute capability:       11.0
  Core clock rate:          2400000 kHz
  Multiprocessor count:     80
  ... (other properties) ...

HIP Runtime Version: 50731000
HIP Driver Version:  50700
```

This tool demonstrates how to use Rust FFI with `bindgen` to interface with the existing HIP C++ runtime and perform basic device query operations.
```
