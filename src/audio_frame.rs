use ffmpeg_next::format::Sample;
use ffmpeg_next::{ChannelLayout, Error};
use ffmpeg_sys_next::{
    av_frame_alloc, av_frame_free, av_frame_get_buffer, AVFrame, AVSampleFormat,
};

pub struct AudioFrame {
    frame: *mut AVFrame,
}

impl AudioFrame {
    pub fn new(
        sample_fmt: Sample,
        channel_layout: ChannelLayout,
        sample_rate: u32,
        nb_samples: u32,
    ) -> anyhow::Result<Self> {
        let frame = unsafe {
            let mut frame: *mut AVFrame = av_frame_alloc();
            if frame.is_null() {
                anyhow::bail!("Failed to alloc frame: Out of memory");
            }

            (*frame).format = AVSampleFormat::from(sample_fmt) as i32;
            (*frame).channel_layout = channel_layout.bits();
            (*frame).sample_rate = sample_rate as _;
            (*frame).nb_samples = nb_samples as _;

            if nb_samples != 0 {
                let ret = av_frame_get_buffer(frame, 0);

                if ret < 0 {
                    av_frame_free((&mut frame) as _);

                    let error = Error::from(ret);
                    anyhow::bail!("Could not allocate frame data: {}", error);
                }
            }

            frame
        };

        Ok(Self { frame })
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(
                (*self.frame).data[0],
                (*self.frame).linesize[0] as usize,
            )
        }
    }
}

impl Drop for AudioFrame {
    fn drop(&mut self) {
        unsafe { av_frame_free((&mut self.frame) as _) }
    }
}
