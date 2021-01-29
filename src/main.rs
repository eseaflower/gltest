mod bidir;
mod gstrender;
mod rendergl;
mod texture;

use bidir::BidirChannel;
use glib::Value;
use glutin::{PossiblyCurrent, dpi::PhysicalSize, event::Event, event_loop::{ControlFlow, EventLoop}, platform::{run_return::EventLoopExtRunReturn, windows::RawHandle, ContextTraitExt}};
use gst::prelude::*;
use gst_gl::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_gl as gst_gl;
use gstreamer_video as gst_video;
use gstrender::{GstRenderStruct, RenderMessage};
use rendergl::{vertex::Quad, view_state::ViewState};
use core::time;
use std::{
    sync::{
        mpsc::{self, Receiver},
        Mutex,
    },
    time::Duration,
};
use texture::TextureTransfer;

const IMAGE_WIDTH: usize = 256;
const IMAGE_HEIGHT: usize = 256;
pub fn generate_texture_data(f: f32) -> Vec<u16> {
    let data_size = IMAGE_WIDTH * IMAGE_HEIGHT;
    let mut data = vec![0_u16; data_size];
    for (y, line) in data.chunks_mut(IMAGE_WIDTH).enumerate() {
        let line_color = ((y as f32 * f / IMAGE_HEIGHT as f32) * u16::MAX as f32) as u16;
        for pixel in line.iter_mut() {
            *pixel = line_color;
        }
    }
    data
}

pub fn generate_lut_data() -> Vec<u16> {
    let data_size = 256 * 256;
    let mut data = vec![0_u16; data_size];
    for (i, entry) in data.iter_mut().enumerate() {
        let sv = (i) % u16::MAX as usize;
        *entry = sv as u16;
    }
    data
}
fn create_from_element(
    element: gst::Element,
    channel: BidirChannel<RenderMessage>,
) -> GstRenderStruct {
    // We assume the element has a 'context' property which is the GLContext
    let ctx = element
        .get_property("context")
        .expect("No property 'context' found")
        .get()
        .expect("Failed to convert to GLContext")
        .expect("Context is None");
    GstRenderStruct::new(ctx, channel)
}

fn setup_filterapp(filterapp: gst::Element, channel: BidirChannel<RenderMessage>) {
    let time = Mutex::new(std::time::Instant::now());
    let renderer: Mutex<Option<GstRenderStruct>> = Mutex::new(None);
    let channel = Mutex::new(Some(channel));
    filterapp
        .connect("client-draw", false, move |_vals| {
            // let tex_id = _vals[1].get::<u32>().unwrap().unwrap();
            // println!("Texture id: {}", tex_id);
            // let tex_width = _vals[2].get::<u32>().unwrap().unwrap();
            // println!("Texture width: {}", tex_width);
            // let tex_height = _vals[3].get::<u32>().unwrap().unwrap();
            // println!("Texture height: {}", tex_height);

            let mut renderer = renderer.lock().unwrap();
            let renderer = match *renderer {
                Some(ref r) => r,
                None => {
                    let filter_element = _vals[0]
                        .get::<gst::Element>()
                        .expect("Failed to get Element")
                        .expect("Value is None");
                    let name = filter_element.get_name().to_string();
                    println!("Name of element: {}", &name);
                    // UGLY HACK: The closure is Send + Sync, which means we can't use the Receiver
                    // but we want to move it into GstRenderStruct.
                    let channel = channel
                        .lock()
                        .unwrap()
                        .take()
                        .expect("Can only cretae GstRenderStruct once");

                    *renderer = Some(create_from_element(filter_element, channel));
                    renderer.as_ref().unwrap()
                }
            };

            let mut time = time.lock().unwrap();
            let el = time.elapsed().as_millis();
            *time = std::time::Instant::now();

            println!("Got draw signal: {} ms", el);

            unsafe { renderer.draw() };

            Some(Value::from(&true))
        })
        .expect("Failed to connect signal handler");
}

const TX_WIDTH: u32 = 1;
const TX_HEIGHT: u32 = 1;
const BUF_SIZE: usize = (TX_WIDTH * TX_HEIGHT * 4) as usize; // Size of one buffer (Assuming 4 channels RGBA)
const FPS: u32 = 5;

fn setup_appsrc(appsrc: &gst_app::AppSrc) {
    let video_info =
        gst_video::VideoInfo::builder(gst_video::VideoFormat::Rgba, TX_WIDTH, TX_HEIGHT)
            .fps(FPS as i32)
            .build()
            .expect("Failed to build video_info");
    appsrc.set_caps(Some(
        &video_info
            .to_caps()
            .expect("Failed to convert info to caps"),
    ));
}

