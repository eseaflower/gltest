use crate::rendergl::bindings::gl;
use glutin::PossiblyCurrent;
use std::{ffi::CStr, mem, ptr};


// TODO: Create a TextureDesc and LutDesc that contains id:s and width/height,


pub struct TextureTransfer {
    _ctx: glutin::Context<PossiblyCurrent>, // Need to keep a ref to the context otherwise it gets deleted since it is moved in the new() method
    bindings: gl::Gl,
    image_id: u32,
    lut_id: u32,
}

impl TextureTransfer {
    const DEFAULT_TEXTURE_WIDTH: usize = 256;
    const DEFAULT_TEXTURE_HEIGHT: usize = 256;
    const DEFAULT_LUT_TEXTURE_WIDTH: usize = 256;
    const DEFAULT_LUT_TEXTURE_HEIGHT: usize = 256;

    pub fn new(ctx: glutin::Context<PossiblyCurrent>) -> Self {
        let bindings = gl::Gl::load_with(|name| ctx.get_proc_address(name) as _);
        println!("Loaded bindings for main context");
        unsafe {
            let image_id = Self::create_mono_texture(
                &bindings,
                Self::DEFAULT_TEXTURE_WIDTH,
                Self::DEFAULT_TEXTURE_HEIGHT,
            );
            let lut_id = Self::create_mono_texture(
                &bindings,
                Self::DEFAULT_LUT_TEXTURE_WIDTH,
                Self::DEFAULT_LUT_TEXTURE_HEIGHT,
            );

            Self {
                bindings,
                image_id,
                lut_id,
                _ctx: ctx,
            }
        }
    }
    unsafe fn create_mono_texture(bindings: &gl::Gl, width: usize, height: usize) -> u32 {
        let mut texture_id = mem::MaybeUninit::uninit();
        bindings.GenTextures(1, texture_id.as_mut_ptr());
        let texture_id = texture_id.assume_init();
        bindings.BindTexture(gl::TEXTURE_2D, texture_id);
        // Set texture filter params
        bindings.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
        bindings.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
        bindings.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
        bindings.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
        // Create the Texture object empty
        bindings.TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::R16 as _,
            width as _,
            height as _,
            0,
            gl::RED,
            gl::UNSIGNED_SHORT,
            ptr::null(),
        );
        bindings.BindTexture(gl::TEXTURE_2D, 0);
        texture_id
    }
    pub fn print_version(&self) {
        unsafe {
            let version = {
                let data = CStr::from_ptr(self.bindings.GetString(gl::VERSION) as *const _)
                    .to_bytes()
                    .to_vec();
                String::from_utf8(data).unwrap()
            };
            println!("Version (func) is: {}", version);
        };
    }
    pub fn get_ids(&self) -> (u32, u32) {
        (self.image_id, self.lut_id)
    }

    pub fn load_image(&self, size: (usize, usize), image_data: &[u16]) {
        unsafe {
            self.bindings.TextureSubImage2D(
                self.image_id,
                0,
                0,
                0,
                size.0 as _,
                size.1 as _,
                gl::RED,
                gl::UNSIGNED_SHORT,
                image_data.as_ptr() as _,
            );
        }
    }
    pub fn load_lut(&self, lut_data: &[u16]) {
        assert!(lut_data.len() == 256 * 256); // We can only handle 16-bit LUTs
        unsafe {
            self.bindings.TextureSubImage2D(
                self.lut_id,
                0,
                0,
                0,
                256,
                256,
                gl::RED,
                gl::UNSIGNED_SHORT,
                lut_data.as_ptr() as _,
            );
        }
    }

    pub fn flush(&self) {
        unsafe {
            // Make sure to flush the command queue
            self.bindings.Flush();
        }
    }
}
