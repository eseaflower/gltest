use glib::Value;
use gst::prelude::*;
use gst_gl::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_gl as gst_gl;
use gstreamer_video as gst_video;
use std::{mem, ptr, sync::Mutex, time::Duration};

#[allow(clippy::unreadable_literal)]
#[allow(clippy::unused_unit)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::manual_non_exhaustive)]
mod gl {
    pub use self::Gl as MyGl;
    include!("../bindings/test_gl_bindings.rs");
}

const VS_SRC: &[u8] = b"
#version 450

layout(location=0) in vec3 a_pos;

void main() {
    gl_Position = vec4(a_pos, 1.0);
}
\0";

const FS_SRC: &[u8] = b"
#version 450

layout(location=0) out vec4 f_color;

void main() {
    f_color = vec4(0.0, 1.0, 0.0, 1.0);
}
\0";

#[rustfmt::skip]
static VERTICES: [f32; 12] = [
    -0.5_f32, -0.5_f32, 0.0_f32,
    0.5_f32, -0.50_f32, 0.0_f32,
    -0.5_f32, 0.5_f32, 0.0_f32,
    0.5_f32, 0.5_f32, 0.0_f32,
];

static INDICES: [u16; 6] = [0, 1, 2, 1, 3, 2];

struct GstRenderStruct {
    bindings: gl::MyGl,
    program: gl::types::GLuint,
    vao: gl::types::GLuint,
    vertex_buffer: gl::types::GLuint,
    index_buffer: gl::types::GLuint,
}

impl GstRenderStruct {
    fn new(context: gst_gl::GLContext) -> Self {
        let bindings = gl::Gl::load_with(|name| context.get_proc_address(name) as *const _);
        println!("Loaded bindings in context");
        let (program, vao, vertex_buffer, index_buffer) =
            unsafe { Self::setup_context_resources(&bindings) };
        Self {
            bindings,
            program,
            vao,
            vertex_buffer,
            index_buffer,
        }
    }
    unsafe fn setup_context_resources(
        bindings: &gl::MyGl,
    ) -> (
        gl::types::GLuint,
        gl::types::GLuint,
        gl::types::GLuint,
        gl::types::GLuint,
    ) {
        let vs = bindings.CreateShader(gl::VERTEX_SHADER);
        bindings.ShaderSource(vs, 1, [VS_SRC.as_ptr() as *const _].as_ptr(), ptr::null());
        bindings.CompileShader(vs);

        let fs = bindings.CreateShader(gl::FRAGMENT_SHADER);
        bindings.ShaderSource(fs, 1, [FS_SRC.as_ptr() as *const _].as_ptr(), ptr::null());
        bindings.CompileShader(fs);

        let program = bindings.CreateProgram();
        bindings.AttachShader(program, vs);
        bindings.AttachShader(program, fs);
        bindings.LinkProgram(program);

        {
            let mut success: gl::types::GLint = 1;
            bindings.GetProgramiv(fs, gl::LINK_STATUS, &mut success);
            assert!(success != 0);
        }
        bindings.DetachShader(program, vs);
        bindings.DeleteShader(vs);
        bindings.DetachShader(program, fs);
        bindings.DeleteShader(fs);

        // Generate Vertex Array Object, this stores buffers/pointers/indexes
        let mut vao = mem::MaybeUninit::uninit();
        bindings.GenVertexArrays(1, vao.as_mut_ptr());
        let vao = vao.assume_init();
        // Bind the VAO (it "records" which buffers to use to draw)
        bindings.BindVertexArray(vao);

        // Create Vertex Buffer
        let mut vertex_buffer = mem::MaybeUninit::uninit();
        bindings.GenBuffers(1, vertex_buffer.as_mut_ptr());
        let vertex_buffer = vertex_buffer.assume_init();
        bindings.BindBuffer(gl::ARRAY_BUFFER, vertex_buffer);
        bindings.BufferData(
            gl::ARRAY_BUFFER,
            (VERTICES.len() * mem::size_of::<f32>()) as _,
            VERTICES.as_ptr() as _,
            gl::STATIC_DRAW,
        );

        // Create Index Buffer
        let mut index_buffer = mem::MaybeUninit::uninit();
        bindings.GenBuffers(1, index_buffer.as_mut_ptr());
        let index_buffer = index_buffer.assume_init();
        bindings.BindBuffer(gl::ELEMENT_ARRAY_BUFFER, index_buffer);
        bindings.BufferData(
            gl::ELEMENT_ARRAY_BUFFER,
            (INDICES.len() * mem::size_of::<u16>()) as _,
            INDICES.as_ptr() as _,
            gl::STATIC_DRAW,
        );
        // Setup attribute pointers while the VAO is bound to record this.

        // The position is in layout=0 in the shader
        bindings.VertexAttribPointer(
            0,
            3,
            gl::FLOAT,
            gl::FALSE,
            (3 * mem::size_of::<f32>()) as _,
            ptr::null(),
        );

        // Enable attribute 0
        bindings.EnableVertexAttribArray(0);

        // Unbind the VAO BEFORE! unbinding the vertex- and index-buffers
        bindings.BindVertexArray(0);
        bindings.BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
        bindings.BindBuffer(gl::ARRAY_BUFFER, 0);
        bindings.DisableVertexAttribArray(0);
        (program, vao, vertex_buffer, index_buffer)
    }

