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
