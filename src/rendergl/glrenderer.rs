use super::{bindings::gl, vertex};
use std::{
    ffi::{c_void, CString},
    mem, ptr,
};
use vertex::Quad;

pub struct GlRenderer {
    bindings: gl::Gl,
    vao: u32,
    quad_vertex_buffer: u32,
    quad_index_buffer: u32,
    program_mono: u32,
    program_argb: u32,
}

impl GlRenderer {
    pub fn new<F>(func: F) -> Self
    where
        F: FnMut(&'static str) -> *const c_void,
    {
        let bindings = gl::Gl::load_with(func);
        unsafe { Self::create(bindings) }
    }

    unsafe fn create(bindings: gl::Gl) -> Self {
        let program_mono = Self::compile_program(
            &bindings,
            include_str!("shaders/glvert.glsl"),
            include_str!("shaders/glfrag.glsl"),
        );
        let program_argb = 0_u32; // TODO: Create this program.
        let (vao, quad_vertex_buffer, quad_index_buffer) = Self::create_vao(&bindings);
        Self {
            bindings,
            vao,
            quad_vertex_buffer,
            quad_index_buffer,
            program_mono,
            program_argb,
        }
    }

    unsafe fn compile_program(bindings: &gl::Gl, vs_src: &str, fs_src: &str) -> u32 {
        let vs = Self::compile_shader(bindings, vs_src, gl::VERTEX_SHADER);
        let fs = Self::compile_shader(bindings, fs_src, gl::FRAGMENT_SHADER);

        let program = bindings.CreateProgram();
        bindings.AttachShader(program, vs);
        bindings.AttachShader(program, fs);
        bindings.LinkProgram(program);

        {
            let mut success: gl::types::GLint = 1;
            bindings.GetProgramiv(program, gl::LINK_STATUS, &mut success);
            assert!(success != 0);
        }
        bindings.DetachShader(program, vs);
        bindings.DeleteShader(vs);
        bindings.DetachShader(program, fs);
        bindings.DeleteShader(fs);
        program
    }

    unsafe fn compile_shader(
        bindings: &gl::Gl,
        src: &str,
        shader_type: gl::types::GLenum,
    ) -> u32 {
        let shader = bindings.CreateShader(shader_type);
        let shader_src = CString::new(src).expect("Failed to include vertex shader source");
        // bindings.ShaderSource(vs, 1, [VS_SRC.as_ptr() as *const _].as_ptr(), ptr::null());
        bindings.ShaderSource(shader, 1, [shader_src.as_ptr() as _].as_ptr(), ptr::null());
        bindings.CompileShader(shader);
        {
            let mut success: gl::types::GLint = 1;
            bindings.GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
            assert!(success != 0);
        }
        shader
    }
    unsafe fn create_vao(bindings: &gl::Gl) -> (u32, u32, u32) {
        // Generate Vertex Array Object, this stores buffers/pointers/indexes
        let mut vao = mem::MaybeUninit::uninit();
        bindings.GenVertexArrays(1, vao.as_mut_ptr());
        let vao = vao.assume_init();
        // Bind the VAO (it "records" which buffers to use to draw)
        bindings.BindVertexArray(vao);

        // Create Vertex Buffer
        let mut quad_vertex_buffer = mem::MaybeUninit::uninit();
        bindings.GenBuffers(1, quad_vertex_buffer.as_mut_ptr());
        let quad_vertex_buffer = quad_vertex_buffer.assume_init();
        bindings.BindBuffer(gl::ARRAY_BUFFER, quad_vertex_buffer);
        bindings.BufferData(
            gl::ARRAY_BUFFER,
            (Quad::VERTICES.len() * mem::size_of::<vertex::Vertex>()) as _,
            // vertex::VERTICES.as_ptr() as _,
            ptr::null() as _,
            gl::STREAM_DRAW,
        );

        // Create Index Buffer
        let mut quad_index_buffer = mem::MaybeUninit::uninit();
        bindings.GenBuffers(1, quad_index_buffer.as_mut_ptr());
        let quad_index_buffer = quad_index_buffer.assume_init();
        bindings.BindBuffer(gl::ELEMENT_ARRAY_BUFFER, quad_index_buffer);
        bindings.BufferData(
            gl::ELEMENT_ARRAY_BUFFER,
            (Quad::INDICES.len() * mem::size_of::<u16>()) as _,
            Quad::INDICES.as_ptr() as _, // Set the index buffer statically
            gl::STATIC_DRAW,
        );
        // Setup attribute pointers while the VAO is bound to record this.

        // The position is in layout=0 in the shader
        bindings.VertexAttribPointer(
            0,
            vertex::NUM_VERTEX_COORDS as _,
            gl::FLOAT,
            gl::FALSE,
            mem::size_of::<vertex::Vertex>() as _,
            ptr::null(),
        );
        // Texture coords in layout=1
        bindings.VertexAttribPointer(
            1,
            vertex::NUM_TEX_COORDS as _,
            gl::FLOAT,
            gl::FALSE,
            mem::size_of::<vertex::Vertex>() as _,
            (vertex::NUM_VERTEX_COORDS * mem::size_of::<f32>()) as _,
        );
        // Enable attribute 0
        bindings.EnableVertexAttribArray(0);
        bindings.EnableVertexAttribArray(1);

        // Unbind the VAO BEFORE! unbinding the vertex- and index-buffers
        bindings.BindVertexArray(0);
        bindings.BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
        bindings.BindBuffer(gl::ARRAY_BUFFER, 0);
        bindings.DisableVertexAttribArray(0);
        bindings.DisableVertexAttribArray(1);
        (vao, quad_vertex_buffer, quad_index_buffer)
    }

    unsafe fn update_vertex_buffer(&self, vertices: &[vertex::Vertex]) {
        assert!(vertices.len() == Quad::VERTICES.len()); // Make sure the vertices match
        self.bindings
            .BindBuffer(gl::ARRAY_BUFFER, self.quad_vertex_buffer);
        self.bindings.BufferSubData(
            gl::ARRAY_BUFFER,
            0,
            (vertices.len() * mem::size_of::<vertex::Vertex>()) as _,
            vertices.as_ptr() as _,
        );

        self.bindings.BindBuffer(gl::ARRAY_BUFFER, 0);
    }
    unsafe fn draw_image(&self, vertices: &[vertex::Vertex], image_texture: u32, lut_texture: u32) {
        // Update the vertex buffer
        self.update_vertex_buffer(vertices);

        self.bindings.UseProgram(self.program_mono);
        self.bindings.BindVertexArray(self.vao);

        // Activate and bind the textures
        self.bindings.ActiveTexture(gl::TEXTURE0); // Activate texture unit 0
        self.bindings.BindTexture(gl::TEXTURE_2D, image_texture);
        // self.bindings.BindTexture(gl::TEXTURE_2D, self.texture_id);
        self.bindings.ActiveTexture(gl::TEXTURE0 + 1);
        self.bindings.BindTexture(gl::TEXTURE_2D, lut_texture);

        self.bindings
            .DrawElements(gl::TRIANGLES, 6, gl::UNSIGNED_SHORT, ptr::null());

        // Unbind resources
        self.bindings.BindVertexArray(0);
        self.bindings.ActiveTexture(gl::TEXTURE0); // Activate texture unit 0
        self.bindings.BindTexture(gl::TEXTURE_2D, 0);
        self.bindings.ActiveTexture(gl::TEXTURE0 + 1); // Activate texture unit 0
        self.bindings.BindTexture(gl::TEXTURE_2D, 0);
        self.bindings.UseProgram(0);
    }

    pub fn draw(&self, vertices: &[vertex::Vertex], image_texture: u32, lut_texture: u32) {
        unsafe {
            self.bindings.ClearColor(1.0, 0.0, 0.0, 1.0);
            self.bindings.Clear(gl::COLOR_BUFFER_BIT);
            // Draw the image
            self.draw_image(vertices, image_texture, lut_texture);
            // Place to draw the cursor (remember alpha blend)?
        }
    }
}
