use std::fs::File;
use std::io::Read;

use glutin::EventsLoop;
use glutin::GlContext;
use glutin::GlWindow;

use webrender::Renderer;
use webrender::api::RenderApiSender;
use glutin::Event;

fn create_window(events_loop: &EventsLoop) -> GlWindow {
    let window_builder = glutin::WindowBuilder::new()
        .with_title("photon")
        .with_dimensions((800, 600).into());
    let context = glutin::ContextBuilder::new()
        .with_vsync(false)
        .with_multisampling(4)
        .with_srgb(true);
    return GlWindow::new(window_builder, context, &events_loop).unwrap();
}

fn create_webrender(gl_window: &GlWindow, events_loop: &EventsLoop) -> (Renderer, RenderApiSender) {
    let gl = match gl_window.get_api() {
        glutin::Api::OpenGl => unsafe {
            gleam::gl::GlFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _)
        },
        glutin::Api::OpenGlEs => unsafe {
            gleam::gl::GlesFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _)
        },
        glutin::Api::WebGl => unimplemented!(),
    };
    let mut device_pixel_ratio = gl_window.get_hidpi_factor();
    let opts = webrender::RendererOptions {
        device_pixel_ratio: device_pixel_ratio as f32,
        ..webrender::RendererOptions::default()
    };
    let notifier = Box::new(Notifier::new(events_loop.create_proxy()));
    return webrender::Renderer::new(gl, notifier, opts, None).unwrap();
}

fn framebuffer_size(gl_window: &GlWindow) -> webrender::api::DeviceIntSize {
    let size = gl_window
        .get_inner_size()
        .unwrap()
        .to_physical(gl_window.get_hidpi_factor());
    webrender::api::DeviceIntSize::new(size.width as i32, size.height as i32)
}

fn run_events_loop() {
    let mut events_loop = EventsLoop::new();
    let gl_window = create_window(&events_loop);
    unsafe {
        gl_window.make_current().unwrap();
    }

    let (mut renderer, sender) = create_webrender(&gl_window, &events_loop);
    let api = sender.create_api();

    let document_id = api.add_document(framebuffer_size(&gl_window), 0);
    let pipeline_id = webrender::api::PipelineId(0, 0);

    let mut txn = webrender::api::Transaction::new();
    txn.set_root_pipeline(pipeline_id);
    txn.generate_frame();
    api.send_transaction(document_id, txn);

    events_loop.run_forever(|event| {
        match event {
            Event::Awakened => {
                return glutin::ControlFlow::Continue;
            },
            Event::WindowEvent { event: window_event, .. } => match window_event {
                glutin::WindowEvent::CloseRequested => {
                    return glutin::ControlFlow::Break;
                }
                _ => {
                    return glutin::ControlFlow::Continue;
                }
            },
            _ => {
                return glutin::ControlFlow::Continue;
            }
        }
    });

    renderer.deinit();
}

fn main() -> std::io::Result<()> {
    let mut f = File::open("resources/Main.java")?;
    let mut content = String::new();
    f.read_to_string(&mut content)?;

    run_events_loop();

    return Ok(());
}

struct Notifier {
    events_proxy: glutin::EventsLoopProxy,
}

impl Notifier {
    fn new(events_proxy: glutin::EventsLoopProxy) -> Notifier {
        Notifier { events_proxy }
    }
}

impl webrender::api::RenderNotifier for Notifier {
    fn clone(&self) -> Box<webrender::api::RenderNotifier> {
        Box::new(Notifier {
            events_proxy: self.events_proxy.clone(),
        })
    }

    fn wake_up(&self) {
        self.events_proxy.wakeup().ok();
    }

    fn new_frame_ready(
        &self,
        _: webrender::api::DocumentId,
        _scrolled: bool,
        _composite_needed: bool,
        _render_time: Option<u64>,
    ) {
        self.wake_up();
    }
}