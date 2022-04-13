use clap::Parser;
use ffexample::{log_packet, AudioFrame, Picture};
use ffmpeg_next::codec::traits::Encoder;
use ffmpeg_next::codec::{Capabilities, Id};
use ffmpeg_next::encoder::{audio, video, Decision};
use ffmpeg_next::format::sample::Type;
use ffmpeg_next::format::{context, Flags, Pixel, Sample};
use ffmpeg_next::software::{resampling, scaling};
use ffmpeg_next::{codec, format, ChannelLayout, Error, Rational, Rescale, Rounding};
use ffmpeg_sys_next::{av_compare_ts, av_rescale_rnd};
use std::ops::Deref;
use std::path::PathBuf;

const STREAM_DURATION: f64 = 10.0;
const STREAM_FRAME_RATE: i32 = 25;

#[derive(Debug, clap::Parser)]
struct Opts {
    #[clap()]
    destination: PathBuf,
}

trait FrameWriter {
    fn get_frame(&mut self) -> anyhow::Result<bool>;
    fn write_frame(&mut self, output: &mut context::Output) -> anyhow::Result<()>;
}

struct VideoContext {
    video_encoder_ctx: video::Encoder,
    video_stream_index: usize,
    time_base: Rational,
    frame: Picture,
    tmp_frame: Option<Picture>,
    sws_ctx: Option<scaling::Context>,
    packet: ffmpeg_next::Packet,
    next_pts: i64,
    encode: bool,
}

impl FrameWriter for VideoContext {
    fn get_frame(&mut self) -> anyhow::Result<bool> {
        if unsafe {
            av_compare_ts(
                self.next_pts,
                self.time_base.into(),
                STREAM_DURATION as i64,
                Rational::new(1, 1).into(),
            ) > 0
        } {
            return Ok(false);
        }

        self.frame.make_writable()?;

        if let (Some(sws_ctx), Some(tmp_frame)) = (self.sws_ctx.as_mut(), self.tmp_frame.as_mut()) {
            tmp_frame.fill(self.next_pts as u32);
            let in_video = tmp_frame.as_video();
            let mut out_video = self.frame.as_video();
            sws_ctx.run(&in_video, &mut out_video)?;
        } else {
            self.frame.fill(self.next_pts as u32);
        }

        self.frame.set_pts(self.next_pts);
        self.next_pts += 1;

        return Ok(true);
    }

    fn write_frame(&mut self, output: &mut context::Output) -> anyhow::Result<()> {
        let get = self.get_frame()?;
        if get {
            self.video_encoder_ctx
                .send_frame(self.frame.as_video().deref())?;
        } else {
            self.video_encoder_ctx.send_eof()?;
        }

        loop {
            if let Err(e) = self.video_encoder_ctx.receive_packet(&mut self.packet) {
                match e {
                    Error::Eof => {
                        self.encode = false;
                    }
                    Error::Other { errno: 35 } => {
                        break;
                    }
                    e => {
                        anyhow::bail!("Error encoding a frame: {}", e);
                    }
                }
            }

            /* rescale output packet timestamp values from codec to stream timebase */
            self.packet.rescale_ts(
                Rational::from(unsafe { (*self.video_encoder_ctx.as_ptr()).time_base }),
                self.time_base,
            );
            self.packet.set_stream(self.video_stream_index);

            /* Write the compressed frame to the media file. */
            log_packet(self.time_base, &self.packet, "output");

            if let Err(e) = self.packet.write_interleaved(output) {
                anyhow::bail!("Error while writing output packet: {}", e);
            }
            // pkt is now blank (av_interleaved_write_frame() takes ownership of
            // its contents and resets pkt), so that no unreferencing is necessary.
            // This would be different if one used av_write_frame().
        }

        Ok(())
    }
}

struct AudioContext {
    audio_encoder_ctx: audio::Encoder,
    audio_stream_index: usize,
    time_base: Rational,
    t: f32,
    tincr: f32,
    tincr2: f32,
    nb_samples: u32,
    samples_count: i64,
    frame: AudioFrame,
    tmp_frame: AudioFrame,
    swr_ctx: resampling::Context,
    packet: ffmpeg_next::Packet,
    next_pts: i64,
    encode: bool,
}

