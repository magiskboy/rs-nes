use crate::ppu::{FRAMEBUFFER_SIZE, SCREEN_HEIGHT, SCREEN_WIDTH};
use std::ffi::{CStr, CString};
use std::ptr;

const VERT_SRC: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aUv;
out vec2 vUv;
void main() {
    vUv = aUv;
    gl_Position = vec4(aPos, 0.0, 1.0);
}
"#;

// Flip V so PPU top row appears at top of the window.
const FRAG_SRC: &str = r#"#version 330 core
in vec2 vUv;
out vec4 FragColor;
uniform sampler2D uTex;
void main() {
    FragColor = texture(uTex, vec2(vUv.x, 1.0 - vUv.y));
}
"#;

/// Uploads the NES RGB framebuffer to a GL texture and draws a fullscreen quad.
pub struct GlPresenter {
    program: u32,
    vao: u32,
    vbo: u32,
    texture: u32,
}

impl GlPresenter {
    /// Build GL objects. Call after `gl::load_with` on a current context.
    pub fn new() -> Result<Self, String> {
        unsafe {
            let program = compile_program(VERT_SRC, FRAG_SRC)?;

            // pos.xy, uv.xy — two triangles covering NDC
            #[rustfmt::skip]
            let verts: [f32; 24] = [
                -1.0,  1.0,  0.0, 1.0,
                -1.0, -1.0,  0.0, 0.0,
                 1.0, -1.0,  1.0, 0.0,
                -1.0,  1.0,  0.0, 1.0,
                 1.0, -1.0,  1.0, 0.0,
                 1.0,  1.0,  1.0, 1.0,
            ];

            let mut vao = 0;
            let mut vbo = 0;
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (verts.len() * std::mem::size_of::<f32>()) as isize,
                verts.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );
            let stride = (4 * std::mem::size_of::<f32>()) as i32;
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, stride, ptr::null());
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(
                1,
                2,
                gl::FLOAT,
                gl::FALSE,
                stride,
                (2 * std::mem::size_of::<f32>()) as *const _,
            );
            gl::BindVertexArray(0);

            let mut texture = 0;
            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB as i32,
                SCREEN_WIDTH as i32,
                SCREEN_HEIGHT as i32,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                ptr::null(),
            );

            let loc = gl::GetUniformLocation(program, c"uTex".as_ptr());
            gl::UseProgram(program);
            gl::Uniform1i(loc, 0);

            Ok(Self {
                program,
                vao,
                vbo,
                texture,
            })
        }
    }

    /// Upload packed RGB (`256×240×3`) from the PPU.
    pub fn upload(&self, framebuffer: &[u8]) {
        assert!(framebuffer.len() >= FRAMEBUFFER_SIZE);
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                SCREEN_WIDTH as i32,
                SCREEN_HEIGHT as i32,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                framebuffer.as_ptr() as *const _,
            );
        }
    }

    /// Draw letterboxed integer-scaled NES picture into the framebuffer.
    pub fn draw(&self, fb_width: i32, fb_height: i32) {
        let scale = ((fb_width / SCREEN_WIDTH as i32).max(1))
            .min((fb_height / SCREEN_HEIGHT as i32).max(1));
        let w = SCREEN_WIDTH as i32 * scale;
        let h = SCREEN_HEIGHT as i32 * scale;
        let x = (fb_width - w) / 2;
        let y = (fb_height - h) / 2;

        unsafe {
            gl::Viewport(x, y, w, h);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(self.program);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
            gl::BindVertexArray(0);
        }
    }
}

impl Drop for GlPresenter {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.program);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteTextures(1, &self.texture);
        }
    }
}

unsafe fn compile_program(vert: &str, frag: &str) -> Result<u32, String> {
    unsafe {
        let vs = compile_shader(gl::VERTEX_SHADER, vert)?;
        let fs = compile_shader(gl::FRAGMENT_SHADER, frag)?;
        let program = gl::CreateProgram();
        gl::AttachShader(program, vs);
        gl::AttachShader(program, fs);
        gl::LinkProgram(program);
        gl::DeleteShader(vs);
        gl::DeleteShader(fs);

        let mut ok = 0;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0;
            gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf = vec![0u8; len as usize];
            gl::GetProgramInfoLog(program, len, ptr::null_mut(), buf.as_mut_ptr() as *mut _);
            let msg = CStr::from_ptr(buf.as_ptr() as *const _).to_string_lossy();
            return Err(format!("program link: {msg}"));
        }
        Ok(program)
    }
}

unsafe fn compile_shader(kind: u32, src: &str) -> Result<u32, String> {
    unsafe {
        let shader = gl::CreateShader(kind);
        let c = CString::new(src).map_err(|e| e.to_string())?;
        gl::ShaderSource(shader, 1, &c.as_ptr(), ptr::null());
        gl::CompileShader(shader);
        let mut ok = 0;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0;
            gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf = vec![0u8; len as usize];
            gl::GetShaderInfoLog(shader, len, ptr::null_mut(), buf.as_mut_ptr() as *mut _);
            let msg = CStr::from_ptr(buf.as_ptr() as *const _).to_string_lossy();
            return Err(format!("shader compile: {msg}"));
        }
        Ok(shader)
    }
}

