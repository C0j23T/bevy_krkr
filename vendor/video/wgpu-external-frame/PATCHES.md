# Local video import patch

Source: [wgpu-external-frame 0.4.0](https://github.com/water-rs/wgpu-external-frame/tree/56e1c1c483614eed519652f26172a39370902180), Apache-2.0 OR MIT. Source, build script, tests and licenses are retained; the manifest comes from the published `Cargo.toml.orig`.

Android's external-format conversion also accepts Qualcomm's 8-bit NV12 UBWC format (`0x7fa30c06`). MediaCodec can return this format through a `YUV_420_888` ImageReader. Vulkan's AHardwareBuffer properties and sampler YCbCr conversion describe the actual layout; the buffer is never read or decompressed on the CPU. Unknown external formats and 10-bit formats still fail the format check.

The format and bit depth are defined in Qualcomm's [gr_camera_info.h](https://android.googlesource.com/platform/hardware/qcom/sm7250/display/+/refs/tags/aml_cbr_330911010/gralloc/gr_camera_info.h): `CAMERA_PIXEL_FORMAT_YCbCr_420_SP_UBWC`, distinct from `CAMERA_PIXEL_FORMAT_YCbCr_420_TP10_UBWC`.
