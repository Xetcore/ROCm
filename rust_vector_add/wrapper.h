#include <hip/hip_runtime_api.h>
#include <hip/hiprtc.h> // For hipModule_t and related types if directly used, though hip_runtime_api.h should cover most needs for runtime functions.

// Explicitly declare functions if not covered or for clarity, though hip_runtime_api.h should declare them.
// For example, if a specific function was missing from standard headers or part of a specific HIP version:
// extern hipError_t hipModuleLoadData(hipModule_t* module, const void* image);
// extern hipError_t hipModuleGetFunction(hipFunction_t* function, hipModule_t module, const char* kname);
// extern hipError_t hipModuleLaunchKernel(hipFunction_t f, unsigned int gridDimX, unsigned int gridDimY, unsigned int gridDimZ, unsigned int blockDimX, unsigned int blockDimY, unsigned int blockDimZ, unsigned int sharedMemBytes, hipStream_t stream, void** kernelParams, void** extra);
// extern hipError_t hipModuleUnload(hipModule_t module);
