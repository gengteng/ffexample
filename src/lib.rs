mod image;

use ffmpeg_next::Rational;
pub use image::*;
use std::fmt::{Display, Formatter};

pub struct Timestamp(pub Option<i64>);

impl Timestamp {
    pub fn to_time(&self, time_base: Rational) -> String {
        match self.0 {
            None => self.to_string(),
            Some(pts) => format!("{:.4}", pts as f64 * f64::from(time_base)),
        }
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            None => write!(f, "NOPTS"),
            Some(pts) => write!(f, "{}", pts),
        }
    }
}
