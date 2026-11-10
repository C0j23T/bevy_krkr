use std::io::Cursor;

use ffmpeg_next::{self as ffmpeg, codec, ffi, format, frame, media, software};

pub struct Decoder {
    input: format::context::Input,
    video: ffmpeg::decoder::Video,
    stream: usize,
    time_base: f64,
    start: f64,
    next_time: f64,
    eof: bool,
    draining: bool,
    audio: Option<AudioDecoder>,
    pub audio_frames: Vec<AudioFrame>,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub fps: f64,
    pub audio_streams: u32,
    pub backend: String,
    audio_indices: Vec<usize>,
    video_indices: Vec<usize>,
    hardware: bool,
}

pub struct AudioFrame {
    pub time: f64,
    pub samples: Vec<f32>,
}

struct AudioDecoder {
    decoder: ffmpeg::decoder::Audio,
    stream: usize,
    time_base: f64,
    resampler: software::resampling::context::Context,
}

pub struct Frame {
    pub pixels: frame::Video,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub time: f64,
}

impl Decoder {
    pub fn open(bytes: Vec<u8>, hardware: bool) -> Result<Self, String> {
        ffmpeg::init().map_err(|e| e.to_string())?;
        let io = format::context::StreamIo::from_read_seek(Cursor::new(bytes))
            .map_err(|e| e.to_string())?;
        let input = format::input_from_stream(io, None, None).map_err(|e| e.to_string())?;
        let stream = input
            .streams()
            .best(media::Type::Video)
            .ok_or("no video stream")?;
        let index = stream.index();
        let time_base = f64::from(stream.time_base());
        let start = if stream.start_time() == ffi::AV_NOPTS_VALUE {
            0.0
        } else {
            stream.start_time() as f64 * time_base
        };
        let fps = f64::from(stream.avg_frame_rate());
        let duration = if stream.duration() > 0 {
            stream.duration() as f64 * time_base
        } else {
            input.duration().max(0) as f64 / f64::from(ffi::AV_TIME_BASE)
        };
        let context = codec::context::Context::from_parameters(stream.parameters())
            .map_err(|e| e.to_string())?;
        let (video, backend) = if hardware {
            hardware_decoder(context)?
        } else {
            (
                context.decoder().video().map_err(|e| e.to_string())?,
                "software".into(),
            )
        };
        let audio_indices: Vec<_> = input
            .streams()
            .filter(|s| s.parameters().medium() == media::Type::Audio)
            .map(|s| s.index())
            .collect();
        let video_indices: Vec<_> = input
            .streams()
            .filter(|s| s.parameters().medium() == media::Type::Video)
            .map(|s| s.index())
            .collect();
        let audio = input
            .streams()
            .best(media::Type::Audio)
            .map(AudioDecoder::open)
            .transpose()?;
        let (width, height) = (video.width(), video.height());
        if width == 0 || height == 0 {
            return Err("invalid video dimensions".into());
        }
        let audio_streams = audio_indices.len() as u32;
        Ok(Self {
            input,
            video,
            stream: index,
            time_base,
            start,
            next_time: 0.0,
            eof: false,
            draining: false,
            audio,
            audio_frames: Vec::new(),
            width,
            height,
            duration,
            fps: if fps.is_finite() && fps > 0.0 {
                fps
            } else {
                30.0
            },
            backend,
            audio_streams,
            audio_indices,
            video_indices,
            hardware,
        })
    }

    pub fn state(&self) -> krkr_shim::video::VideoState {
        krkr_shim::video::VideoState {
            width: self.width,
            height: self.height,
            duration: self.duration,
            fps: self.fps,
            audio_streams: self.audio_streams,
            video_streams: self.video_indices.len() as u32,
            audio_stream: self
                .audio
                .as_ref()
                .and_then(|audio| self.audio_indices.iter().position(|&i| i == audio.stream))
                .map_or(-1, |i| i as i32),
            video_stream: self
                .video_indices
                .iter()
                .position(|&i| i == self.stream)
                .map_or(-1, |i| i as i32),
            ..Default::default()
        }
    }