    unsafe fn draw(&self) {
        self.clear();
        self.bindings.UseProgram(self.program); // Use our shaders
        self.bindings.BindVertexArray(self.vao); // Bind the state stored in the VAO

        self.bindings
            .DrawElements(gl::TRIANGLES, 6, gl::UNSIGNED_SHORT, ptr::null());

        // Unbind resources
        self.bindings.BindVertexArray(0);
    }

    unsafe fn clear(&self) {
        self.bindings.ClearColor(1.0, 0.0, 0.0, 1.0);
        self.bindings.Clear(gl::COLOR_BUFFER_BIT);
    }
}

fn create_from_element(element: gst::Element) -> GstRenderStruct {
    // We assume the element has a 'context' property which is the GLContext
    let ctx = element
        .get_property("context")
        .expect("No property 'context' found")
        .get()
        .expect("Failed to convert to GLContext")
        .expect("Context is None");
    GstRenderStruct::new(ctx)
}

fn setup_filterapp(filterapp: gst::Element) {
    let time = Mutex::new(std::time::Instant::now());
    let renderer: Mutex<Option<GstRenderStruct>> = Mutex::new(None);
    filterapp
        .connect("client-draw", false, move |_vals| {
            let tex_id = _vals[1].get::<u32>().unwrap().unwrap();
            println!("Texture id: {}", tex_id);
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
                    *renderer = Some(create_from_element(filter_element));
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

const TX_WIDTH: u32 = 8;
const TX_HEIGHT: u32 = 8;
const BUF_SIZE: usize = (TX_WIDTH * TX_HEIGHT * 4) as usize; // Size of one buffer (Assuming 4 channels RGBA)
const FPS: u32 = 10;

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

    // We should now be able to get the filterapp and its GLContext
    let filterapp = pipeline
        .get_by_name("filterapp")
        .expect("Failed to find filterapp-name");
    setup_filterapp(filterapp);

    let appsrc = pipeline
        .get_by_name("app")
        .expect("Failed to find 'app'")
        .dynamic_cast::<gst_app::AppSrc>()
        .expect("Failed to cast to AppSrc");
    setup_appsrc(&appsrc);

    // Uncurrent the "main" GL context and start the pipeline
    // The set to current again once the contexts have been shared.

    pipeline
        .set_state(gst::State::Playing)
        .expect("Pipeline should be playable");

    let mut last_time = std::time::Instant::now();
    let target_sleep = 1000 / FPS;
    'main_loop: loop {
        let now = std::time::Instant::now();
        let sleep_time = target_sleep as i32 - (now - last_time).as_millis() as i32;
        println!("Sleeping for: {}", sleep_time);
        let sleep_time = sleep_time.max(0) as u64;
        // std::thread::sleep(Duration::from_millis(sleep_time));
        spin_sleep::sleep(Duration::from_millis(sleep_time));
        last_time = std::time::Instant::now();

        // Create a "fake" buffer and send down the pipeline
        // let mut buffer = gst::Buffer::with_size(BUF_SIZE).expect("Failed to allocate new buffer");
        let mut data = vec![0; BUF_SIZE];
        let data_slice = &mut data[..];
        for y in 0..TX_HEIGHT / 2 {
            for x in 0..TX_WIDTH / 2 {
                let idx = (y * TX_WIDTH * 4 + x * 4) as usize;
                data_slice[idx] = 200;
                data_slice[idx + 1] = 200;
                data_slice[idx + 2] = 200;
                data_slice[idx + 3] = 200;
            }
        }

        let mut buffer = gst::Buffer::from_slice(data);
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
