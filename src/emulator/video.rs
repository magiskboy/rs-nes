//! NES framebuffer → OpenGL present (with optional display filters).

use crate::ppu::{FRAMEBUFFER_SIZE, SCREEN_HEIGHT, SCREEN_WIDTH};
use std::ffi::{CStr, CString};
use std::ptr;

/// How the NES picture is scaled / shaded on the host display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoFilter {
    /// Integer scale, nearest-neighbor (crisp pixels).
    #[default]
    Sharp,
    /// Bilinear upscale (softer).
    Smooth,
    /// Horizontal scanlines only.
    Scanlines,
    /// CRT-ish: curvature, scanlines, vignette, RGB mask.
    Crt,
}

impl VideoFilter {
    pub const ALL: [Self; 4] = [Self::Sharp, Self::Smooth, Self::Scanlines, Self::Crt];

    pub fn label(self) -> &'static str {
        match self {
            Self::Sharp => "Sharp",
            Self::Smooth => "Smooth",
            Self::Scanlines => "Scanlines",
            Self::Crt => "CRT",
        }
    }

    pub fn cycle(self, dir: i32) -> Self {
        let i = Self::ALL.iter().position(|&f| f == self).unwrap_or(0) as i32;
        let n = Self::ALL.len() as i32;
        let next = (i + dir).rem_euclid(n) as usize;
        Self::ALL[next]
    }
}

const VERT_SRC: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aUv;
out vec2 vUv;
void main() {
    vUv = aUv;
    gl_Position = vec4(aPos, 0.0, 1.0);
}
"#;

// uFlipY: NES PPU upload needs V flip; UI FBO is already top-correct.
// uFilter: 0 Sharp, 1 Smooth, 2 Scanlines, 3 CRT
const FRAG_SRC: &str = r#"#version 330 core
in vec2 vUv;
out vec4 FragColor;
uniform sampler2D uTex;
uniform int uFilter;
uniform int uFlipY;

void main() {
    vec2 uv = uFlipY != 0 ? vec2(vUv.x, 1.0 - vUv.y) : vUv;

    if (uFilter == 3) {
        // Mild barrel curvature toward screen edges.
        vec2 cc = uv * 2.0 - 1.0;
        float r2 = dot(cc, cc);
        cc *= 1.0 + r2 * 0.06;
        uv = cc * 0.5 + 0.5;
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            FragColor = vec4(0.0, 0.0, 0.0, 1.0);
            return;
        }
    }

    vec3 col = texture(uTex, uv).rgb;

    if (uFilter == 2 || uFilter == 3) {
        // Soft scanlines tied to NES vertical resolution.
        float line = sin(uv.y * 240.0 * 3.14159265);
        float scan = 0.78 + 0.22 * line * line;
        col *= scan;
    }

    if (uFilter == 3) {
        // Edge vignette
        vec2 vc = uv * 2.0 - 1.0;
        float vig = clamp(1.0 - dot(vc, vc) * 0.18, 0.55, 1.0);
        col *= vig;

        // Coarse RGB aperture (uses output pixels).
        float triad = mod(gl_FragCoord.x, 3.0);
        vec3 mask = triad < 1.0
            ? vec3(1.15, 0.85, 0.85)
            : (triad < 2.0 ? vec3(0.85, 1.15, 0.85) : vec3(0.85, 0.85, 1.15));
        col *= mix(vec3(1.0), mask, 0.4);

        // Compensate darkening from scanlines / mask.
        col *= 1.2;
    }

    FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
"#;

/// Uploads the NES RGB framebuffer to a GL texture and draws a fullscreen quad.
///
/// Also owns an offscreen FBO so the NES OS shell can be drawn and then presented
/// through the same video filter as games.
pub struct GlPresenter {
    program: u32,
    vao: u32,
    vbo: u32,
    texture: u32,
    u_filter: i32,
    u_flip_y: i32,
    /// Offscreen target for shell UI (window-sized).
    ui_fbo: u32,
    ui_tex: u32,
    ui_w: i32,
    ui_h: i32,
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

            gl::UseProgram(program);
            let loc = gl::GetUniformLocation(program, c"uTex".as_ptr());
            gl::Uniform1i(loc, 0);
            let u_filter = gl::GetUniformLocation(program, c"uFilter".as_ptr());
            let u_flip_y = gl::GetUniformLocation(program, c"uFlipY".as_ptr());

