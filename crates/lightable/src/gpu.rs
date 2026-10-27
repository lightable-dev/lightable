//! Drawing [`lightable_render::tess::Geometry`] with OpenGL / WebGL2 (via glow).
//!
//! Vertex data is uploaded once per drawing. A frame only sets two
//! uniforms per visible chunk and draws, so panning and zooming cost almost
//! nothing on the CPU. Chunk origins are subtracted from the view centre in
//! `f64` before going to the GPU, so precision is relative to the view.

use eframe::egui_glow::ShaderVersion;
use eframe::glow::{self, HasContext as _};

use lightable_render::View;
use lightable_render::tess::{Chunk, Geometry};

/// One vertex: position (2 × f32, relative to the chunk origin) and colour.
const STRIDE: i32 = 12;

struct Batch {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    count: i32,
}

struct GpuChunk {
    layer: u32,
    origin: [f64; 2],
    bounds: Option<lightable_render::Bounds>,
    lines: Option<Batch>,
    coarse_lines: Option<Batch>,
    max_radius: f64,
    triangles: Option<Batch>,
    points: Option<Batch>,
}

/// GPU copies of a drawing's geometry. Holds GL handles only (not the
/// context), so it can live behind the thread-safe paint callback; call
/// [`GpuScene::destroy`] with the context to free it.
pub struct GpuScene {
    program: glow::Program,
    u_offset: Option<glow::UniformLocation>,
    u_scale: Option<glow::UniformLocation>,
    u_point_size: Option<glow::UniformLocation>,
    chunks: Vec<GpuChunk>,
}

/// What the last frame drew, for the status bar.
#[derive(Default, Clone, Copy)]
pub struct DrawStats {
    pub chunks: usize,
    pub vertices: usize,
}

fn shader_source(version: ShaderVersion, vertex: bool) -> String {
    let header = version.version_declaration();
    let precision = if version.is_embedded() {
        "precision highp float;\n"
    } else {
        ""
    };
    let new = version.is_new_shader_interface();
    let (attr, vary_out, vary_in, frag_out, frag_color) = if new {
        ("in", "out", "in", "out vec4 frag_color;\n", "frag_color")
    } else {
        ("attribute", "varying", "varying", "", "gl_FragColor")
    };
    if vertex {
        format!(
            "{header}{precision}\
             {attr} vec2 a_pos;\n{attr} vec4 a_color;\n{vary_out} vec4 v_color;\n\
             uniform vec2 u_offset;\nuniform vec2 u_scale;\nuniform float u_point_size;\n\
             void main() {{\n  v_color = a_color;\n  gl_PointSize = u_point_size;\n\
               gl_Position = vec4((a_pos + u_offset) * u_scale, 0.0, 1.0);\n}}\n"
        )
    } else {
        format!(
            "{header}{precision}{vary_in} vec4 v_color;\n{frag_out}\
             void main() {{\n  {frag_color} = v_color;\n}}\n"
        )
    }
}

unsafe fn compile(gl: &glow::Context, version: ShaderVersion) -> Result<glow::Program, String> {
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, vertex) in [(glow::VERTEX_SHADER, true), (glow::FRAGMENT_SHADER, false)] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &shader_source(version, vertex));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(format!("shader: {}", gl.get_shader_info_log(shader)));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.bind_attrib_location(program, 0, "a_pos");
        gl.bind_attrib_location(program, 1, "a_color");
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(format!("program: {}", gl.get_program_info_log(program)));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        Ok(program)
    }
}

/// Interleaves positions and colours (12 bytes per vertex).
fn interleave(positions: &[[f32; 2]], colors: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(positions.len() * STRIDE as usize);
    for (p, c) in positions.iter().zip(colors) {
        bytes.extend_from_slice(&p[0].to_le_bytes());
        bytes.extend_from_slice(&p[1].to_le_bytes());
        bytes.extend_from_slice(c);
    }
    bytes
}

unsafe fn upload(
    gl: &glow::Context,
    positions: &[[f32; 2]],
    colors: &[[u8; 4]],
) -> Result<Option<Batch>, String> {
    if positions.is_empty() {
        return Ok(None);
    }
    unsafe {
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            &interleave(positions, colors),
            glow::STATIC_DRAW,
        );
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, STRIDE, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 4, glow::UNSIGNED_BYTE, true, STRIDE, 8);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        Ok(Some(Batch {
            vao,
            vbo,
            count: positions.len() as i32,
        }))
    }
}

