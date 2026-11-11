use zui_api::{event::*, widget::Invalidation};
use zui_opengl::{
    OpenGlError, OpenGlFrame, OpenGlRenderer,
    glow::{self, HasContext},
};

#[derive(Default)]
pub struct Triangle {
    program: Option<glow::Program>,
    vao: Option<glow::VertexArray>,
    shift: f32,
}
impl OpenGlRenderer for Triangle {
    fn initialize(&mut self, gl: &glow::Context) -> Result<(), OpenGlError> {
        unsafe {
            let program = gl.create_program().map_err(OpenGlError::Backend)?;
            self.program = Some(program);
            self.vao = Some(gl.create_vertex_array().map_err(OpenGlError::Backend)?);
            let sources = [
                (
                    glow::VERTEX_SHADER,
                    r#"#version 330 core
                    uniform float angle;
                    uniform float aspect;
                    out vec3 color;
                    const vec2 points[3] = vec2[3](vec2(-0.6,-0.5),vec2(0.6,-0.5),vec2(0.0,0.6));
                    const vec3 colors[3] = vec3[3](vec3(1.0,0.35,0.25),vec3(0.25,0.8,0.9),vec3(0.65,0.45,1.0));
                    void main() {
                        vec2 p = mat2(cos(angle),sin(angle),-sin(angle),cos(angle))*points[gl_VertexID];
                        gl_Position = vec4(p.x/aspect,p.y,0.0,1.0);
                        color=colors[gl_VertexID];
                    }"#,
                ),
                (
                    glow::FRAGMENT_SHADER,
                    r#"#version 330 core
                    in vec3 color;
                    out vec4 output_color;
                    void main() { output_color=vec4(color,1.0); }"#,
                ),
            ];
            for (kind, source) in sources {
                let shader = gl.create_shader(kind).map_err(OpenGlError::Backend)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    let log = gl.get_shader_info_log(shader);
                    gl.delete_shader(shader);
                    return Err(OpenGlError::Backend(log));
                }
                gl.attach_shader(program, shader);
                gl.delete_shader(shader);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(OpenGlError::Backend(gl.get_program_info_log(program)));
            }
            eprintln!(
                "OpenGlView driver: {}",
                gl.get_parameter_string(glow::VERSION)
            );
        }
        Ok(())
    }
    fn render(&mut self, gl: &glow::Context, frame: OpenGlFrame) -> Result<(), OpenGlError> {
        unsafe {
            gl.clear_color(0.055, 0.075, 0.11, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);
            gl.use_program(self.program);
            gl.bind_vertex_array(self.vao);
            let program = self.program.unwrap();
            gl.uniform_1_f32(
                gl.get_uniform_location(program, "angle").as_ref(),
                frame.elapsed.as_secs_f32() * 0.4 + self.shift,
            );
            gl.uniform_1_f32(
                gl.get_uniform_location(program, "aspect").as_ref(),
                frame.extent.width as f32 / frame.extent.height as f32,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
        Ok(())
    }
    fn input(&mut self, _: EventContext, event: &WidgetEvent) -> EventResponse {
        let step = match event {
            WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                ..
            }) => 0.4,
            WidgetEvent::Input(InputEvent::Key(k)) if k.pressed && k.key == Key::ArrowLeft => -0.2,
            WidgetEvent::Input(InputEvent::Key(k)) if k.pressed && k.key == Key::ArrowRight => 0.2,
            _ => return EventResponse::default(),
        };
        self.shift += step;
        EventResponse {
            handled: true,
            invalidation: Invalidation::Paint,
            ..Default::default()
        }
    }
    fn destroy(&mut self, gl: &glow::Context) {
        unsafe {
            if let Some(vao) = self.vao.take() {
                gl.delete_vertex_array(vao);
            }
            if let Some(program) = self.program.take() {
                gl.delete_program(program);
            }
        }
    }
}
