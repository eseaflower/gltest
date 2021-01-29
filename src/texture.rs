use crate::{main, rendergl::bindings::gl};
use glutin::PossiblyCurrent;
use std::{ffi::CStr, mem, ptr};

// TODO: Create a TextureDesc and LutDesc that contains id:s and width/height,
#[derive(Debug, Clone, PartialEq)]
pub enum LeaseState {
    Free,
    Leased,
}
#[derive(Debug, Clone, PartialEq)]
pub enum TextureType {
    Mono,
    Lut,
}
#[derive(Debug, Clone)]
pub struct TextureDescription {
    pub id: u32,
    pub width: usize,
    pub height: usize,
    pub state: LeaseState,
    pub kind: TextureType,
}
impl TextureDescription {
    pub fn contains(&self, size: (usize, usize)) -> bool {
        size.0 <= self.width && size.1 <= self.height
    }
}
pub struct TextureTransfer {
    _ctx: glutin::Context<PossiblyCurrent>, // Need to keep a ref to the context otherwise it gets deleted since it is moved in the new() method
    bindings: gl::Gl,
    image: Vec<TextureDescription>,
    lut: Vec<TextureDescription>,
}

impl TextureTransfer {
    const DEFAULT_TEXTURE_WIDTH: usize = 256;
    const DEFAULT_TEXTURE_HEIGHT: usize = 256;
    const DEFAULT_LUT_TEXTURE_WIDTH: usize = 256;
    const DEFAULT_LUT_TEXTURE_HEIGHT: usize = 256;

    pub fn new(ctx: glutin::Context<PossiblyCurrent>) -> Self {
        let bindings = gl::Gl::load_with(|name| ctx.get_proc_address(name) as _);
        println!("Loaded bindings for main context");

        Self {
            bindings,
            image: Vec::new(),
            lut: Vec::new(),
            _ctx: ctx,
        }
    }

    unsafe fn create_mono_texture(
        bindings: &gl::Gl,
        width: usize,
        height: usize,
        kind: TextureType,
    ) -> TextureDescription {
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

        TextureDescription {
            width,
            height,
            id: texture_id,
            state: LeaseState::Free,
            kind,
        }
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

    fn alloc_size(width: usize, height: usize) -> (usize, usize) {
        // Compute the smallest power of 2 that contains the larger of width/height
        let max_log2 = (width.max(height) as f32).log2().ceil();
        let max_pow2 = 2.0_f32.powf(max_log2) as usize;
        (max_pow2, max_pow2)
    }

    fn acquire_texture(
        &mut self,
        width: usize,
        height: usize,
        kind: TextureType,
    ) -> Option<TextureDescription> {
        // Check which kind to allocate
        let collection = match kind {
            TextureType::Mono => &mut self.image,
            TextureType::Lut => &mut self.lut,
        };
        // Check available image textures
        let texture = collection.iter_mut().find(|t| t.contains((width, height)));
        let texture = match texture {
            Some(texture) => texture,
            None => {
                let (alloc_width, alloc_height) = Self::alloc_size(width, height);
                println!(
                    "Allocating new texture of size: {}x{}",
                    alloc_width, alloc_height
                );
                let texture = unsafe {
                    // Since we have a weired control flow here we can't borrow self again (since we
                    // have a mutable borrow from the top). This is why we can't have create_mono_texture
                    // as a member function. (The borrow of &self.bindings the borrow-checker can figure
                    // out does not overlap with the mutable borrow of the image/lut collections.)
                    Self::create_mono_texture(&self.bindings, alloc_width, alloc_height, kind)
                };
                collection.push(texture);
                collection.last_mut().unwrap()
            }
        };
        match texture.state {
            LeaseState::Free => {
                texture.state = LeaseState::Leased;
                Some(texture.clone())
            }
            LeaseState::Leased => None,
        }
    }

    fn acquire_mono_image(&mut self, width: usize, height: usize) -> Option<TextureDescription> {
        self.acquire_texture(width, height, TextureType::Mono)
    }

    fn acquire_lut(&mut self) -> Option<TextureDescription> {
        self.acquire_texture(
            Self::DEFAULT_LUT_TEXTURE_WIDTH,
            Self::DEFAULT_LUT_TEXTURE_HEIGHT,
            TextureType::Lut,
        )
    }

    pub fn load_image(
        &mut self,
        size: (usize, usize),
        image_data: &[u16],
    ) -> Option<TextureDescription> {
        let texture = self.acquire_mono_image(size.0, size.1)?; // Return None if we can't acquire the texture
        assert!(size.0 <= texture.width && size.1 <= texture.height);
        assert!(texture.kind == TextureType::Mono);
        unsafe {
            self.bindings.TextureSubImage2D(
                texture.id,
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
        Some(texture)
    }
    pub fn load_lut(&mut self, lut_data: &[u16]) -> Option<TextureDescription> {
        let texture = self.acquire_lut()?;
        assert!(lut_data.len() == texture.width * texture.height); // We can only handle 16-bit LUTs
        assert!(texture.kind == TextureType::Lut);
        unsafe {
            self.bindings.TextureSubImage2D(
                texture.id,
                0,
                0,
                0,
                texture.width as _,
                texture.height as _,
                gl::RED,
                gl::UNSIGNED_SHORT,
                lut_data.as_ptr() as _,
            );
        };
        Some(texture)
    }

    pub fn release_texture(&mut self, texture: TextureDescription) {
        let collection = match texture.kind {
            TextureType::Mono => &mut self.image,
            TextureType::Lut => &mut self.lut,
        };
        // Find the corresponding entry in the collection based on id
        let texture = collection.iter_mut().find(|t| t.id == texture.id);
        if let Some(texture) = texture {
            assert!(texture.state == LeaseState::Leased);
            println!("Releasing texture {:?}", texture);
            texture.state = LeaseState::Free;
        }
    }

    pub fn flush(&self) {
        unsafe {
            // Make sure to flush the command queue
            self.bindings.Flush();
        }
    }
}
