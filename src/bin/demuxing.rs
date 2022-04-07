use ffexample::{Image, Timestamp};

use clap::Parser;
use ffmpeg_next::decoder::Decoder;
use ffmpeg_next::format::context;
use ffmpeg_next::media::Type;
use ffmpeg_next::{decoder, format, frame};
use ffmpeg_next::{Error, Packet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::ops::DerefMut;
use std::path::PathBuf;

/// Demuxing and decoding example.
#[derive(Debug, Parser)]
struct Opts {
    /// Source file path
    #[clap()]
    source: PathBuf,

    /// Video destination file path
    #[clap()]
    destination_video: PathBuf,

    /// Audio destination file path
    #[clap()]
    destination_audio: PathBuf,
}

struct VideoContext {
    stream_idx: usize,
    dec_ctx: decoder::Video,
    width: u32,
    height: u32,
    pixel: format::Pixel,
    frame: frame::Video,
    frame_count: u32,
    image: Image,
    dst_path: PathBuf,
    dst_file: File,
}

impl VideoContext {
    fn new(input: &context::Input, dst_path: PathBuf) -> anyhow::Result<Self> {
        let video_stream = input
            .streams()
            .best(Type::Video)
            .ok_or_else(|| anyhow::anyhow!("Failed to find best video stream"))?;
        let stream_idx = video_stream.index();

        let mut video_codec_ctx = ffmpeg_next::codec::Context::new();
        video_codec_ctx.set_parameters(video_stream.parameters())?;

        let decoder = ffmpeg_next::decoder::find(video_stream.parameters().id())
            .ok_or_else(|| anyhow::anyhow!("Failed to find video decoder"))?;

        let dec_ctx = Decoder(video_codec_ctx).open_as(decoder)?.video()?;
        let dst_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&dst_path)?;

        let width = dec_ctx.width();
        let height = dec_ctx.height();
        let pixel = dec_ctx.format();

        Ok(Self {
            stream_idx,
            dec_ctx,
            width,
            height,
            pixel,
            frame: frame::Video::empty(),
            frame_count: 0,
            image: Image::new(width, height, pixel, 1)?,
            dst_path,
            dst_file,
        })
    }

    pub fn decode_packet(&mut self, packet: Option<&Packet>) -> anyhow::Result<()> {
        match packet {
            None => self.dec_ctx.send_eof()?,
            Some(packet) => self.dec_ctx.send_packet(packet)?,
        }

        loop {
            if let Err(e) = self.dec_ctx.receive_frame(self.frame.deref_mut()) {
                match e {
                    Error::Eof | Error::Other { errno: 35 } => {
                        break;
                    }
                    e => return Err(e.into()),
                }
            }

            if self.frame.width() != self.width
                || self.frame.height() != self.height
                || self.frame.format() != self.pixel
            {
                anyhow::bail!("Error: Width, height and pixel format have to be constant in a rawvideo file, but the width,\
                    height or pixel format of the input video changed:\n\
                    old: width = {}, height = {}, format = {}\n\
                    new: width = {}, height = {}, format = {}", 
                    self.width, self.height, self.pixel.descriptor().unwrap().name(),
                    self.frame.width(), self.frame.height(), self.frame.format().descriptor().unwrap().name());
            }

            // 输出到文件
            self.image.copy_from_video(&self.frame);
            let data = self.image.data();
            self.dst_file.write_all(data)?;

            println!(
                "video_frame n:{} coded_n:{}",
                self.frame_count,
                self.frame.coded_number(),
            );
            self.frame_count += 1;

            // 不需要对 Frame 进行 unref，receive_frame 内部会做这个工作
        }

        Ok(())
    }

    pub fn close(&mut self) -> anyhow::Result<()> {
        self.decode_packet(None)?;
        self.dst_file.flush()?;
        println!("Play the output video file with the command:\nffplay -f rawvideo -pixel_format {} -video_size {}x{} {}",
                 self.pixel.descriptor().ok_or_else(|| anyhow::anyhow!("Failed to get descriptor of video format"))?.name(), self.width, self.height,
                 self.dst_path.display());
        Ok(())
    }
}

impl Drop for VideoContext {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("Failed to close video context: {}", e);
        }
    }
}

struct AudioContext {
    stream_idx: usize,
    dec_ctx: decoder::Audio,
    sample: format::Sample,
    frame: frame::Audio,
    frame_count: u32,
    dst_path: PathBuf,
    dst_file: File,
}

