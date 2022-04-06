#![allow(dead_code)]
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame::Video;
use ffmpeg_next::Error;
use ffmpeg_sys_next::{av_freep, av_image_alloc, av_image_copy, AVFrame, AVPixelFormat};
use std::os::raw::c_int;

pub struct Image {
    data: [*mut u8; 4],
    line_size: [c_int; 4],
    width: u32,
    height: u32,
    size: usize,
    pixel: AVPixelFormat,
    align: u32,
}

impl Image {
    pub fn new(
        width: u32,
        height: u32,
        pixel: Pixel,
        align: u32,
    ) -> Result<Self, ffmpeg_next::Error> {
        let pix_fmt = pixel.into();

        let mut result = Self {
            data: [0 as *mut u8; 4],
            line_size: [0; 4],
            width,
            height,
            size: 0,
            pixel: pix_fmt,
            align,
        };

        let ret = unsafe {
            av_image_alloc(
                result.data.as_mut_ptr(),
                result.line_size.as_mut_ptr(),
                width as c_int,
                height as c_int,
                pix_fmt,
                align as c_int,
            )
        };

        if ret <= 0 {
            return Err(Error::from(ret));
        }

        result.size = ret as usize;
        Ok(result)
    }

    pub fn data(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.data[0], self.size) }
    }

    pub fn copy_from_video(&mut self, video: &Video) {
        unsafe {
            let src: *mut AVFrame = video.as_ptr() as _;
            let src_data = (*src).data.as_mut_ptr() as _;
            let src_line_size = (*src).linesize.as_mut_ptr();
            av_image_copy(
                self.data.as_mut_ptr(),
                self.line_size.as_mut_ptr(),
                src_data,
                src_line_size,
                self.pixel,
                self.width as _,
                self.height as _,
            );
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn pixel(&self) -> Pixel {
        Pixel::from(self.pixel)
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn align(&self) -> u32 {
        self.align
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            av_freep(self.data.as_mut_ptr() as _);
        }
    }
}
