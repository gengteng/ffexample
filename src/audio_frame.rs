use ffmpeg_next::format::Sample;
use ffmpeg_next::{frame, ChannelLayout, Error};
use ffmpeg_sys_next::{
    av_frame_alloc, av_frame_free, av_frame_get_buffer, av_frame_make_writable, AVFrame,
    AVSampleFormat,
};

pub struct AudioFrame {
    frame: frame::Audio,
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

        Ok(Self {
            frame: unsafe { frame::Audio::wrap(frame) },
        })
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        unsafe {
            let frame = self.frame.as_mut_ptr();
            std::slice::from_raw_parts_mut((*frame).data[0], (*frame).linesize[0] as usize)
        }
    }

    pub fn data(&self) -> &[u8] {
        unsafe {
            let frame = self.frame.as_ptr();
            std::slice::from_raw_parts((*frame).data[0], (*frame).linesize[0] as usize)
        }
    }

    pub fn as_audio(&self) -> &frame::Audio {
        &self.frame
    }

    pub fn as_audio_mut(&mut self) -> &mut frame::Audio {
        &mut self.frame
    }

    pub fn make_writable(&mut self) -> anyhow::Result<()> {
        let ret = unsafe { av_frame_make_writable(self.frame.as_mut_ptr()) };
        if ret < 0 {
            anyhow::bail!("Failed to make frame writable: {}", Error::from(ret));
        }
        Ok(())
    }

    pub fn nb_samples(&self) -> i32 {
        self.frame.samples() as i32
    }

    pub fn set_pts(&mut self, pts: i64) {
        self.frame.set_pts(Some(pts));
    }
}