            let mut ui_fbo = 0;
            let mut ui_tex = 0;
            gl::GenFramebuffers(1, &mut ui_fbo);
            gl::GenTextures(1, &mut ui_tex);

            Ok(Self {
                program,
                vao,
                vbo,
                texture,
                u_filter,
                u_flip_y,
                ui_fbo,
                ui_tex,
                ui_w: 0,
                ui_h: 0,
            })
        }
    }

    /// Resize the shell UI offscreen buffer if the window framebuffer changed.
    pub fn ensure_ui_target(&mut self, width: i32, height: i32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.ui_w == width && self.ui_h == height {
            return;
        }
        self.ui_w = width;
        self.ui_h = height;
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.ui_tex);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB as i32,
                width,
                height,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                ptr::null(),
            );
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.ui_fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                self.ui_tex,
                0,
            );
            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            debug_assert_eq!(status, gl::FRAMEBUFFER_COMPLETE);
        }
    }

    /// Bind the shell UI FBO and clear it (call before drawing the OS overlay).
    pub fn begin_ui(&self, clear: [f32; 4]) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.ui_fbo);
            gl::Viewport(0, 0, self.ui_w, self.ui_h);
            gl::ClearColor(clear[0], clear[1], clear[2], clear[3]);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
    }

    /// Present the shell UI texture to the default framebuffer with the active filter.
    pub fn present_ui(&self, fb_width: i32, fb_height: i32, filter: VideoFilter) {
        let filter_id = match filter {
            VideoFilter::Sharp => 0,
            VideoFilter::Smooth => 1,
            VideoFilter::Scanlines => 2,
            VideoFilter::Crt => 3,
        };
        let mag = match filter {
            VideoFilter::Smooth => gl::LINEAR,
            _ => gl::NEAREST,
        };
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, fb_width, fb_height);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(self.program);
            if self.u_filter >= 0 {
                gl::Uniform1i(self.u_filter, filter_id);
            }
            if self.u_flip_y >= 0 {
                gl::Uniform1i(self.u_flip_y, 0);
            }
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.ui_tex);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, mag as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, mag as i32);
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
            gl::BindVertexArray(0);
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

    /// Draw letterboxed NES picture into the framebuffer with the chosen filter.
    pub fn draw(&self, fb_width: i32, fb_height: i32, filter: VideoFilter) {
        let (x, y, w, h) = present_rect(fb_width, fb_height, filter);
        let filter_id = match filter {
            VideoFilter::Sharp => 0,
            VideoFilter::Smooth => 1,
            VideoFilter::Scanlines => 2,
            VideoFilter::Crt => 3,
        };
        let mag = match filter {
            VideoFilter::Smooth => gl::LINEAR,
            _ => gl::NEAREST,
        };

        unsafe {
            gl::Viewport(x, y, w, h);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(self.program);
            if self.u_filter >= 0 {
                gl::Uniform1i(self.u_filter, filter_id);
            }
            if self.u_flip_y >= 0 {
                gl::Uniform1i(self.u_flip_y, 1);
            }
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, mag as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, mag as i32);
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
            gl::BindVertexArray(0);
        }
    }
}

fn present_rect(fb_width: i32, fb_height: i32, filter: VideoFilter) -> (i32, i32, i32, i32) {
    let (w, h) = match filter {
        // CRT looks better when the picture fills the window (still aspect-correct).
        VideoFilter::Crt => {
            let sx = fb_width as f32 / SCREEN_WIDTH as f32;
            let sy = fb_height as f32 / SCREEN_HEIGHT as f32;
            let s = sx.min(sy).max(1.0);
            (
                (SCREEN_WIDTH as f32 * s).round() as i32,
                (SCREEN_HEIGHT as f32 * s).round() as i32,
            )
        }
        _ => {
            let scale = ((fb_width / SCREEN_WIDTH as i32).max(1))
                .min((fb_height / SCREEN_HEIGHT as i32).max(1));
            (SCREEN_WIDTH as i32 * scale, SCREEN_HEIGHT as i32 * scale)
        }
    };
    let x = (fb_width - w) / 2;
    let y = (fb_height - h) / 2;
    (x, y, w, h)
}

impl Drop for GlPresenter {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.program);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteTextures(1, &self.texture);
            gl::DeleteTextures(1, &self.ui_tex);
            gl::DeleteFramebuffers(1, &self.ui_fbo);
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
