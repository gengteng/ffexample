mod audio_frame;
mod image;
mod picture;

pub use audio_frame::*;
use ffmpeg_next::{Packet, Rational};
pub use image::*;
pub use picture::*;
use std::fmt::{Display, Formatter};

pub struct Timestamp {
    ts: Option<i64>,
    base: Rational,
}

impl Timestamp {
    pub fn new(ts: Option<i64>, base: Rational) -> Self {
        Self { ts, base }
    }

    pub fn ts_string(&self) -> String {
        match self.ts {
            None => "NOPTS".into(),
            Some(ts) => format!("{}", ts),
        }
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.ts {
            None => write!(f, "NOPTS"),
            Some(pts) => write!(f, "{:.4}", pts as f64 * f64::from(self.base)),
        }
    }
}

pub fn log_packet(time_base: Rational, packet: &Packet, tag: &'static str) {
    let pts = Timestamp::new(packet.pts(), time_base);
    let dts = Timestamp::new(packet.dts(), time_base);
    let duration = Timestamp::new(Some(packet.duration()), time_base);

    println!(
        "{}: pts:{} pts_time:{} dts:{} dts_time:{} duration:{} duration_time:{} stream_index:{}",
        tag,
        pts.ts_string(),
        pts,
        dts.ts_string(),
        dts,
        duration.ts_string(),
        duration,
        packet.stream()
    );
}
