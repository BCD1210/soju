# GPU integration public references

The runner uses installed libnx headers and the public references below. These
references describe wire layouts and instruction fields; guest examples come
from the installed devkitPro example package. No emulator sources are used.

- https://switchbrew.org/wiki/NV_services
  NVMap, address-space mapping and GPFIFO ioctl layouts.
- https://github.com/switchbrew/libnx/blob/master/nx/include/switch/nvidia/ioctl.h
  Installed header: MapBufferEx FixedOffset, IsCacheable and Modify flags.
- https://github.com/switchbrew/libnx/blob/master/nx/include/switch/nvidia/gpu_channel.h
  Installed header: GPFIFO entry flags and per-channel fence accumulation.
- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/envydis/macro.c
  MME result destinations, method selector fields and branch/exit delay semantics.
  Hand-encoded tests exercise parameter/send ordering and conditional exit.
- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/rnndb/graph/gf100_3d.xml
  CSAA_ENABLE, MULTISAMPLE_MODE, LINKED_TSC and window clip register descriptions.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb197.h
  MME upload/call registers, alpha controls and interface version checks.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/twod/cl902d.h
  902D initialization: render enable, clipping, color key, beta values, processing
  clusters, corral size, overlap and sample mode. Actual copies continue through
  the GPU crate's bounded pitch RGBA8 SRCCOPY implementation. Unsupported clip,
  color key, zeta, conditional rendering and bilinear modes fail before copy.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/host/cla06f.h
  GPFIFO entry address/length, privilege, level and synchronization fields.

The Metal bridge remains synchronous. It counts executed increment methods,
accepts zero-increment initialization, and publishes completion only after guest
memory writes commit. This is not an asynchronous GPU scheduler. Unsupported
methods and instruction combinations remain explicit errors.
