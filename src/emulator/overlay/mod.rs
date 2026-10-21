//! Immediate-mode GL UI: solid rects + 8×8 bitmap text (screen-pixel coords).

mod font8x8;

use font8x8::FONT8X8;
use std::ffi::{CStr, CString};
use std::ptr;

/// Floats per vertex: x y u v r g b a
const FLOATS_PER_VERT: usize = 8;
/// Vertices per quad (two triangles).
const VERTS_PER_QUAD: usize = 6;
const FLOATS_PER_QUAD: usize = FLOATS_PER_VERT * VERTS_PER_QUAD;
/// Enough for debug HUD (shadowed text) + pause/settings menus.
const MAX_QUADS: usize = 4096;
const MAX_FLOATS: usize = MAX_QUADS * FLOATS_PER_QUAD;

/// Overlay drawer in top-left origin pixel space.
pub struct Overlay {
    program: u32,
    vao: u32,
    vbo: u32,
    font_tex: u32,
    /// Interleaved: x y u v r g b a
    verts: Vec<f32>,
    fb_w: f32,
    fb_h: f32,
}

impl Overlay {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let program = compile_program(VERT_SRC, FRAG_SRC)?;
            let mut vao = 0;
            let mut vbo = 0;
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (MAX_FLOATS * std::mem::size_of::<f32>()) as isize,
                ptr::null(),
                gl::DYNAMIC_DRAW,
            );
            let stride = (FLOATS_PER_VERT * std::mem::size_of::<f32>()) as i32;
            for (i, (size, offset)) in [(2, 0), (2, 2), (4, 4)].into_iter().enumerate() {
                gl::EnableVertexAttribArray(i as u32);
                gl::VertexAttribPointer(
                    i as u32,
                    size,
                    gl::FLOAT,
                    gl::FALSE,
                    stride,
                    (offset * std::mem::size_of::<f32>()) as *const _,
                );
            }
            gl::BindVertexArray(0);

            let font_tex = build_font_texture()?;

            Ok(Self {
                program,
                vao,
                vbo,
                font_tex,
                verts: Vec::with_capacity(1024),
                fb_w: 1.0,
                fb_h: 1.0,
            })
        }
    }

    pub fn begin(&mut self, fb_w: i32, fb_h: i32) {
        self.fb_w = fb_w.max(1) as f32;
        self.fb_h = fb_h.max(1) as f32;
        self.verts.clear();
    }

    /// Filled axis-aligned rect in pixel coords (`rgba` 0..=1). `u < 0` → solid.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, rgba: [f32; 4]) {
        self.quad(x, y, w, h, -1.0, -1.0, -1.0, -1.0, rgba);
    }

    pub fn text(&mut self, x: f32, y: f32, scale: f32, rgba: [f32; 4], s: &str) {
        let cw = 8.0 * scale;
        let ch = 8.0 * scale;
        let mut cx = x;
        for c in s.chars() {
            if c == '\n' {
                continue;
            }
            let code = c as u32;
            if !(0x20..0x7F).contains(&code) {
                cx += cw;
                continue;
            }
            let idx = (code - 0x20) as usize;
            // Atlas: 16 glyphs per row, each 8×8 in a 128×48 texture.
            let col = (idx % 16) as f32;
            let row = (idx / 16) as f32;
            let u0 = col * 8.0 / 128.0;
            let v0 = row * 8.0 / 48.0;
            let u1 = (col + 1.0) * 8.0 / 128.0;
            let v1 = (row + 1.0) * 8.0 / 48.0;
            self.quad(cx, y, cw, ch, u0, v0, u1, v1, rgba);
            cx += cw;
        }
    }

    pub fn text_width(s: &str, scale: f32) -> f32 {
        s.chars().count() as f32 * 8.0 * scale
    }

    pub fn flush(&mut self) {
        if self.verts.is_empty() {
            return;
        }
        let count = (self.verts.len() / 8) as i32;
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::Viewport(0, 0, self.fb_w as i32, self.fb_h as i32);

            gl::UseProgram(self.program);
            let loc = gl::GetUniformLocation(self.program, c"uScreen".as_ptr());
            gl::Uniform2f(loc, self.fb_w, self.fb_h);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.font_tex);
            let tloc = gl::GetUniformLocation(self.program, c"uFont".as_ptr());
            gl::Uniform1i(tloc, 0);

            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                (self.verts.len() * std::mem::size_of::<f32>()) as isize,
                self.verts.as_ptr() as *const _,
            );
            gl::DrawArrays(gl::TRIANGLES, 0, count);
            gl::BindVertexArray(0);
            gl::Disable(gl::BLEND);
        }
        self.verts.clear();
    }

    fn quad(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
        rgba: [f32; 4],
    ) {
        // Debug HUD can fill the batch; flush mid-frame so the pause menu is not dropped.
        if self.verts.len() + FLOATS_PER_QUAD > MAX_FLOATS {
            self.flush();
        }
        let [r, g, b, a] = rgba;
        let x0 = x;
        let y0 = y;
        let x1 = x + w;
        let y1 = y + h;
        for (px, py, u, v) in [
            (x0, y0, u0, v0),
            (x0, y1, u0, v1),
            (x1, y1, u1, v1),
            (x0, y0, u0, v0),
            (x1, y1, u1, v1),
            (x1, y0, u1, v0),
        ] {
            self.verts
                .extend_from_slice(&[px, py, u, v, r, g, b, a]);
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.program);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteTextures(1, &self.font_tex);
        }
    }
}

unsafe fn build_font_texture() -> Result<u32, String> {
    // 16×6 glyphs × 8px = 128×48, R8 coverage.
    let mut pixels = vec![0u8; 128 * 48];
    for (i, glyph) in FONT8X8.iter().enumerate() {
        let col = i % 16;
        let row = i / 16;
        for (gy, byte) in glyph.iter().enumerate() {
            for gx in 0..8 {
                if byte & (1 << gx) != 0 {
                    let x = col * 8 + gx;
                    let y = row * 8 + gy;
                    pixels[y * 128 + x] = 255;
                }
            }
        }
    }

    unsafe {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::R8 as i32,
            128,
            48,
            0,
            gl::RED,
            gl::UNSIGNED_BYTE,
            pixels.as_ptr() as *const _,
        );
        Ok(tex)
    }
}

const VERT_SRC: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aUv;
layout (location = 2) in vec4 aColor;
uniform vec2 uScreen;
out vec2 vUv;
out vec4 vColor;
void main() {
    vec2 ndc = vec2(aPos.x / uScreen.x * 2.0 - 1.0, 1.0 - aPos.y / uScreen.y * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
    vUv = aUv;
    vColor = aColor;
}
"#;

const FRAG_SRC: &str = r#"#version 330 core
in vec2 vUv;
in vec4 vColor;
out vec4 FragColor;
uniform sampler2D uFont;
void main() {
    if (vUv.x < 0.0) {
        FragColor = vColor;
    } else {
        float a = texture(uFont, vUv).r;
        FragColor = vec4(vColor.rgb, vColor.a * a);
    }
}
"#;

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
            return Err(format!("overlay link: {msg}"));
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
            return Err(format!("overlay shader: {msg}"));
        }
        Ok(shader)
    }
}