impl FrameWriter for AudioContext {
    fn get_frame(&mut self) -> anyhow::Result<bool> {
        if unsafe {
            av_compare_ts(
                self.next_pts,
                self.time_base.into(),
                STREAM_DURATION as i64,
                Rational::new(1, 1).into(),
            ) > 0
        } {
            return Ok(false);
        }

        let data = self.tmp_frame.data_mut();
        for _ in 0..self.nb_samples {
            let v = (self.t.sin() * 10000f32) as i32;
            for i in 0..self.audio_encoder_ctx.channels() as usize {
                data[i] = v as u8;
            }
            self.t += self.tincr;
            self.tincr += self.tincr2;
        }

        self.tmp_frame.set_pts(self.next_pts);
        self.next_pts += self.tmp_frame.nb_samples() as i64;

        Ok(true)
    }

    fn write_frame(&mut self, output: &mut context::Output) -> anyhow::Result<()> {
        let get = self.get_frame()?;
        if get {
            let delay = self
                .swr_ctx
                .delay()
                .ok_or_else(|| anyhow::anyhow!("Failed to get delay"))?;
            let sample_rate = self.audio_encoder_ctx.rate() as i64;
            let dst_nb_samples = unsafe {
                av_rescale_rnd(
                    delay.output + self.frame.nb_samples() as i64,
                    sample_rate,
                    sample_rate,
                    Rounding::Up.into(),
                )
            };
            assert_eq!(dst_nb_samples, self.nb_samples as i64);

            self.frame.make_writable()?;

            self.swr_ctx
                .run(&self.tmp_frame.as_audio(), &mut self.frame.as_audio())?;
            self.frame.set_pts(
                self.samples_count
                    .rescale(Rational::new(1, sample_rate as i32), self.time_base),
            );
            self.samples_count += dst_nb_samples;

            self.audio_encoder_ctx
                .send_frame(self.frame.as_audio().deref())?;
        } else {
            self.audio_encoder_ctx.send_eof()?;
        }

        loop {
            if let Err(e) = self.audio_encoder_ctx.receive_packet(&mut self.packet) {
                match e {
                    Error::Eof => {
                        self.encode = false;
                    }
                    Error::Other { errno: 35 } => {
                        break;
                    }
                    e => {
                        anyhow::bail!("Error encoding a frame: {}", e);
                    }
                }
            }

            /* rescale output packet timestamp values from codec to stream timebase */
            self.packet.rescale_ts(
                Rational::from(unsafe { (*self.audio_encoder_ctx.as_ptr()).time_base }),
                self.time_base,
            );
            self.packet.set_stream(self.audio_stream_index);

            /* Write the compressed frame to the media file. */
            log_packet(self.time_base, &self.packet, "output");

            if let Err(e) = self.packet.write_interleaved(output) {
                anyhow::bail!("Error while writing output packet: {}", e);
            }
            // pkt is now blank (av_interleaved_write_frame() takes ownership of
            // its contents and resets pkt), so that no unreferencing is necessary.
            // This would be different if one used av_write_frame().
        }

        Ok(())
    }
}

struct Muxing {
    video: Option<VideoContext>,
    audio: Option<AudioContext>,
}