impl AudioContext {
    fn new(input: &context::Input, dst_path: PathBuf) -> anyhow::Result<Self> {
        let audio_stream = input
            .streams()
            .best(Type::Audio)
            .ok_or_else(|| anyhow::anyhow!("Failed to find best audio stream"))?;
        let stream_idx = audio_stream.index();

        let mut audio_codec_ctx = ffmpeg_next::codec::Context::new();
        audio_codec_ctx.set_parameters(audio_stream.parameters())?;

        let decoder = ffmpeg_next::decoder::find(audio_stream.parameters().id())
            .ok_or_else(|| anyhow::anyhow!("Failed to find video decoder"))?;

        let dec_ctx = Decoder(audio_codec_ctx).open_as(decoder)?.audio()?;
        let dst_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&dst_path)?;

        let sample = dec_ctx.format();

        Ok(Self {
            stream_idx,
            dec_ctx,
            sample,
            frame: frame::Audio::empty(),
            frame_count: 0,
            dst_path,
            dst_file,
        })
    }

    pub fn decode_packet(&mut self, packet: Option<&Packet>) -> anyhow::Result<()> {
        match packet {
            None => self.dec_ctx.send_eof()?,
            Some(packet) => self.dec_ctx.send_packet(packet)?,
        }

        loop {
            if let Err(e) = self.dec_ctx.receive_frame(self.frame.deref_mut()) {
                match e {
                    Error::Eof | Error::Other { errno: 35 } => {
                        break;
                    }
                    e => return Err(e.into()),
                }
            }

            let unpadded_line_size = self.frame.samples() * self.sample.bytes();
            let data = &self.frame.data(0)[0..unpadded_line_size];
            self.dst_file.write_all(data)?;

            println!(
                "audio_frame n:{} nb_samples:{} pts:{}",
                self.frame_count,
                self.frame.samples(),
                Timestamp(self.frame.pts()).to_time(self.dec_ctx.time_base())
            );
            self.frame_count += 1;
        }
        Ok(())
    }

    pub fn close(&mut self) -> anyhow::Result<()> {
        self.decode_packet(None)?;
        self.dst_file.flush()?;

        let sample_str = sample_to_str(self.sample).ok_or_else(|| {
            anyhow::anyhow!(
                "sample format {} is not supported as output format",
                self.sample.name()
            )
        })?;

        let channels = if self.sample.is_planar() {
            println!("Warning: the sample format the decoder produced is planar ({}).\nThis example will output the first channel only.", self.sample.name());
            1
        } else {
            self.dec_ctx.channels()
        };

        println!(
            "Play the output audio file with the command:\nffplay -f {} -ac {} -ar {} {}",
            sample_str,
            channels,
            self.dec_ctx.rate(),
            self.dst_path.display()
        );
        Ok(())
    }
}

impl Drop for AudioContext {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("Failed to close audio context: {}", e);
        }
    }
}

struct DemuxingContext {
    input: context::Input,
    packet: Packet,

    video: Option<VideoContext>,
    audio: Option<AudioContext>,
}

impl DemuxingContext {
    pub fn new(opts: Opts) -> anyhow::Result<Self> {
        let Opts {
            source,
            destination_video,
            destination_audio,
        } = opts;

        let input = ffmpeg_next::format::input(&source)?;

        let video = VideoContext::new(&input, destination_video).ok();
        let audio = AudioContext::new(&input, destination_audio).ok();

        if video.is_none() && audio.is_none() {
            anyhow::bail!("Could not find audio or video stream in the input, aborting");
        }

        if let Some(video) = &video {
            println!(
                "Demuxing video from file '{}' into '{}'",
                source.display(),
                video.dst_path.display()
            );
        }

        if let Some(audio) = &audio {
            println!(
                "Demuxing audio from file '{}' into '{}'",
                source.display(),
                audio.dst_path.display()
            );
        }

        let packet = ffmpeg_next::packet::Packet::empty();

        Ok(DemuxingContext {
            input,
            packet,
            video,
            audio,
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            if let Err(e) = self.packet.read(&mut self.input) {
                match e {
                    Error::Eof | Error::Other { errno: 35 } => {
                        break;
                    }
                    e => return Err(e.into()),
                }
            }

            if let Some(video) = &mut self.video {
                if self.packet.stream() == video.stream_idx {
                    video.decode_packet(Some(&self.packet))?;
                    continue;
                }
            }

            if let Some(audio) = &mut self.audio {
                if self.packet.stream() == audio.stream_idx {
                    audio.decode_packet(Some(&self.packet))?;
                }
            }
        }

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    DemuxingContext::new(opts)?.run()
}

fn sample_to_str(sample: format::Sample) -> Option<&'static str> {
    use format::Sample;
    match sample {
        Sample::None => None,
        Sample::U8(_) => "u8".into(),
        Sample::I16(_) => "s16le".into(),
        Sample::I32(_) => "s32le".into(),
        Sample::I64(_) => "s64le".into(),
        Sample::F32(_) => "f32le".into(),
        Sample::F64(_) => "f64le".into(),
    }
}