    pub fn select_stream(
        &mut self,
        video: bool,
        ordinal: usize,
        seconds: f64,
    ) -> Result<(), String> {
        let indices = if video {
            &self.video_indices
        } else {
            &self.audio_indices
        };
        let index = *indices
            .get(ordinal)
            .ok_or("invalid video/audio stream index")?;
        let stream = self.input.stream(index).ok_or("missing media stream")?;
        if video {
            let context = codec::context::Context::from_parameters(stream.parameters())
                .map_err(|e| e.to_string())?;
            let (decoder, backend) = if self.hardware {
                hardware_decoder(context)?
            } else {
                (
                    context.decoder().video().map_err(|e| e.to_string())?,
                    "software".into(),
                )
            };
            if decoder.width() == 0 || decoder.height() == 0 {
                return Err("invalid video dimensions".into());
            }
            self.width = decoder.width();
            self.height = decoder.height();
            self.time_base = f64::from(stream.time_base());
            self.start = if stream.start_time() == ffi::AV_NOPTS_VALUE {
                0.0
            } else {
                stream.start_time() as f64 * self.time_base
            };
            let fps = f64::from(stream.avg_frame_rate());
            self.fps = if fps.is_finite() && fps > 0.0 {
                fps
            } else {
                30.0
            };
            self.duration = if stream.duration() > 0 {
                stream.duration() as f64 * self.time_base
            } else {
                self.input.duration().max(0) as f64 / f64::from(ffi::AV_TIME_BASE)
            };
            self.stream = index;
            self.video = decoder;
            self.backend = backend;
        } else {
            self.audio = Some(AudioDecoder::open(stream)?);
        }
        self.seek(seconds)
    }

    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        let timestamp = ((seconds + self.start) * f64::from(ffi::AV_TIME_BASE)) as i64;
        self.input
            .seek(timestamp, ..timestamp)
            .map_err(|e| e.to_string())?;
        self.video.flush();
        if let Some(audio) = &mut self.audio {
            audio.decoder.flush();
            unsafe {
                ffi::swr_close(audio.resampler.as_mut_ptr());
            }
            let result = unsafe { ffi::swr_init(audio.resampler.as_mut_ptr()) };
            if result < 0 {
                return Err(ffmpeg::Error::from(result).to_string());
            }
        }
        self.next_time = seconds;
        self.audio_frames.clear();
        self.eof = false;
        self.draining = false;
        Ok(())
    }

    pub fn next(
        &mut self,
        scaler: &mut Option<software::scaling::context::Context>,
    ) -> Result<Option<Frame>, String> {
        if self.eof {
            return Ok(None);
        }
        let mut decoded = frame::Video::empty();
        loop {
            match self.video.receive_frame(&mut decoded) {
                Ok(()) => break,
                Err(ffmpeg::Error::Eof) => {
                    self.eof = true;
                    return Ok(None);
                }
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {}
                Err(e) => return Err(e.to_string()),
            }
            if self.draining {
                return Err("video decoder did not finish draining".into());
            }
            let mut packet = ffmpeg::Packet::empty();
            loop {
                match packet.read(&mut self.input) {
                    Ok(()) if packet.stream() == self.stream => {
                        self.video.send_packet(&packet).map_err(|e| e.to_string())?;
                        break;
                    }
                    Ok(()) => {
                        if let Some(audio) = &mut self.audio
                            && packet.stream() == audio.stream
                        {
                            audio
                                .decoder
                                .send_packet(&packet)
                                .map_err(|e| e.to_string())?;
                            self.audio_frames.extend(audio.receive(self.start)?);
                        }
                    }
                    Err(ffmpeg::Error::Eof) => {
                        self.video.send_eof().map_err(|e| e.to_string())?;
                        if let Some(audio) = &mut self.audio {
                            audio.decoder.send_eof().map_err(|e| e.to_string())?;
                            self.audio_frames.extend(audio.receive(self.start)?);
                        }
                        self.draining = true;
                        break;
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        let time = decoded
            .timestamp()
            .or(decoded.pts())
            .map_or(self.next_time, |pts| {
                pts as f64 * self.time_base - self.start
            });
        self.next_time = time + 1.0 / self.fps;
        let mut transferred = frame::Video::empty();
        if self.hardware
            && !cfg!(target_os = "android")
            && unsafe { (*decoded.as_ptr()).hw_frames_ctx.is_null() }
        {
            return Err("hardware decoder returned a software frame".into());
        }
        let software = unsafe {
            if !(*decoded.as_ptr()).hw_frames_ctx.is_null() {
                let result =
                    ffi::av_hwframe_transfer_data(transferred.as_mut_ptr(), decoded.as_ptr(), 0);
                if result < 0 {
                    return Err(ffmpeg::Error::from(result).to_string());
                }
                &transferred
            } else {
                &decoded
            }
        };
        if scaler.as_ref().is_none_or(|scaler| {
            scaler.input().format != software.format()
                || scaler.input().width != software.width()
                || scaler.input().height != software.height()
        }) {
            *scaler = Some(
                software::scaling::context::Context::get(
                    software.format(),
                    software.width(),
                    software.height(),
                    format::Pixel::RGBA,
                    self.width,
                    self.height,
                    software::scaling::flag::Flags::BILINEAR,
                )
                .map_err(|e| e.to_string())?,
            );
        }
        let space = match decoded.color_space() {
            ffmpeg::color::Space::BT709 => ffi::SWS_CS_ITU709,
            ffmpeg::color::Space::BT2020NCL | ffmpeg::color::Space::BT2020CL => ffi::SWS_CS_BT2020,
            ffmpeg::color::Space::SMPTE240M => ffi::SWS_CS_SMPTE240M,
            _ => ffi::SWS_CS_DEFAULT,
        };
        unsafe {
            let coefficients = ffi::sws_getCoefficients(space);
            let result = ffi::sws_setColorspaceDetails(
                scaler.as_mut().unwrap().as_mut_ptr(),
                coefficients,
                i32::from(decoded.color_range() == ffmpeg::color::Range::JPEG),
                coefficients,
                1,
                0,
                1 << 16,
                1 << 16,
            );
            if result < 0 {
                return Err(ffmpeg::Error::from(result).to_string());
            }
        }
        let mut rgba = frame::Video::empty();
        scaler
            .as_mut()
            .unwrap()
            .run(software, &mut rgba)
            .map_err(|e| e.to_string())?;
        Ok(Some(Frame {
            pitch: rgba.stride(0) as u32,
            pixels: rgba,
            width: self.width,
            height: self.height,
            time,
        }))
    }
}

fn hardware_decoder(context: codec::Context) -> Result<(ffmpeg::decoder::Video, String), String> {
    #[cfg(target_os = "android")]
    {
        let codec_name = match context.id() {
            codec::Id::MPEG2VIDEO => "mpeg2",
            id => id.name(),
        };
        let name = format!("{codec_name}_mediacodec");
        let codec = codec::decoder::find_by_name(&name)
            .ok_or_else(|| format!("decoder {name} unavailable"))?;
        let mut options = ffmpeg::Dictionary::new();
        options.set("ndk_codec", "1");
        return context
            .decoder()
            .open_as_with(codec, options)
            .and_then(|d| d.video())
            .map(|video| (video, "mediacodec".into()))
            .map_err(|e| e.to_string());
    }
    #[cfg(not(target_os = "android"))]
    unsafe {
        let mut context = context;
        let codec = codec::decoder::find(context.id()).ok_or("video decoder unavailable")?;
        let names: &[&std::ffi::CStr] = if cfg!(target_vendor = "apple") {
            &[c"videotoolbox"]
        } else if cfg!(target_os = "windows") {
            &[c"d3d11va", c"cuda"]
        } else {
            &[c"vaapi", c"cuda", c"vulkan"]
        };
        let mut errors = Vec::new();
        for name in names {
            let kind = ffi::av_hwdevice_find_type_by_name(name.as_ptr());
            let mut supported = false;
            for index in 0.. {
                let config = ffi::avcodec_get_hw_config(codec.as_ptr(), index);
                if config.is_null() {
                    break;
                }
                if (*config).device_type == kind
                    && (*config).methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32 != 0
                {
                    supported = true;
                    break;
                }
            }
            if !supported {
                continue;
            }
            let mut device = std::ptr::null_mut();
            let result = ffi::av_hwdevice_ctx_create(
                &mut device,
                kind,
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            );
            if result < 0 {
                errors.push(format!(
                    "{}: {}",
                    name.to_string_lossy(),
                    ffmpeg::Error::from(result)
                ));
                continue;
            }
            (*context.as_mut_ptr()).hw_device_ctx = device;
            (*context.as_mut_ptr()).get_format = Some(hardware_format);
            return context
                .decoder()
                .open_as(codec)
                .and_then(|d| d.video())
                .map(|video| (video, name.to_string_lossy().into_owned()))
                .map_err(|e| e.to_string());
        }
        Err(format!("no hardware video decoder: {}", errors.join(", ")))
    }
}

#[cfg(not(target_os = "android"))]
unsafe extern "C" fn hardware_format(
    context: *mut ffi::AVCodecContext,
    mut formats: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    unsafe {
        let device = (*(*context).hw_device_ctx)
            .data
            .cast::<ffi::AVHWDeviceContext>();
        while *formats != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            for index in 0.. {
                let config = ffi::avcodec_get_hw_config((*context).codec, index);
                if config.is_null() {
                    break;
                }
                if (*config).device_type == (*device).type_ && (*config).pix_fmt == *formats {
                    return *formats;
                }
            }
            formats = formats.add(1);
        }
        ffi::AVPixelFormat::AV_PIX_FMT_NONE
    }
}

impl AudioDecoder {
    fn open(stream: format::stream::Stream<'_>) -> Result<Self, String> {
        let create = || -> Result<Self, ffmpeg::Error> {
            let decoder = codec::context::Context::from_parameters(stream.parameters())?
                .decoder()
                .audio()?;
            let layout = if decoder.channel_layout().is_empty() {
                ffmpeg::ChannelLayout::default(decoder.channels().into())
            } else {
                decoder.channel_layout()
            };
            let resampler = software::resampling::context::Context::get(
                decoder.format(),
                layout,
                decoder.rate(),
                format::Sample::F32(format::sample::Type::Packed),
                ffmpeg::ChannelLayout::STEREO,
                48000,
            )?;
            Ok(Self {
                decoder,
                stream: stream.index(),
                time_base: f64::from(stream.time_base()),
                resampler,
            })
        };
        create().map_err(|e| e.to_string())
    }

    fn receive(&mut self, start: f64) -> Result<Vec<AudioFrame>, String> {
        let mut frames = Vec::new();
        let mut decoded = frame::Audio::empty();
        loop {
            match self.decoder.receive_frame(&mut decoded) {
                Ok(()) => {}
                Err(ffmpeg::Error::Eof) => break,
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => break,
                Err(error) => return Err(error.to_string()),
            }
            let time =
                decoded.timestamp().or(decoded.pts()).unwrap_or(0) as f64 * self.time_base - start;
            if decoded.rate() == 0 {
                return Err("invalid audio sample rate".into());
            }
            let capacity = (decoded.samples() as u64 * 48000)
                .div_ceil(u64::from(decoded.rate()))
                .saturating_add(256) as usize;
            let mut pcm = frame::Audio::new(
                format::Sample::F32(format::sample::Type::Packed),
                capacity,
                ffmpeg::ChannelLayout::STEREO,
            );
            self.resampler
                .run(&decoded, &mut pcm)
                .map_err(|e| e.to_string())?;
            let samples = pcm
                .plane::<(f32, f32)>(0)
                .iter()
                .flat_map(|&(left, right)| [left, right])
                .collect();
            frames.push(AudioFrame { time, samples });
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_selection_changes_decoded_video_and_audio() {
        let bytes = include_bytes!("../../../../tests/fixtures/multistream.mp4").to_vec();
        let mut decoder = Decoder::open(bytes, false).unwrap();
        assert_eq!(decoder.state().audio_streams, 2);
        assert_eq!(decoder.state().video_streams, 2);
        let mut crossings = Vec::new();
        for index in 0..2 {
            decoder.select_stream(false, index, 0.0).unwrap();
            decoder.select_stream(true, index, 0.0).unwrap();
            let mut scaler = None;
            let mut samples = Vec::new();
            while let Some(frame) = decoder.next(&mut scaler).unwrap() {
                assert_eq!(frame.width, if index == 0 { 64 } else { 32 });
                let rgba = frame.pixels.data(0);
                assert!(rgba[if index == 0 { 0 } else { 2 }] > 240);
                for audio in std::mem::take(&mut decoder.audio_frames) {
                    samples.extend(audio.samples.chunks_exact(2).map(|pair| pair[0]));
                }
            }
            assert!(samples.len() > 40000);
            crossings.push(
                samples
                    .windows(2)
                    .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
                    .count(),
            );
        }
        assert!((400..500).contains(&crossings[0]), "{crossings:?}");
        assert!((800..1000).contains(&crossings[1]), "{crossings:?}");
    }
}
