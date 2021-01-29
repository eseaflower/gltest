use crate::{bidir::BidirChannel, rendergl, texture::TextureDescription};
use gst_gl::GLContextExtManual;
use gstreamer_gl as gst_gl;
use std::sync::mpsc::Receiver;

#[derive(Debug)]
pub struct RenderMessage {
    pub image_texture: TextureDescription,
    pub lut_texture: TextureDescription,
    pub vertex_data: Vec<rendergl::vertex::Vertex>,
}

pub struct GstRenderStruct {
    renderer: rendergl::glrenderer::GlRenderer,
    channel: BidirChannel<RenderMessage>,
    _ctx: gst_gl::GLContext,
}

impl GstRenderStruct {
    pub fn new(context: gst_gl::GLContext, channel: BidirChannel<RenderMessage>) -> Self {
        let renderer = rendergl::glrenderer::GlRenderer::new(|name| {
            context.get_proc_address(name) as *const _
        });
        Self {
            renderer,
            channel,
            _ctx: context,
        }
    }

    pub unsafe fn draw(&self) {
        let message = self
            .channel
            .recv()
            .expect("Failed to receive RenderMessage");
        println!("Received message {:?}", message);

        self.renderer.draw(
            &message.vertex_data,
            message.image_texture.id,
            message.lut_texture.id,
        );
        // Send the message back signalling that we are done
        self.channel
            .send(message)
            .expect("Failed to send RenderMessage back to main");
    }
}
