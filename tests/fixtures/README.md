`video.mp4` is generated test content: 0.5 seconds of red, 0.5 seconds of
green, and a 440 Hz sine wave. It contains H.264 video and AAC audio.

```sh
ffmpeg -f lavfi -i color=c=red:s=64x64:r=10:d=0.5 \
  -f lavfi -i color=c=lime:s=64x64:r=10:d=0.5 \
  -f lavfi -i sine=frequency=440:sample_rate=48000:duration=1 \
  -filter_complex '[0:v][1:v]concat=n=2:v=1:a=0[v]' \
  -map '[v]' -map 2:a -c:v libx264 -pix_fmt yuv420p -c:a aac \
  -shortest -y video.mp4
```

`not-video.txt` exercises invalid media input.

`multistream.mp4` contains red 64×64 and blue 32×32 H.264 streams and
440 Hz / 880 Hz AAC audio streams, each one second long. Generate with:

```sh
ffmpeg -f lavfi -i color=c=red:s=64x64:r=10:d=1 \
  -f lavfi -i color=c=blue:s=32x32:r=10:d=1 \
  -f lavfi -i sine=frequency=440:sample_rate=48000:duration=1 \
  -f lavfi -i sine=frequency=880:sample_rate=48000:duration=1 \
  -map 0:v -map 1:v -map 2:a -map 3:a -c:v libx264 -pix_fmt yuv420p \
  -c:a aac -shortest -y multistream.mp4
```

`layer-ex-image.rgba` contains reference output from the pinned upstream
[wamsoft/layerExImage](https://github.com/wamsoft/layerExImage) at commit
`3aff625dba5c545ba722bb43b0a5ca1ad4f4336d`. Regenerate it from an external checkout
with `python3 tests/fixtures/generate_layer_ex_image.py /path/to/layerExImage`.
The generator verifies the upstream source hashes; running tests only needs the
committed RGBA fixture. Cases are 16×16 light,
colorize (partial blend), modulate, Gaussian blur; 3×2 Gaussian blur; and
16×16 colorize (full blend), with varying RGB and Alpha. The last two cases
also exercise a negative CPU pitch. The test compares GPU output with these
independent CPU reference pixels.

`layer-alpha.mkv` contains a red left half and a gray right half. It checks
LayerExMovie's packed mask layout separately from native video Alpha.

```sh
ffmpeg -f lavfi -i color=c=red:s=64x64:r=10:d=0.4 \
  -f lavfi -i color=c=gray:s=64x64:r=10:d=0.4 \
  -filter_complex '[0:v][1:v]hstack[v]' -map '[v]' -c:v ffv1 -y layer-alpha.mkv
```

`pack_xp3.py SOURCE_DIRECTORY OUTPUT.xp3` packages an export into an unencrypted
XP3 with compressed segments and index. The NVL acceptance test accepts either
the source directory or this archive via `NVL_MAKER_DATA`, and copies its input
to an isolated temporary project before writing any savedata.
