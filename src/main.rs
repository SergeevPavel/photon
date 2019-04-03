extern crate euclid;
extern crate byteorder;
#[macro_use] extern crate log;

mod text;
mod dom;
mod transport;
mod perf;

use gleam::gl;
use std::fs::File;
use std::io::{Read, BufReader};

use glutin::{Event, ElementState, MouseButton, MouseScrollDelta};
use glutin::EventsLoop;
use glutin::GlContext;
use glutin::GlWindow;
use webrender::api::*;
use webrender::api::units::*;
use webrender::DebugFlags;
use webrender::Renderer;

use text::*;
use std::time::{Duration};
use std::env;
use std::net::{ToSocketAddrs, Ipv4Addr};
use serde::Deserialize;


fn create_window(events_loop: &EventsLoop) -> GlWindow {
    let window_builder = glutin::WindowBuilder::new()
        .with_title("photon")
        .with_resizable(false)
        .with_dimensions((1250, 900).into());
    let context = glutin::ContextBuilder::new()
        .with_vsync(true)
//        .with_double_buffer(Some(false))
//        .with_multisampling(4)
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
    let device_pixel_ratio = gl_window.get_hidpi_factor();
    let opts = webrender::RendererOptions {
        device_pixel_ratio: device_pixel_ratio as f32,
        ..webrender::RendererOptions::default()
    };
    let notifier = Box::new(Notifier::new(events_loop.create_proxy()));
    gl.clear_color(0.6, 0.6, 0.6, 1.0);
    gl.clear(gl::COLOR_BUFFER_BIT);
    gl.finish();
    return webrender::Renderer::new(gl, notifier, opts, None, framebuffer_size(gl_window)).unwrap();
}

fn framebuffer_size(gl_window: &GlWindow) -> FramebufferIntSize {
    let size = gl_window
        .get_inner_size()
        .unwrap()
        .to_physical(gl_window.get_hidpi_factor());
    FramebufferIntSize::new(size.width as i32, size.height as i32)
}

fn render_text_from_file(api: &RenderApi,
                         fonts_manager: &mut FontsManager,
                         pipeline_id: PipelineId,
                         document_id: DocumentId,
                         layout_size: LayoutSize,
                         epoch: &mut Epoch,
                         file_path: String) {
    let text_size = 16;
    let text = get_text(file_path);

    let mut txn = Transaction::new();
    let mut builder = DisplayListBuilder::new(pipeline_id, layout_size);

    let info = LayoutPrimitiveInfo::new(LayoutRect::new(LayoutPoint::zero(), builder.content_size()));
    let root_space_and_clip = SpaceAndClipInfo::root_scroll(pipeline_id);
    builder.push_simple_stacking_context(&info, root_space_and_clip.spatial_id);

    let scroll_content_box = euclid::TypedRect::new(
        euclid::TypedPoint2D::zero(),
        euclid::TypedSize2D::new(2000.0, 100000.0),
    );

    let scroll_space_and_clip = builder.define_scroll_frame(
        &root_space_and_clip,
        None,
        scroll_content_box,
        euclid::TypedRect::new(euclid::TypedPoint2D::zero(), layout_size),
        vec![],
        None,
        webrender::api::ScrollSensitivity::ScriptAndInputEvents,
        LayoutVector2D::new(0.0, 0.0),
    );

    let mut info = LayoutPrimitiveInfo::new(scroll_content_box);
    info.tag = Some((0, 1));
    builder.push_rect(&info,
                      &scroll_space_and_clip,
                      ColorF::new(0.9, 0.9, 0.91, 1.0)
    );

    fonts_manager.show_text(&mut builder, &scroll_space_and_clip, text.as_str(), LayoutPoint::new(0.0, text_size as f32));

    builder.pop_stacking_context();
    epoch.0 += 1;

    txn.set_display_list(
        epoch.clone(),
        None,
        layout_size,
        builder.finalize(),
        true,
    );

    txn.set_root_pipeline(pipeline_id);
    txn.generate_frame();
    api.send_transaction(document_id, txn);
}

