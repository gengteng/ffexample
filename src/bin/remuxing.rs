use clap::Parser;
use ffexample::log_packet;
use ffmpeg_next::format::context;
use ffmpeg_next::media::Type;
use ffmpeg_next::{Error, Packet, Rational};
use std::path::PathBuf;

/// Remux streams from one container format to another.
#[derive(Debug, Parser)]
struct Opts {
    /// Source file path
    #[clap()]
    source: PathBuf,

    /// Destination file path
    #[clap()]
    destination: PathBuf,
}

struct RemuxingContext {
    input: context::Input,
    stream_mapping: Vec<Option<usize>>,
    output: context::Output,
    packet: Packet,
}

impl RemuxingContext {
    pub fn new(opts: Opts) -> anyhow::Result<Self> {
        let input = ffmpeg_next::format::input(&opts.source)?;
        ffmpeg_next::format::context::input::dump(&input, 0, None);

        let mut stream_mapping = vec![None; input.nb_streams() as usize];

        let mut output = ffmpeg_next::format::output(&opts.destination)?;

        let mut stream_index = 0;
        for stream in input.streams() {
            match stream.parameters().medium() {
                Type::Video | Type::Audio | Type::Subtitle => {
                    stream_mapping[stream.index()] = Some(stream_index);
                    stream_index += 1;
                }
                _ => {
                    stream_mapping[stream.index()] = None;
                    continue;
                }
            }

            let mut new_stream = output.add_stream(None)?;
            new_stream.set_parameters(stream.parameters());

            unsafe {
                (*new_stream.parameters().as_mut_ptr()).codec_tag = 0;
            }
        }

        ffmpeg_next::format::context::output::dump(&output, 0, None);

        let packet = ffmpeg_next::packet::Packet::empty();

        Ok(RemuxingContext {
            input,
            stream_mapping,
            output,
            packet,
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        self.output.write_header()?;

        loop {
            if let Err(e) = self.packet.read(&mut self.input) {
                match e {
                    Error::Eof | Error::Other { errno: 35 } => {
                        break;
                    }
                    e => return Err(e.into()),
                }
            }

            let in_stream = self.input.stream(self.packet.stream()).unwrap();
            if in_stream.index() >= self.stream_mapping.len() {
                continue;
            }

            let out_stream = if let Some(output_stream) = self.stream_mapping[in_stream.index()] {
                self.packet.set_stream(output_stream);
                self.output.stream(output_stream).unwrap()
            } else {
                continue;
            };

            log_packet(in_stream.time_base(), &self.packet, "in");

            self.packet
                .rescale_ts(in_stream.time_base(), out_stream.time_base());
            self.packet.set_position(-1);

            log_packet(out_stream.time_base(), &self.packet, "out");

            self.packet.write_interleaved(&mut self.output)?;
        }

        self.output.write_trailer()?;

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    RemuxingContext::new(opts)?.run()
}
