use crate::{bidir::BidirChannel, rendergl::bindings::gl};
use core::panic;
use glutin::{
    dpi::PhysicalSize,
    event_loop::EventLoop,
    platform::{
        windows::{EventLoopExtWindows, RawHandle},
        ContextTraitExt,
    },
    Context, PossiblyCurrent,
};
use gst_gl::prelude::*;
use gstreamer_gl as gst_gl;
use std::{ffi::CStr, mem, ptr, thread};

#[derive(Debug, Clone, PartialEq)]
pub enum TextureType {
    Mono,
    Lut,
}
#[derive(Debug, Clone)]
pub struct TextureDescription {
    pub handle: TextureHandle,
    pub kind: TextureType,
}
#[derive(Debug)]
struct DataDescription {
    size: (usize, usize),
    data: Vec<u16>,
}
#[derive(Debug)]
struct LoadDescription {
    texture: TextureHandle,
    data: DataDescription,
}
#[derive(Debug)]
pub enum UploaderMessage {
    GetContext,
    SetContextCurrent,
    Context(gst_gl::GLContext),
    LoadData(LoadDescription),
    Texture(Option<TextureHandle>),
    AcquireTexture((usize, usize)),
    ReleaseTexture(TextureHandle),
    Flush,
    Fail(String),
}
// This is not thread safe!!!!!!!!!!! (needs a Mutex if multiple threads should access it)
pub struct ThreadUploader {
    channel: BidirChannel<UploaderMessage>,
}

impl ThreadUploader {
    const LUT_TEXTURE_WIDTH: usize = 256;
    const LUT_TEXTURE_HEIGHT: usize = 256;

    pub fn new() -> Self {
        let (me, other) = BidirChannel::new_pair();
        let _ = thread::spawn(move || Self::thread_func(other));
        Self { channel: me }
    }
    pub fn get_shared_context(&self) -> gst_gl::GLContext {
        self.channel
            .send(UploaderMessage::GetContext)
            .expect("Failed to send GetContext");
        let message = self.channel.recv().expect("Failed to recv context");
        match message {
            UploaderMessage::Context(ctx) => ctx,
            m => panic!("Failed to recieve context got: {:?}", m),
        }
    }
    pub fn set_current(&self) {
        self.channel
            .send(UploaderMessage::SetContextCurrent)
            .expect("Failed to send currenting message");
    }

    pub fn acquire_image_handle(&self, size: (usize, usize)) -> Option<TextureDescription> {
        let (width, height) = Self::alloc_size(size.0, size.1);
        self.channel
            .send(UploaderMessage::AcquireTexture((width, height)))
            .expect("Failed to send aquire message");
        let message = self.channel.recv().expect("Failed to recv message");
        match message {
            UploaderMessage::Texture(handle) => Some(TextureDescription {
                handle: handle?,
                kind: TextureType::Mono,
            }),
            m => panic!("Unexpected message type: {:?}", m),
        }
    }
    pub fn acquire_lut_handle(&self) -> Option<TextureDescription> {
        let (width, height) = (Self::LUT_TEXTURE_WIDTH, Self::LUT_TEXTURE_HEIGHT);
        self.channel
            .send(UploaderMessage::AcquireTexture((width, height)))
            .expect("Failed to send lut aquire message");
        let message = self.channel.recv().expect("Failed to recv message");
        match message {
            UploaderMessage::Texture(handle) => Some(TextureDescription {
                handle: handle?,
                kind: TextureType::Lut,
            }),
            m => panic!("Unexpected message type: {:?}", m),
        }
    }
    fn load_texture(&self, texture: &TextureHandle, size: (usize, usize), data: Vec<u16>) {
        assert!(size.0 <= texture.width && size.1 <= texture.height);
        assert!(size.0 * size.1 == data.len());
        self.channel
            .send(UploaderMessage::LoadData(LoadDescription {
                texture: texture.clone(),
                data: DataDescription { size, data },
            }))
            .expect("Failed to send upload message");
    }
    pub fn load_image(&self, texture: &TextureDescription, size: (usize, usize), data: Vec<u16>) {
        assert!(texture.kind == TextureType::Mono);
        self.load_texture(&texture.handle, size, data);
    }

