use ffmpeg_next::format::Pixel;
use ffmpeg_next::{frame, Error};
use ffmpeg_sys_next::{
    av_frame_alloc, av_frame_free, av_frame_get_buffer, av_frame_make_writable, AVFrame,
    AVPixelFormat,
};

pub struct Picture {
    frame: frame::Video,
}

impl Picture {
    pub fn new(pix_fmt: Pixel, width: u32, height: u32) -> anyhow::Result<Self> {
        let frame = unsafe {
            let mut frame: *mut AVFrame = av_frame_alloc();
            if frame.is_null() {
                anyhow::bail!("Failed to alloc frame: Out of memory");
            }
            (*frame).format = AVPixelFormat::from(pix_fmt) as i32;
            (*frame).width = width as _;
            (*frame).height = height as _;

            let ret = av_frame_get_buffer(frame, 0);

            if ret < 0 {
                av_frame_free((&mut frame) as _);

                let error = Error::from(ret);
                anyhow::bail!("Could not allocate frame data: {}", error);
            }

            frame
        };

        Ok(Self {
            frame: unsafe { frame::Video::wrap(frame) },
        })
    }

    pub fn as_video(&self) -> &frame::Video {
        &self.frame
    }

    pub fn as_video_mut(&mut self) -> &mut frame::Video {
        &mut self.frame
    }

    pub fn format(&self) -> Pixel {
        self.frame.format()
    }

    pub fn width(&self) -> u32 {
        self.frame.width()
    }

    pub fn height(&self) -> u32 {
        self.frame.height()
    }

    pub fn set_pts(&mut self, pts: i64) {
        self.frame.set_pts(Some(pts))
    }

    pub fn make_writable(&mut self) -> anyhow::Result<()> {
        let ret = unsafe { av_frame_make_writable(self.frame.as_mut_ptr()) };
        if ret < 0 {
            anyhow::bail!("Failed to make frame writable: {}", Error::from(ret));
        }
        Ok(())
    }

    pub fn fill(&mut self, frame_index: u32) {
        let frame = unsafe { self.frame.as_mut_ptr() };
        let i = frame_index;

        /* Y */
        for y in 0..self.height() {
            for x in 0..self.width() {
                unsafe {
                    let ls = (*frame).linesize[0] as u32;
                    (*frame).data[0]
                        .offset((y * ls + x) as isize)
                        .write((x + y + i * 3) as u8);
                }
            }
        }

        /* Cb and Cr */
        for y in 0..self.height() / 2 {
            for x in 0..self.width() / 2 {
                unsafe {
                    let ls1 = (*frame).linesize[1] as u32;
                    let ls2 = (*frame).linesize[2] as u32;

                    (*frame).data[1]
                        .offset((y * ls1 + x) as isize)
                        .write((128 + y + i * 2) as u8);
                    (*frame).data[2]
                        .offset((y * ls2 + x) as isize)
                        .write((64 + x + i * 5) as u8);
                }
            }
        }
    }
}