impl GpuScene {
    pub fn new(gl: &glow::Context, geometry: &Geometry) -> Result<Self, String> {
        unsafe {
            let program = compile(gl, ShaderVersion::get(gl))?;
            let u_offset = gl.get_uniform_location(program, "u_offset");
            let u_scale = gl.get_uniform_location(program, "u_scale");
            let u_point_size = gl.get_uniform_location(program, "u_point_size");
            let mut chunks = Vec::with_capacity(geometry.chunks.len());
            for c in &geometry.chunks {
                chunks.push(Self::chunk(gl, c)?);
            }
            Ok(Self {
                program,
                u_offset,
                u_scale,
                u_point_size,
                chunks,
            })
        }
    }

    unsafe fn chunk(gl: &glow::Context, c: &Chunk) -> Result<GpuChunk, String> {
        unsafe {
            Ok(GpuChunk {
                layer: c.layer,
                origin: c.origin,
                bounds: c.bounds,
                lines: upload(gl, &c.lines, &c.line_colors)?,
                coarse_lines: upload(gl, &c.coarse_lines, &c.coarse_line_colors)?,
                max_radius: c.max_radius,
                triangles: upload(gl, &c.triangles, &c.triangle_colors)?,
                points: upload(gl, &c.points, &c.point_colors)?,
            })
        }
    }

    /// Draws the visible layers' chunks that intersect the view. Called
    /// with the GL viewport already set to the canvas.
    pub fn draw(
        &self,
        gl: &glow::Context,
        view: &View,
        visible: &[bool],
        pixels_per_point: f32,
    ) -> DrawStats {
        let shown = view.world_bounds();
        let mut stats = DrawStats::default();
        unsafe {
            gl.use_program(Some(self.program));
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            #[cfg(not(target_arch = "wasm32"))]
            gl.enable(glow::PROGRAM_POINT_SIZE);
            let sx = (2.0 * view.scale / view.size[0]) as f32;
            let sy = (2.0 * view.scale / view.size[1]) as f32;
            gl.uniform_2_f32(self.u_scale.as_ref(), sx, sy);
            gl.uniform_1_f32(self.u_point_size.as_ref(), 3.0 * pixels_per_point);
            // Two passes: every fill first (hatches, solids), then lines
            // and points, so no fill hides a line of another chunk.
            for fills in [true, false] {
                for c in &self.chunks {
                    if !visible.get(c.layer as usize).copied().unwrap_or(true) {
                        continue;
                    }
                    if let Some(b) = &c.bounds
                        && !b.intersects(&shown)
                    {
                        continue;
                    }
                    if fills && c.triangles.is_none() {
                        continue;
                    }
                    // f64 here, so the f32 offset is small near the view.
                    let ox = (c.origin[0] - view.center[0]) as f32;
                    let oy = (c.origin[1] - view.center[1]) as f32;
                    gl.uniform_2_f32(self.u_offset.as_ref(), ox, oy);
                    if !fills {
                        stats.chunks += 1;
                    }
                    // Arcs small on screen: the coarse version looks the same.
                    let coarse = c.coarse_lines.is_some()
                        && lightable_render::tess::use_coarse(
                            c.max_radius,
                            view.scale,
                            f64::from(pixels_per_point),
                        );
                    let lines = if coarse { &c.coarse_lines } else { &c.lines };
                    let batches = if fills {
                        vec![(&c.triangles, glow::TRIANGLES)]
                    } else {
                        vec![(lines, glow::LINES), (&c.points, glow::POINTS)]
                    };
                    for (batch, mode) in batches {
                        if let Some(b) = batch {
                            gl.bind_vertex_array(Some(b.vao));
                            gl.draw_arrays(mode, 0, b.count);
                            stats.vertices += b.count as usize;
                        }
                    }
                }
            }
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
        stats
    }
}

impl GpuScene {
    /// Frees the GPU objects.
    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            for c in &self.chunks {
                for b in [&c.lines, &c.coarse_lines, &c.triangles, &c.points]
                    .into_iter()
                    .flatten()
                {
                    gl.delete_vertex_array(b.vao);
                    gl.delete_buffer(b.vbo);
                }
            }
            gl.delete_program(self.program);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertices_are_interleaved_little_endian() {
        let bytes = interleave(&[[1.0, -2.0]], &[[1, 2, 3, 255]]);
        assert_eq!(bytes.len(), 12);
        assert_eq!(&bytes[0..4], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[4..8], &(-2.0f32).to_le_bytes());
        assert_eq!(&bytes[8..12], &[1, 2, 3, 255]);
    }

    #[test]
    fn shaders_for_each_glsl_dialect() {
        for v in [
            ShaderVersion::Gl120,
            ShaderVersion::Gl140,
            ShaderVersion::Es100,
            ShaderVersion::Es300,
        ] {
            let vs = shader_source(v, true);
            let fs = shader_source(v, false);
            assert!(vs.starts_with(v.version_declaration()));
            assert_eq!(fs.contains("frag_color"), v.is_new_shader_interface());
            assert_eq!(vs.contains("precision highp"), v.is_embedded());
        }
    }
}
