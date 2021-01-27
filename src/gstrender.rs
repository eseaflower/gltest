use std::sync::mpsc::Receiver;
use gst_gl::GLContextExtManual;
use gstreamer_gl as gst_gl;
use crate::rendergl;


#[derive(Debug)]
pub struct RenderMessage {
    pub image_texture_id: u32,
    pub lut_texture_id: u32,
    pub vertex_data: Vec<rendergl::vertex::Vertex>,
}

pub struct GstRenderStruct {
    renderer: rendergl::glrenderer::GlRenderer,
    recv: Receiver<RenderMessage>,
    _ctx: gst_gl::GLContext,
}

impl GstRenderStruct {
    pub fn new(context: gst_gl::GLContext, recv: Receiver<RenderMessage>) -> Self {
        let renderer = rendergl::glrenderer::GlRenderer::new(|name| {
            context.get_proc_address(name) as *const _
        });
        Self {
            renderer,
            recv,
            _ctx: context,
        }
    }

    pub unsafe fn draw(&self) {
        let message = self.recv.recv().expect("Failed to receive RenderMessage");
        println!("Received message {:?}", message);

        self.renderer.draw(
            &message.vertex_data,
            message.image_texture_id,
            message.lut_texture_id,
        );
    }
}