impl Muxing {
    pub fn new(opts: Opts) -> anyhow::Result<(context::Output, Self)> {
        let Opts { destination } = opts;

        let mut output = format::output(&destination).or_else(|_| {
            println!("Could not deduce output format from file extension: using MPEG.");
            format::output_as(&destination, "mpeg")
        })?;

        let format = output.format();

        let global_header = format.flags().contains(Flags::GLOBAL_HEADER);

        let video_codec_id = Id::from(unsafe { (*format.as_ptr()).video_codec });
        let has_video = video_codec_id != Id::None;
        let video = if has_video {
            let mut video_stream = output.add_stream(video_codec_id)?;
            // let video_codec = video_codec_id
            //     .encoder()
            //     .ok_or_else(|| anyhow::anyhow!("Failed to get codec of {}", video_codec_id.name()))?
            //     .video()?;
            let mut video_encoder_ctx = codec::encoder::Encoder(codec::Context::new())
                .video()?
                .open_as(video_codec_id)?;
            video_encoder_ctx.set_bit_rate(400_000);

            // 分辨率必须为 2 的倍数
            video_encoder_ctx.set_width(352);
            video_encoder_ctx.set_height(288);

            let time_base = Rational::new(1, STREAM_FRAME_RATE);
            video_encoder_ctx.set_time_base(time_base);
            video_stream.set_time_base(time_base);

            // 每 12 帧一个 I 帧
            video_encoder_ctx.set_gop(12);

            video_encoder_ctx.set_format(Pixel::YUV420P);

            if video_codec_id == Id::MPEG2VIDEO {
                // 只是测试一下，我们也添加 B 帧
                video_encoder_ctx.set_max_b_frames(2);
            } else if video_codec_id == Id::MPEG1VIDEO {
                // 需要避免使用某些系数溢出的宏块。
                // 这不会发生在普通视频中，它只是在这里发生，
                // 因为色度平面的运动与亮度平面不匹配。
                video_encoder_ctx.set_mb_decision(Decision::RateDistortion);
            }

            if global_header {
                // 某些格式希望流标头是分开的。
                video_encoder_ctx.set_flags(codec::Flags::empty() | codec::Flags::GLOBAL_HEADER);
            }

            // open video
            let frame = Picture::new(
                video_encoder_ctx.format(),
                video_encoder_ctx.width(),
                video_encoder_ctx.height(),
            )?;
            let tmp_frame = if video_encoder_ctx.format() != Pixel::YUV420P {
                Some(Picture::new(
                    Pixel::YUV420P,
                    video_encoder_ctx.width(),
                    video_encoder_ctx.height(),
                )?)
            } else {
                None
            };
            video_stream.set_parameters(&video_encoder_ctx);

            let sws_ctx = if video_encoder_ctx.format() != Pixel::YUV420P {
                Some(scaling::Context::get(
                    Pixel::YUV420P,
                    video_encoder_ctx.width(),
                    video_encoder_ctx.height(),
                    video_encoder_ctx.format(),
                    video_encoder_ctx.width(),
                    video_encoder_ctx.height(),
                    scaling::Flags::BICUBIC,
                )?)
            } else {
                None
            };

            Some(VideoContext {
                video_encoder_ctx,
                video_stream_index: video_stream.index(),
                time_base: video_stream.time_base(),
                frame,
                tmp_frame,
                sws_ctx,
                packet: ffmpeg_next::Packet::empty(),
                next_pts: 0,
                encode: true,
            })
        } else {
            None
        };

        let audio_codec_id = Id::from(unsafe { (*format.as_ptr()).audio_codec });
        let has_audio = audio_codec_id != Id::None;
        let audio = if has_audio {
            let mut audio_stream = output.add_stream(audio_codec_id)?;
            let audio_codec = audio_codec_id
                .encoder()
                .ok_or_else(|| anyhow::anyhow!("Failed to get codec of {}", audio_codec_id.name()))?
                .audio()?;
            let mut audio_encoder_ctx = codec::encoder::Encoder(codec::Context::new())
                .audio()?
                .open_as(audio_codec_id)?;
            audio_encoder_ctx.set_format(
                audio_codec
                    .formats()
                    .and_then(|mut i| i.next())
                    .unwrap_or(Sample::F32(Type::Planar)),
            );
            audio_encoder_ctx.set_bit_rate(64000);
            // 这一段不太懂，暂时理解为：
            // 1. 如果 Codec 支持的 sample_rate 为空，则选择默认值 44100
            // 2. 如果 Codec 支持的 sample_rate 不为空，并且没有任何一个值为 44100，则采用第一个
            // 3. 否则，使用 44100
            audio_encoder_ctx.set_rate(44100);
            if let Some(mut rates) = audio_codec.rates() {
                if let Some(first) = rates.next() {
                    audio_encoder_ctx.set_rate(first);
                }

                for rate in rates {
                    if rate == 44100 {
                        audio_encoder_ctx.set_rate(44100);
                    }
                }
            }
            let channels = audio_encoder_ctx.channel_layout().channels();
            audio_encoder_ctx.set_channels(channels);

            // 这段逻辑跟上面选择 sample_fmt 类似
            audio_encoder_ctx.set_channel_layout(ChannelLayout::STEREO);
            if let Some(mut channel_layouts) = audio_codec.channel_layouts() {
                if let Some(first) = channel_layouts.next() {
                    audio_encoder_ctx.set_channel_layout(first);
                }

                for layout in channel_layouts {
                    if layout == ChannelLayout::STEREO {
                        audio_encoder_ctx.set_channel_layout(ChannelLayout::STEREO);
                    }
                }
            }
            let channels = audio_encoder_ctx.channel_layout().channels();
            audio_encoder_ctx.set_channels(channels);
            audio_stream.set_time_base(Rational::new(1, audio_encoder_ctx.rate() as i32));

            if global_header {
                // 某些格式希望流标头是分开的。
                audio_encoder_ctx.set_flags(codec::Flags::empty() | codec::Flags::GLOBAL_HEADER);
            }

            // open audio
            let sample_rate = audio_encoder_ctx.rate() as f32;
            let t = 0f32;
            let tincr = 2f32 * std::f32::consts::PI * 110.0 / sample_rate;
            // 以每秒 110 Hz 的速度递增频率
            let tincr2 = 2f32 * std::f32::consts::PI * 110.0 / sample_rate / sample_rate;

            let nb_samples = if audio_codec
                .capabilities()
                .contains(Capabilities::VARIABLE_FRAME_SIZE)
            {
                10000
            } else {
                audio_encoder_ctx.frame_size()
            };

            let frame = AudioFrame::new(
                audio_encoder_ctx.format(),
                audio_encoder_ctx.channel_layout(),
                audio_encoder_ctx.rate(),
                nb_samples,
            )?;
            let tmp_frame = AudioFrame::new(
                Sample::I16(Type::Packed),
                audio_encoder_ctx.channel_layout(),
                audio_encoder_ctx.rate(),
                nb_samples,
            )?;

            audio_stream.set_parameters(&audio_encoder_ctx);

            let swr_ctx = resampling::Context::get(
                Sample::I16(Type::Packed),
                audio_encoder_ctx.channel_layout(),
                audio_encoder_ctx.rate(),
                audio_encoder_ctx.format(),
                audio_encoder_ctx.channel_layout(),
                audio_encoder_ctx.rate(),
            )?;

            Some(AudioContext {
                audio_encoder_ctx,
                audio_stream_index: audio_stream.index(),
                time_base: audio_stream.time_base(),
                t,
                tincr,
                tincr2,
                nb_samples,
                samples_count: 0,
                frame,
                tmp_frame,
                swr_ctx,
                packet: ffmpeg_next::Packet::empty(),
                next_pts: 0,
                encode: true,
            })
        } else {
            None
        };

        Ok((output, Self { video, audio }))
    }

    pub fn run(&mut self, output: &mut context::Output) -> anyhow::Result<()> {
        output.write_header()?;

        while let Some(writer) = self.next_writer() {
            writer.write_frame(output)?;
        }

        output.write_trailer()?;
        Ok(())
    }

    fn next_writer(&mut self) -> Option<&mut dyn FrameWriter> {
        match (self.video.as_mut(), self.audio.as_mut()) {
            (Some(video), None) if video.encode => Some(video),
            (Some(video), Some(audio))
                if video.encode
                    && (!audio.encode
                        || unsafe {
                            av_compare_ts(
                                video.next_pts,
                                video.time_base.into(),
                                audio.next_pts,
                                audio.time_base.into(),
                            ) <= 0
                        }) =>
            {
                Some(video)
            }
            (_, Some(audio)) if audio.encode => Some(audio),
            _ => None,
        }
    }
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let (mut output, mut muxing) = Muxing::new(opts)?;
    muxing.run(&mut output)
}