fn setup_context_sharing(bus: &gst::Bus) -> (EventLoop<()>, glutin::Context<PossiblyCurrent>) {
    let event_loop: EventLoop<()> = glutin::event_loop::EventLoop::new();
    // let window = glutin::window::WindowBuilder::new().with_title("GL rendering");
    let windowed_context = glutin::ContextBuilder::new()
        // .with_vsync(true)
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (4, 5)))
        .with_gl_profile(glutin::GlProfile::Core)
        .build_headless(&event_loop, PhysicalSize::new(100, 100))
        // .build_windowed(window, &event_loop)
        .expect("Failed to build window");
    let windowed_context = unsafe {
        windowed_context
            .make_current()
            .expect("Failed to make context current")
    };
    // Build gstreamer sharable context
    let (gl_context, gl_display, platform) = match unsafe { windowed_context.raw_handle() } {
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
        gst_gl::GLContext::new_wrapped(&gl_display, gl_context, platform, gst_gl::GLAPI::OPENGL3)
    }
    .unwrap();

    shared_context
        .activate(true)
        .expect("Couldn't activate wrapped GL context");

    shared_context
        .fill_info()
        .expect("Failed to fill context info");

    #[allow(clippy::single_match)]
    bus.set_sync_handler(move |_, msg| {
        match msg.view() {
            gst::MessageView::NeedContext(ctxt) => {
                println!("Got context message");
                let context_type = ctxt.get_context_type();
                if context_type == *gst_gl::GL_DISPLAY_CONTEXT_TYPE {
                    println!("Ignoring display");
                    // if let Some(el) = msg.get_src().map(|s| s.downcast::<gst::Element>().unwrap()) {
                    //     println!("Display context");
                    //     let context = gst::Context::new(context_type, true);
                    //     context.set_gl_display(&gl_display);
                    //     el.set_context(&context);
                    // }
                }
                if context_type == "gst.gl.app_context" {
                    if let Some(el) = msg.get_src().map(|s| s.downcast::<gst::Element>().unwrap()) {
                        println!("App context");
                        let mut context = gst::Context::new(context_type, true);
                        {
                            let context = context.get_mut().unwrap();
                            let s = context.get_mut_structure();
                            s.set("context", &shared_context);
                        }
                        el.set_context(&context);
                    }
                }
            }
            _ => (),
        }

        gst::BusSyncReply::Pass
    });
    (event_loop, windowed_context)
}

