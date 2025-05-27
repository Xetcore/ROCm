## Conceptual Changes for Rust CLI Integration into ROCm Build System

This document outlines the conceptual modifications required to integrate the newly developed Rust-based Command Line Interface (CLI) tools into the existing ROCm build system. The primary files affected are `tools/rocm-build/ROCm.mk` and `tools/rocm-build/runner`.

### 1. Modifications to `tools/rocm-build/ROCm.mk`

The Makefile `ROCm.mk` will be updated to manage the compilation of the Rust CLI tools themselves.

*   **New Variables:**
    *   `RUST_CLI_PROJECTS`: A list of names for the Rust CLI projects (e.g., `rocm_cmake_builder half_builder rocm_smi_lib_builder`). This variable will allow easy iteration over all Rust projects.
    *   `RUST_CLI_BIN_DIR`: The designated directory where the compiled Rust CLI binaries will be stored (e.g., `${ROCM_PATH}/libexec/rocm-build` or a subdirectory within `${OUT_DIR}`). This centralizes the location of the tools.
    *   `RUST_CLI_SRC_BASE_DIR`: The base directory containing the Rust projects (e.g., `$(CURDIR)/tools/rocm-build`).

*   **New Target: `build_rust_clis`**
    *   **Purpose**: This target is responsible for compiling all Rust CLI tools specified in `RUST_CLI_PROJECTS`.
    *   **Mechanism**:
        1.  It will iterate through each project name in the `RUST_CLI_PROJECTS` list.
        2.  For each project, it will change to the project's source directory (e.g., `$(RUST_CLI_SRC_BASE_DIR)/<project_name>/`).
        3.  It will then invoke `cargo build --release` to compile the Rust project in release mode.
        4.  Upon successful compilation, the resulting binary (e.g., `target/release/<project_name>`) will be copied to the `$(RUST_CLI_BIN_DIR)` directory. The copied binary might be renamed for consistency if needed (e.g., from `rocm_cmake_builder` to `rocm-cmake-builder-rust`).
        5.  Ensure `$(RUST_CLI_BIN_DIR)` is created if it doesn't exist.
    *   **Output**: The compiled Rust CLI binaries will be available in `$(RUST_CLI_BIN_DIR)`.

*   **Dependency Integration:**
    *   The `build_rust_clis` target will be added as a dependency to an early, fundamental target in the build system, such as the one responsible for creating `${OUT_DIR}/logs` or another prerequisite target that runs before any component builds. This ensures that the Rust CLI tools are compiled and available before the `runner` script attempts to use them.
    *   Example: `${OUT_DIR}/logs: build_rust_clis`

### 2. Modifications to `tools/rocm-build/runner`

The `runner` script, which is the central dispatcher for building individual ROCm components, will be modified to preferentially use the Rust CLI tools if available, falling back to the original shell scripts otherwise.

*   **Determining Rust CLI Path:**
    *   At the beginning of its execution for a specific component (e.g., `rocm-cmake`, `half`, `rocm-smi-lib`), the `runner` will construct the expected path to the corresponding Rust CLI executable.
    *   This path will be based on the `RUST_CLI_BIN_DIR` (which could be passed as an argument to `runner` or derived from `ROCM_PATH`) and the component's specific Rust CLI tool name (e.g., `rocm-cmake-builder-rust`, `half-builder-rust`).

*   **Modified Logic for 'clean' and 'build' Phases:**

    For both the `clean_component` and `build_component` functions/phases within `runner`:

    1.  **Check for Rust CLI Executable**: The script will check if the determined Rust CLI executable exists and is executable.
    2.  **Rust CLI Execution (If Found)**:
        *   If the Rust CLI executable exists, the `runner` will construct and execute the command for the Rust tool.
        *   **Command Construction**:
            *   For `clean`: `"${RUST_CLI_EXECUTABLE}" clean [common_flags_for_rust_cli]`
            *   For `build`: `"${RUST_CLI_EXECUTABLE}" build [build_flags_for_rust_cli] [component_specific_flags_for_rust_cli]`
        *   **Flag Translation**: The `runner` script will need to translate existing Makefile/shell script flags to the argument format expected by the Rust CLI tools.
            *   Example: A Makefile flag `-r` (for release) might translate to `--release` for the Rust CLI.
            *   Example: `-s` (for static libs) might translate to `--static-libs`.
            *   This involves mapping existing variables like `BUILD_RELEASE`, `BUILD_STATIC_LIBS`, `ENABLE_ASAN`, etc., to their corresponding Rust CLI options.
            *   Global configuration options (like `ROCM_PATH`, `OUT_DIR`, `CPACKGEN`, `ROCM_LIBPATCH_VERSION`, source directories) will be passed as arguments (e.g., `--rocm-path "${ROCM_PATH}"`, `--output-dir "${OUT_DIR}"`, `--<component>-src-dir "${COMPONENT_SRC_PATH}"`) or via environment variables if the Rust CLIs are designed to consume them directly (CLI arguments should take precedence).
        *   The `runner` will then execute this constructed command. The exit code of the Rust CLI will determine the success or failure of the step.
    3.  **Fallback to Shell Script (If Not Found)**:
        *   If the Rust CLI executable does not exist, the `runner` will log a message indicating the fallback and proceed to call the original shell script for the component (e.g., `build_rocm-cmake.sh`, `build_half.sh`) with the existing flag and environment variable setup.
        *   This ensures backward compatibility and allows for incremental rollout of the Rust tools.

*   **Maintenance of Existing Logic:**
    *   **`envsetup.sh` Sourcing**: The sourcing of `envsetup.sh` at the beginning of the `runner` (or relevant component build function) will be maintained to ensure the necessary environment variables (like `CXX`, `CC`, ROCm-specific paths, etc.) are set up for both Rust CLI (which might pass them to CMake) and shell script execution.
    *   **`post_inst_pkg.sh` Calls**: The invocation of `post_inst_pkg.sh` after the build and packaging steps will continue as is, as this handles tasks outside the direct scope of component compilation (e.g., creating symlinks, documentation).
    *   **Environment Variables for Global Settings**: The `runner` will continue to rely on and propagate global environment variables like `ROCM_PATH`, `OUT_DIR`, `GFX_ARCH`, etc. The Rust CLIs are designed to accept these as command-line arguments, which the `runner` will provide.

By implementing these changes, the ROCm build system can leverage the new Rust CLI tools for improved maintainability, performance, and robustness, while retaining the ability to fall back to the existing shell scripts during the transition phase or for components not yet migrated.
```
