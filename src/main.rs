extern crate euclid;
extern crate byteorder;
extern crate crossbeam;
extern crate winit;
extern crate harfbuzz;
extern crate harfbuzz_sys;
#[macro_use] extern crate log;
#[macro_use] extern crate thread_profiler;

#[cfg(feature = "dx12")]
extern crate gfx_backend_dx12 as back;
#[cfg(feature = "metal")]
extern crate gfx_backend_metal as back;
#[cfg(feature = "vulkan")]
extern crate gfx_backend_vulkan as back;

mod text;
mod dom;
mod transport;
mod perf;
mod text_layout;

use std::fs::File;
use std::io::{Read, BufReader};

use webrender::api::*;
use webrender::DebugFlags;
use webrender::Renderer;
#[cfg(feature = "gfx-hal")]
use webrender::hal::Instance;

use text::*;
use std::time::{Duration};
use std::env;
use std::net::{ToSocketAddrs, Ipv4Addr};
use serde::Deserialize;
use crossbeam::crossbeam_channel::{Sender, Receiver};
use winit::{Window, EventsLoop, MouseScrollDelta, Event, ControlFlow, ElementState, MouseButton, WindowEvent};


fn create_webrender(event_loop: &winit::EventsLoop, notifier: Box<RenderNotifier>) -> (Window, Renderer<back::Backend>, RenderApiSender) {
    let window_builder = winit::WindowBuilder::new()
        .with_title("photon")
        .with_resizable(false)
        .with_dimensions((1250, 900).into());

    let (window, instance, adapter, surface) = {
        let window = window_builder.build(&event_loop).unwrap();
        let instance = back::Instance::create("gfx-rs instance", 1);
        let mut adapters = instance.enumerate_adapters();
        let adapter = adapters.remove(0);
        let mut surface = instance.create_surface(&window);
        (window, instance, adapter, surface)
    };

    let device_pixel_ratio = window.get_hidpi_factor();
    let opts = webrender::RendererOptions {
        debug_flags: webrender::DebugFlags::PROFILER_DBG,
        clear_color: Some(ColorF::WHITE),
        device_pixel_ratio: device_pixel_ratio as f32,
        ..webrender::RendererOptions::default()
    };
    let winit::dpi::LogicalSize { width, height } = window.get_inner_size().unwrap();
    let init = {
        use std::path::PathBuf;
        let cache_dir = dirs::cache_dir().expect("User's cache directory not found");
        let cache_path = Some(PathBuf::from(&cache_dir).join("pipeline_cache.bin"));

        webrender::DeviceInit {
            instance: Box::new(instance),
            adapter,
            surface: Some(surface),
            window_size: (width as i32, height as i32),
            descriptor_count: None,
            cache_path,
            save_cache: true,
        }
    };
    let (renderer, sender) = webrender::Renderer::new(init, notifier, opts, None).unwrap();
    return (window, renderer, sender);
}

fn framebuffer_size(window: &Window) -> DeviceIntSize {
    let device_pixel_ratio = window.get_hidpi_factor() as f32;

    let framebuffer_size = {
        let size = window
            .get_inner_size()
            .unwrap()
            .to_physical(device_pixel_ratio as f64);
        DeviceIntSize::new(size.width as i32, size.height as i32)
    };

    framebuffer_size
}

enum UserEvent {
    Scroll { cursor_position: WorldPoint, delta: MouseScrollDelta },
    Repaint
}

#[derive(Clone)]
struct EventLoopNotifier {
    events_proxy: winit::EventsLoopProxy,
    pub recv: Receiver<UserEvent>,
    sndr: Sender<UserEvent>,
}

impl EventLoopNotifier {
    fn new(event_loop: &EventsLoop) -> Self {
        let (sndr, recv) = crossbeam::crossbeam_channel::unbounded();
        EventLoopNotifier {
            events_proxy: event_loop.create_proxy(),
            sndr,
            recv
        }
    }
    fn send(&self, event: UserEvent) {
        self.sndr.send(event).unwrap();
        self.events_proxy.wakeup();
    }
}

impl webrender::api::RenderNotifier for EventLoopNotifier {
    fn clone(&self) -> Box<RenderNotifier> {
        Box::new(EventLoopNotifier {
            events_proxy: self.events_proxy.clone(),
            recv: self.recv.clone(),
            sndr: self.sndr.clone()
        })
    }