    pub fn load_lut(&self, texture: &TextureDescription, data: Vec<u16>) {
        assert!(texture.kind == TextureType::Lut);
        self.load_texture(
            &texture.handle,
            (Self::LUT_TEXTURE_WIDTH, Self::LUT_TEXTURE_HEIGHT),
            data,
        );
    }

    pub fn flush(&self) {
        self.channel
            .send(UploaderMessage::Flush)
            .expect("Failed to send flush message");
    }
    pub fn release_texture(&self, texture: TextureHandle) {
        self.channel
            .send(UploaderMessage::ReleaseTexture(texture))
            .expect("Failed to send release message");
    }

    fn alloc_size(width: usize, height: usize) -> (usize, usize) {
        // Compute the smallest power of 2 that contains the larger of width/height
        let max_log2 = (width.max(height) as f32).log2().ceil();
        let max_pow2 = 2.0_f32.powf(max_log2) as usize;
        (max_pow2, max_pow2)
    }

    fn create_main_context() -> (EventLoop<()>, Context<PossiblyCurrent>, gst_gl::GLContext) {
        // Create a new event loop on a thread different from the main thread.
        let event_loop: EventLoop<()> = glutin::event_loop::EventLoop::new_any_thread();
        // let window = glutin::window::WindowBuilder::new().with_title("GL rendering");
        let main_context = glutin::ContextBuilder::new()
            // .with_vsync(true)
            .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (4, 5)))
            .with_gl_profile(glutin::GlProfile::Core)
            .build_headless(&event_loop, PhysicalSize::new(100, 100))
            // .build_windowed(window, &event_loop)
            .expect("Failed to build window");
        let main_context = unsafe {
            main_context
                .make_current()
                .expect("Failed to make context current")
        };
        // Build gstreamer sharable context
        let (gl_context, gl_display, platform) = match unsafe { main_context.raw_handle() } {
            RawHandle::Wgl(wgl_context) => {
                let gl_display = gst_gl::GLDisplay::new();
                (
                    wgl_context as usize,
                    gl_display.upcast::<gst_gl::GLDisplay>(),
                    gst_gl::GLPlatform::WGL,
                )
            }
            #[allow(unreachable_patterns)]
            handler => panic!("Unsupported platform: {:?}.", handler),
        };
        // The shared gstreamer context will be moved into the sync bus handler.
        let shared_context = unsafe {
            gst_gl::GLContext::new_wrapped(
                &gl_display,
                gl_context,
                platform,
                gst_gl::GLAPI::OPENGL3,
            )
        }
        .unwrap();
        shared_context
            .activate(true)
            .expect("Couldn't activate wrapped GL context");
        shared_context
            .fill_info()
            .expect("Failed to fill context info");

        (event_loop, main_context, shared_context)
    }

    fn initial_setup(
        channel: &BidirChannel<UploaderMessage>,
    ) -> (EventLoop<()>, Context<PossiblyCurrent>) {
        let (event_loop, main_context, shared_context) = Self::create_main_context();
        // During setup we uncurrent the main_context and wait for a signal to proceeed
        let main_context = unsafe {
            main_context
                .make_not_current()
                .expect("Failed to uncurrent the main context")
        };
        let message = channel.recv().expect("Failed to get initial setup message");
        match message {
            UploaderMessage::GetContext => channel
                .send(UploaderMessage::Context(shared_context))
                .expect("Failed to reply with shared context"),
            m => panic!("Expected GetContext got: {:?} during initial setup", m),
        };
        // Set the context as current and return
        let message = channel
            .recv()
            .expect("Failed to get signal to current the context");
        let main_context = match message {
            UploaderMessage::SetContextCurrent => unsafe {
                main_context
                    .make_current()
                    .expect("Failed to current the context")
            },
            e => panic!("Expected SetContextCurrent got: {:?}", e),
        };
        (event_loop, main_context)
    }

    fn thread_func(channel: BidirChannel<UploaderMessage>) {
        let (event_loop, main_context) = Self::initial_setup(&channel);
        println!("Initial setup is complete entering dispatcher loop");

        let mut texture_transfer = TextureTransfer::new(main_context);

        loop {
            let message = channel.recv().expect("Failed to recv message in thread");
            let reply = match message {
                UploaderMessage::AcquireTexture(size) => {
                    let texture = texture_transfer.acquire_R16_texture(size.0, size.1);
                    Some(UploaderMessage::Texture(texture))
                }
                UploaderMessage::LoadData(desc) => {
                    texture_transfer.load_R16_texture(
                        desc.texture,
                        desc.data.size.0,
                        desc.data.size.1,
                        &desc.data.data,
                    );
                    None
                }
                UploaderMessage::ReleaseTexture(handle) => {
                    texture_transfer.release_texture(handle);
                    None
                }
                UploaderMessage::Flush => {
                    texture_transfer.flush();
                    None
                }
                _ => panic!("Unexpected message type in main loop!"),
            };

            if let Some(reply) = reply {
                channel.send(reply).expect("Failed to send reply");
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextureHandle {
    pub id: u32,
    pub width: usize,
    pub height: usize,
}
struct TextureTransfer {
    _ctx: glutin::Context<PossiblyCurrent>, // Need to keep a ref to the context otherwise it gets deleted since it is moved in the new() method
    bindings: gl::Gl,
}

impl TextureTransfer {
    pub fn new(ctx: glutin::Context<PossiblyCurrent>) -> Self {
        let bindings = gl::Gl::load_with(|name| ctx.get_proc_address(name) as _);
        println!("Loaded bindings for main context");
        Self {
            bindings,
            _ctx: ctx,
        }
    }

    unsafe fn create_R16_texture(&self, width: usize, height: usize) -> TextureHandle {
        let mut texture_id = mem::MaybeUninit::uninit();
        self.bindings.GenTextures(1, texture_id.as_mut_ptr());
        let texture_id = texture_id.assume_init();
        self.bindings.BindTexture(gl::TEXTURE_2D, texture_id);
        // Set texture filter params
        self.bindings
            .TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
        self.bindings
            .TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
        self.bindings
            .TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
        self.bindings
            .TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
        // Create the Texture object empty
        self.bindings.TexImage2D(
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
        self.bindings.BindTexture(gl::TEXTURE_2D, 0);

        TextureHandle {
            id: texture_id,
            width,
            height,
        }
    }

    fn acquire_R16_texture(&mut self, width: usize, height: usize) -> Option<TextureHandle> {
        // Maybe check for memory contraints, but for now we just allocate.
        unsafe { Some(self.create_R16_texture(width, height)) }
    }

    fn load_R16_texture(
        &self,
        texture: TextureHandle,
        width: usize,
        height: usize,
        image_data: &[u16],
    ) {
        assert!(width <= texture.width && height <= texture.height);
        assert!(image_data.len() == width * height);
        unsafe {
            self.bindings.TextureSubImage2D(
                texture.id,
                0,
                0,
                0,
                width as _,
                height as _,
                gl::RED,
                gl::UNSIGNED_SHORT,
                image_data.as_ptr() as _,
            );
        }
    }

    fn release_texture(&self, texture: TextureHandle) {
        unsafe {
            let texture_id = mem::MaybeUninit::new(texture.id);
            self.bindings.DeleteTextures(1, texture_id.as_ptr());
        }
    }

    pub fn flush(&self) {
        unsafe {
            // Make sure to flush the command queue
            self.bindings.Flush();
        }
    }
}