fn run_event_loop<A: ToSocketAddrs>(render_server_addr: A) {
    let mut events_loop = EventsLoop::new();
    let gl_window = create_window(&events_loop);
    unsafe {
        gl_window.make_current().unwrap();
    }

    let (mut renderer, sender) = create_webrender(&gl_window, &events_loop);
    let api = sender.create_api();

    let framebuffer_size = framebuffer_size(&gl_window);

    let layout_size: LayoutSize = framebuffer_size.to_f32() / euclid::TypedScale::new(gl_window.get_hidpi_factor() as f32);

    let document_id = api.add_document(framebuffer_size, 0);
    let pipeline_id = webrender::api::PipelineId(0, 0);
//    let fonts_manager = text::FontsManager::new(sender.create_api(), document_id);
//    let mut epoch = Epoch(0);
//    render_text_from_file(&api, &fonts_manager, pipeline_id, document_id, layout_size, &mut epoch, "resources/EditorImpl.java".to_string());

    let mut txn = Transaction::new();
    txn.set_root_pipeline(pipeline_id);
    txn.generate_frame();
    api.send_transaction(document_id, txn);
    let mut controller = dom::NoriaClient::spawn(render_server_addr, sender.clone(), pipeline_id, document_id, layout_size);

    let mut cursor_position = WorldPoint::zero();

    events_loop.run_forever(|event| {
        let mut need_repaint = false;

        match event {
            Event::Awakened => {
                need_repaint = true;
            },
            Event::WindowEvent { event: window_event, .. } => match window_event {
                glutin::WindowEvent::CloseRequested
                => {
                    return glutin::ControlFlow::Break;
                }

                glutin::WindowEvent::KeyboardInput {
                    input: glutin::KeyboardInput {
                        state: glutin::ElementState::Pressed,
                        virtual_keycode: Some(code),
                        ..
                    },
                    ..
                } => {
                    match code {
                        glutin::VirtualKeyCode::P => {
                            let mut debug_flags = renderer.get_debug_flags();
                            debug_flags.toggle(DebugFlags::PROFILER_DBG);
                            debug_flags.toggle(DebugFlags::GPU_TIME_QUERIES);
                            api.send_debug_cmd(DebugCommand::SetFlags(debug_flags));
                        }
                        glutin::VirtualKeyCode::D => {
                            perf::print();
                        }
                        glutin::VirtualKeyCode::Space => {
                            {
                                let cursor_position = cursor_position.clone();
                                let mut controller = controller.clone();
                                std::thread::spawn(move || {
                                    for _ in 0..2000 {
                                        let delta = MouseScrollDelta::PixelDelta(glutin::dpi::LogicalPosition::new(0.0, -1.0));
                                        controller.mouse_wheel(cursor_position, delta);
                                        std::thread::sleep(Duration::from_millis(16));
                                    }
                                });

                            }
                        }
                        _ => {}
                    }
                }
                glutin::WindowEvent::CursorMoved {
                    position: glutin::dpi::LogicalPosition { x, y },
                    ..
                } => {
                    cursor_position = WorldPoint::new(x as f32, y as f32);
                }
                glutin::WindowEvent::MouseInput {
                    state, button, ..
                } => {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        controller.mouse_click(cursor_position);
                    }
                }
                glutin::WindowEvent::MouseWheel { delta, ..
                } => {
                    controller.mouse_wheel(cursor_position, delta);

//                    let mut txn = Transaction::new();
//                    txn.scroll(
//                        webrender::api::ScrollLocation::Delta(delta_vector),
//                        cursor_position,
//                    );
//                    txn.generate_frame();
//
//                    api.send_transaction(document_id, txn);
                }
                _ => {}
            },
            _ => {}
        }

        if need_repaint {
            perf::on_new_frame_ready();
            renderer.update();
            {
                let gl = renderer.device.rc_gl().as_ref();
                gl.clear_color(1.0, 1.0, 1.0, 0.0);
                gl.clear(gleam::gl::COLOR_BUFFER_BIT);
            }
            renderer.render(framebuffer_size).unwrap();
            renderer.flush_pipeline_info();
//            {
//                let gl = renderer.device.rc_gl().as_ref();
//                gl.flush();
//                gl.finish();
//            }
            perf::on_frame_rendered();
            gl_window.swap_buffers().unwrap();
            perf::on_new_frame_done();
        }

        return glutin::ControlFlow::Continue;
    });

    renderer.deinit();
}

fn get_text(file_path: String) -> String {
    let mut f = File::open(file_path).unwrap();
    let mut content = String::new();
    f.read_to_string(&mut content).unwrap();
    return content
}

#[derive(Deserialize)]
struct PortFileContent {
    #[serde(rename = "httpPort")]
    http_port: u16,
    #[serde(rename = "tcpPort")]
    tcp_port: u16
}

fn main() -> std::io::Result<()> {
    env_logger::init();
    perf::init();
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        let port_file = &args[1];
        let f = File::open(port_file).expect("No port file");
        let content: PortFileContent = serde_json::from_reader(BufReader::new(f)).unwrap();
        run_event_loop((Ipv4Addr::new(127, 0, 0, 1), content.tcp_port));
    }
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
        self.events_proxy.wakeup().unwrap();
    }

    fn new_frame_ready(
        &self,
        _document_id: webrender::api::DocumentId,
        _scrolled: bool,
        composite_needed: bool,
        _render_time: Option<u64>,
    ) {
        debug!("{:?} {:?} {:?} {:?}", _document_id, _scrolled, composite_needed, _render_time);
        if composite_needed {
            _render_time.map(|t| {
                let d = Duration::from_nanos(t);
                perf::on_wake_up(d);
            });
            self.events_proxy.wakeup().unwrap();
        }
    }
}