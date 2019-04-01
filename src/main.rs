extern crate euclid;
extern crate byteorder;

mod text;
mod dom;
mod transport;

use std::fs::File;
use std::io::{Read, BufReader};

use glutin::{Event, ElementState, MouseButton};
use glutin::EventsLoop;
use glutin::GlContext;
use glutin::GlWindow;
use webrender::api::*;
use webrender::api::units::*;
use webrender::DebugFlags;
use webrender::Renderer;

use text::*;
use std::time::{SystemTime, Instant, UNIX_EPOCH};
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
        .with_double_buffer(Some(true))
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
    return webrender::Renderer::new(gl, notifier, opts, None).unwrap();
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
//
//    let fonts_manager = text::FontsManager::new(sender.create_api(), document_id);
//    let mut epoch = Epoch(0);
//    render_text_from_file(&api, &fonts_manager, pipeline_id, document_id, layout_size, &mut epoch, "resources/EditorImpl.java".to_string());

    let mut txn = Transaction::new();
    txn.set_root_pipeline(pipeline_id);
    api.send_transaction(document_id, txn);
    let mut controller = dom::NoriaClient::spawn(render_server_addr, sender.clone(), pipeline_id, document_id, layout_size);

    let mut cursor_position = WorldPoint::zero();
    let mut perf_log = vec![];
    let mut base_time = SystemTime::now();

    events_loop.run_forever(|event| {
        let mut need_repaint = false;

        match event {
            Event::Awakened => {
//                println!("awake");
//                perf_log.push((SystemTime::now().duration_since(base_time).unwrap().as_millis(), "Awakened"));
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
                            for e in &perf_log {
                                println!("{:?}", e);
                            }
                            perf_log.clear();
                            base_time = SystemTime::now()
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
                    let hit_result = api.hit_test(document_id, Some(pipeline_id), cursor_position, HitTestFlags::empty());
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        controller.mouse_click(hit_result);
                    }
                }
                glutin::WindowEvent::MouseWheel { delta, ..
                } => {
                    let hit_result = api.hit_test(document_id, Some(pipeline_id), cursor_position, HitTestFlags::empty());
                    if hit_result.items.len() > 0 {
                        perf_log.push((SystemTime::now().duration_since(base_time).unwrap().as_millis(), "MouseWheel"));
                    }
                    const LINE_HEIGHT: f32 = 38.0; // TODO treat LineDelta in other place?
                    let delta_vector = match delta {
                        glutin::MouseScrollDelta::LineDelta(dx, dy) => LayoutVector2D::new(-dx, -dy * LINE_HEIGHT),
                        glutin::MouseScrollDelta::PixelDelta(pos) => LayoutVector2D::new(-pos.x as f32, -pos.y as f32),
                    };
                    controller.mouse_wheel(hit_result, delta_vector);

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
//            {
//                let debug_render = renderer.debug_renderer().unwrap();
//                debug_render.add_text(100.0, 100.0, "hello", ColorU::new(0, 0, 0, 255), None);
//
//            }
            println!("{} ready", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()  % 1000);
//            perf_log.push((SystemTime::now().duration_since(base_time).unwrap().as_millis(), "Begin render"));
            let start = Instant::now();
            renderer.update();
            perf_log.push((start.elapsed().as_millis(), "Update took"));
            let start = Instant::now();
            renderer.render(framebuffer_size).unwrap();
            renderer.flush_pipeline_info();
            perf_log.push((start.elapsed().as_millis(), "Render took"));
            let start = Instant::now();
            gl_window.swap_buffers().unwrap();
            perf_log.push((start.elapsed().as_millis(), "Swap buffers took"));
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
        println!("wakeup!");
        self.events_proxy.wakeup().unwrap();
    }

    fn new_frame_ready(
        &self,
        _document_id: webrender::api::DocumentId,
        _scrolled: bool,
        composite_needed: bool,
        _render_time: Option<u64>,
    ) {
//        println!("{:?} {:?} {:?} {:?}", _document_id, _scrolled, composite_needed, _render_time.map(|t| t as f32 / 1000_000.0));
        if composite_needed {
            self.events_proxy.wakeup().unwrap();
        }
    }
}