fn main() {
    gst::init().expect("GStreamer is installed");
    // let pipeline =
    //     gst::parse_launch("videotestsrc ! glupload ! glfilterapp name=filterapp ! glimagesink")
    //         .expect("Pipeline parsed ok");
    let pipeline = gst::parse_launch(
        "appsrc name=app is-live=true min-latency=0 format=time block=true do-timestamp=true !
        glupload !
        glfilterapp name=filterapp ! video/x-raw(memory:GLMemory), width=256, height=256 !
        glimagesink",
    )
    .expect("Pipeline parsed ok");

    let pipeline = pipeline
        .dynamic_cast::<gst::Pipeline>()
        .expect("Should be a pipeline element");
    let bus = pipeline.get_bus().expect("Bus is present");

    let (mut event_loop, window_context) = setup_context_sharing(&bus);
    println!("Context sharing setup");

    // We should now be able to get the filterapp and its GLContext
    let filterapp = pipeline
        .get_by_name("filterapp")
        .expect("Failed to find filterapp-name");

    // Create a bidirectional channel to communicate with the render thread.
    let (channel, other) = BidirChannel::new_pair();
    setup_filterapp(filterapp, other);

    let appsrc = pipeline
        .get_by_name("app")
        .expect("Failed to find 'app'")
        .dynamic_cast::<gst_app::AppSrc>()
        .expect("Failed to cast to AppSrc");
    setup_appsrc(&appsrc);

    // Uncurrent the "main" GL context and start the pipeline
    // The set to current again once the contexts have been shared.
    let window_context = unsafe {
        window_context
            .make_not_current()
            .expect("Failed to uncurrent the window context")
    };

    pipeline
        .set_state(gst::State::Paused)
        .expect("Failed to set the pipeline to paused");
    let (result, _s1, _s2) = pipeline.get_state(gst::ClockTime::none());
    println!("In paused state?");
    match result {
        Ok(_) => {
            println!("Yaya");
        }
        Err(e) => {
            println!("Fail: {:?}", e);
        }
    }
    let window_context = unsafe {
        window_context
            .make_current()
            .expect("Failed to recurrent the window context")
    };
    // Create a texture transfer struct.
    let mut texture_transfer = TextureTransfer::new(window_context);
    println!("Created texture transfer struct");

    pipeline
        .set_state(gst::State::Playing)
        .expect("Pipeline should be playable");

    let mut last_time = std::time::Instant::now();
    let target_sleep = 1000 / FPS;

    texture_transfer.print_version();

    let mut q = Quad::with_init((256_f32, 256_f32));
    let mut state = ViewState::new();
    state.update_magnification(0.5);

    // let mut timer = std::time::Instant::now();
    // event_loop.run(move |ev, t, flow| {
    //     *flow = ControlFlow::Poll;
    //     println!("Loop time: {} ms", timer.elapsed().as_millis());
    //     timer = std::time::Instant::now();
    //     spin_sleep::sleep(Duration::from_millis(10));
    // });

    // todo!();


    'main_loop: loop {
        // let pt = std::time::Instant::now();
        // event_loop.run_return(|_el, _t, flow| {
        //     println!("Inside message handler {:?}", _el);
        //     // Just make sure to pop off any window messages, we really don't care
        //     *flow = glutin::event_loop::ControlFlow::Exit;
        // });
        // println!("WinMsg took {} ms", pt.elapsed().as_millis());

        let now = std::time::Instant::now();
        let sleep_time = target_sleep as i32 - (now - last_time).as_millis() as i32;
        println!("Sleeping for: {}", sleep_time);
        let sleep_time = sleep_time.max(0) as u64;
        // std::thread::sleep(Duration::from_millis(sleep_time));
        spin_sleep::sleep(Duration::from_millis(sleep_time));
        last_time = std::time::Instant::now();

        // Start by looking for messages from the render thread
        if let Ok(from_renderer) = channel.try_recv() {
            // This signals that the renderer has completed this message

            // NOTE: We should perhaps not release the texture directly
            // we might do several renders on the same image!
            texture_transfer.release_texture(from_renderer.image_texture);
            texture_transfer.release_texture(from_renderer.lut_texture);
        }

        // This simulates that we actually should load new texture data
        let image_data = generate_texture_data(1.0);
        let lut_data = generate_lut_data();

        // Try to get a texture to use for upload
        let image_texture = loop {
            if let Some(texture) =
                texture_transfer.load_image((IMAGE_WIDTH, IMAGE_HEIGHT), &image_data)
            {
                break texture;
            }
            let from_renderer = channel.recv().expect("Failed to get message from renderer");
            texture_transfer.release_texture(from_renderer.image_texture);
            texture_transfer.release_texture(from_renderer.lut_texture);
        };
        let lut_texture = loop {
            if let Some(texture) = texture_transfer.load_lut(&lut_data) {
                break texture;
            }
            let from_renderer = channel.recv().expect("Failed to get message from renderer");
            texture_transfer.release_texture(from_renderer.image_texture);
            texture_transfer.release_texture(from_renderer.lut_texture);
        };

        texture_transfer.flush(); // Important to flush!

        // Remap the texture coordinates if we have changed texture size
        q.map_texture_coords(
            (IMAGE_WIDTH as f32, IMAGE_HEIGHT as f32),
            (image_texture.width as f32, image_texture.height as f32),
        );
        let vertex_data = q.get_vertex(&state);

        // Simulate the upload of the image texture.
        channel
            .send(RenderMessage {
                image_texture,
                lut_texture,
                vertex_data,
            })
            .expect("Failed to send textures");

        // Create a "fake" buffer and send down the pipeline
        let mut buffer = gst::Buffer::with_size(BUF_SIZE).expect("Failed to allocate new buffer");

        let buffer_ref = buffer.get_mut().expect("Failed to get BufferRef");
        gst_video::video_meta::VideoMeta::add(
            buffer_ref,
            gst_video::VideoFrameFlags::empty(),
            gst_video::VideoFormat::Rgba,
            TX_WIDTH,
            TX_HEIGHT,
        )
        .expect("Failed to add video meta to buffer");
        let _ = appsrc
            .push_buffer(buffer)
            .expect("Failed to push buffer to appsrc");

        for msg in bus.iter_filtered(&[gst::MessageType::Error]) {
            match msg.view() {
                gst::MessageView::Error(_) => {
                    println!("Error in pipeline");
                    break 'main_loop;
                }
                _ => {
                    println!("Should this message be here?");
                }
            }
        }
    }
    pipeline.send_event(gst::event::Eos::new());
    pipeline
        .set_state(gst::State::Null)
        .expect("Deallocating pipeline");
}
