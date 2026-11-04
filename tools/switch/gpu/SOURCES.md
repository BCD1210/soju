# Public source ledger

Only the public sources below were fetched. No emulator, firmware, game or keys are used.

NVIDIA open-gpu-doc pinned at `9fdf5c4062007929d9f4e6cbad9c9771fe61b880` (MIT; notice in LICENSE.nvidia).
Revision discovery: https://api.github.com/repos/NVIDIA/open-gpu-doc/commits/master
Tree discovery: https://api.github.com/repos/NVIDIA/open-gpu-doc/git/trees/9fdf5c4062007929d9f4e6cbad9c9771fe61b880?recursive=1

- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb197.h
  SHA-256 `175c50f29798b39c6858cafe31497e9e9798c70ca068a64856719003b5d088d7`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb197tex.h
  SHA-256 `d5aad7d7f339e09411b40a5e8b996613ae2c6d802187070bfa66a596513151bf`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/inline-to-memory/cla140.h
  SHA-256 `19a0d452641b2023fb208dd0599dec123006dd193955f53af6a86c73d9a98c31`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/twod/cl902d.h
  SHA-256 `8fa8bcb9f69077cc67f21ff68b17d16078e1cf200be6d728502bbba99ecb83bc`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/host/cla06f.h
  SHA-256 `6c5db0562c117a76f145acbe35a6bacafe612493424078439d1a83c627fbc0b8`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/dma-copy/clb0b5.h
  SHA-256 `169d958bfa379bfdf24c4656f02265ff5bcf22a2866724ee4299138d29df5332`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb097.h
  SHA-256 `61babb743bc5cd8666906575aa7eeda313e0ddb5179714b5a867facc8d59e100`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/clb097tex.h
  SHA-256 `4dfcee5f7b09d9c3efba8c3dd07efb43b37d41586ae3d9338165413b19fe363c`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/3d/cl9097tex.h
  SHA-256 `0b1bc445c0c46401769ec77fc8b97f9fae3c5644054ec7f847cf887d25141162`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/compute/clb1c0.h
  SHA-256 `6d612ede54e9615a30c0e5f03f2dcaed3039e062a7eb329a482782e9eb473942`.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-doc/9fdf5c4062007929d9f4e6cbad9c9771fe61b880/classes/compute/clb1c0qmd.h
  SHA-256 `0041719fd1619035e58eb882e04196743dc1475af65147ff851fdf9f81dd672c`.

B197 methods and field encodings: clb197.h; Maxwell TIC and TSC: clb197tex.h.
A140 inline upload: cla140.h; 902D copy: cl902d.h. Other headers were downloaded for comparison only.
Per-method links: REGISTERS.md. Original shader fixtures use the ISA sources already recorded in ../shader/SOURCES.md.

- https://developer.apple.com/documentation/metal
  Metal device, descriptors, render encoders and readback; locally installed SDK declarations are the API authority.

- https://raw.githubusercontent.com/envytools/envytools/f102b82381f3f11cee113d16374c87091db039d9/rnndb/graph/gf100_shaders.xml
  Export map PSIZE at 0x6c and CLIP_DISTANCE at 0x2c0–0x2dc; same pinned
  public SPH source as the shader crate. Apple MSL specification (in the shader
  ledger), Table 5.4, supplies Metal point_size and clip_distance attributes.
  Metal SDK MTLTexture.h supplies texture-view swizzles; MTLRenderCommandEncoder.h
  supplies visibility counting, depth bias, viewport, and vertex step functions.

The installed Apple SDK's `MTLBlitCommandEncoder.h` supplies buffer/texture copies
and the depth/stencil plane options used for attachment import and readback.