    fn wake_up(&self) {
        self.send(UserEvent::Repaint)
    }

    fn new_frame_ready(&self, document_id: DocumentId, scrolled: bool, composite_needed: bool, render_time_ns: Option<u64>) {
        profile_scope!("wake up!");
        debug!("{:?} {:?} {:?} {:?}", document_id, scrolled, composite_needed, render_time_ns);
        if composite_needed {
            render_time_ns.map(|t| {
                perf::on_wake_up(Duration::from_nanos(t));
            });
            self.send(UserEvent::Repaint);
        }
    }
}

fn run_event_loop<A: ToSocketAddrs>(render_server_addr: A) {
    let mut events_loop = EventsLoop::new();
    let notifier = EventLoopNotifier::new(&events_loop);

    let (mut window, mut renderer, sender) = create_webrender(&events_loop, webrender::api::RenderNotifier::clone(&notifier));
    let api = sender.create_api();

    let framebuffer_size = framebuffer_size(&window);

    let layout_size: LayoutSize = framebuffer_size.to_f32() / euclid::TypedScale::new(window.get_hidpi_factor() as f32);

    let document_id = api.add_document(framebuffer_size, 0);
    let pipeline_id = webrender::api::PipelineId(0, 0);

    let mut txn = Transaction::new();
    txn.set_root_pipeline(pipeline_id);
    txn.generate_frame();
    api.send_transaction(document_id, txn);
    let mut controller = dom::NoriaClient::spawn(render_server_addr, sender.clone(), pipeline_id, document_id, layout_size);

    let mut cursor_position = WorldPoint::zero();

    events_loop.run_forever(|event| {
        match event {
            Event::Awakened => {
                match notifier.recv.recv().unwrap() {
                    UserEvent::Scroll { cursor_position, delta } => {
                        controller.mouse_wheel(cursor_position, delta);
                    },
                    UserEvent::Repaint => {
                        perf::on_new_frame_ready();
                        renderer.update();
                        renderer.render(framebuffer_size).unwrap();
                        renderer.flush_pipeline_info();
                        perf::on_frame_rendered();
                    },
                }
            },
            Event::WindowEvent { event: window_event, .. } => match window_event {
                WindowEvent::CloseRequested
                => {
                    return ControlFlow::Break;
                }

                WindowEvent::KeyboardInput {
                    input: winit::KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(code),
                        ..
                    },
                    ..
                } => {
                    match code {
                        winit::VirtualKeyCode::P => {
//                            let debug_renderer = renderer.debug_renderer().unwrap();
//                            debug_renderer.add_text(10.0, 10.0, &"hello", ColorU::new(0, 0, 0, 255), None);
//                            debug_renderer.render();
                            let mut debug_flags = renderer.get_debug_flags();
                            debug_flags.toggle(DebugFlags::PROFILER_DBG);
                            debug_flags.toggle(DebugFlags::GPU_TIME_QUERIES);
                            debug_flags.toggle(DebugFlags::RENDER_TARGET_DBG);
                            api.send_debug_cmd(DebugCommand::SetFlags(debug_flags));
                        }
                        winit::VirtualKeyCode::D => {
                            perf::print();
                        }
                        winit::VirtualKeyCode::W => {
                            println!("Drop profile");
                            renderer.save_cpu_profile("profile.json");
                        }
                        winit::VirtualKeyCode::Space => {
                            let sender = Clone::clone(&notifier);
                            let position = cursor_position;
                            std::thread::spawn(move || {
                                for _ in 0..2000 {
                                    let delta = MouseScrollDelta::PixelDelta(winit::dpi::LogicalPosition::new(0.0, -1.0));
                                    sender.send(UserEvent::Scroll {cursor_position: position, delta});
                                    std::thread::sleep(Duration::from_millis(16));
                                }
                            });
                        }
                        _ => {}
                    }
                }
                winit::WindowEvent::CursorMoved {
                    position: winit::dpi::LogicalPosition { x, y },
                    ..
                } => {
                    cursor_position = WorldPoint::new(x as f32, y as f32);
                }
                winit::WindowEvent::MouseInput {
                    state, button, ..
                } => {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        controller.mouse_click(cursor_position);
                    }
                }
                winit::WindowEvent::MouseWheel { delta, ..
                } => {
                    controller.mouse_wheel(cursor_position, delta);
                }
                _ => {}
            },
            _ => {}
        }
        return ControlFlow::Continue;
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