use ffmpeg_next::format::Pixel;
use ffmpeg_next::{frame, Error};
use ffmpeg_sys_next::{
    av_frame_alloc, av_frame_free, av_frame_get_buffer, av_frame_make_writable, AVFrame,
    AVPixelFormat,
};
use std::mem::transmute;

pub struct Picture {
    frame: *mut AVFrame,
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

        Ok(Self { frame })
    }

    pub fn as_video(&self) -> frame::Video {
        unsafe { frame::Video::wrap(self.frame as _) }
    }

    pub fn format(&self) -> Pixel {
        unsafe { transmute::<i32, AVPixelFormat>((*self.frame).format) }.into()
    }

    pub fn width(&self) -> u32 {
        unsafe { (*self.frame).width as _ }
    }

    pub fn height(&self) -> u32 {
        unsafe { (*self.frame).height as _ }
    }

    pub fn set_pts(&mut self, pts: i64) {
        unsafe { (*self.frame).pts = pts }
    }

    pub fn make_writable(&mut self) -> anyhow::Result<()> {
        let ret = unsafe { av_frame_make_writable(self.frame) };
        if ret < 0 {
            anyhow::bail!("Failed to make frame writable: {}", Error::from(ret));
        }
        Ok(())
    }

    pub fn fill(&mut self, frame_index: u32) {
        let i = frame_index;

        /* Y */
        for y in 0..self.height() {
            for x in 0..self.width() {
                unsafe {
                    let ls = (*self.frame).linesize[0] as u32;
                    (*self.frame).data[0]
                        .offset((y * ls + x) as isize)
                        .write((x + y + i * 3) as u8);
                }
            }
        }

        /* Cb and Cr */
        for y in 0..self.height() / 2 {
            for x in 0..self.width() / 2 {
                unsafe {
                    let ls1 = (*self.frame).linesize[1] as u32;
                    let ls2 = (*self.frame).linesize[2] as u32;

                    (*self.frame).data[1]
                        .offset((y * ls1 + x) as isize)
                        .write((128 + y + i * 2) as u8);
                    (*self.frame).data[2]
                        .offset((y * ls2 + x) as isize)
                        .write((64 + x + i * 5) as u8);
                }
            }
        }
    }
}

impl Drop for Picture {
    fn drop(&mut self) {
        unsafe { av_frame_free((&mut self.frame) as _) }
    }
}
