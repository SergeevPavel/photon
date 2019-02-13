
extern crate euclid;

use std::fs::File;
use std::io::Read;
use std::path::Path;

use glutin::Event;
use glutin::EventsLoop;
use glutin::GlContext;
use glutin::GlWindow;
use webrender::api::*;
use webrender::Renderer;

use euclid::TypedPoint2D;

mod text;

use text::*;
use app_units::Au;

fn create_window(events_loop: &EventsLoop) -> GlWindow {
    let window_builder = glutin::WindowBuilder::new()
        .with_title("photon")
        .with_resizable(false)
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

fn init_font(api: &RenderApi, document_id: DocumentId, pipeline_id: PipelineId, font_size: i32) -> (FontKey, FontInstanceKey) {
    let mut txn = Transaction::new();
    txn.set_root_pipeline(pipeline_id);
    let font_key = text::add_font(&api, &mut txn, "resources/Fira Code/ttf/FiraCode-Medium.ttf");
    let font_instance_key = add_font_instance(&api, &mut txn, font_key, font_size);
    api.send_transaction(document_id, txn);
    return (font_key, font_instance_key);
}

fn run_events_loop() {
    let mut events_loop = EventsLoop::new();
    let gl_window = create_window(&events_loop);
    unsafe {
        gl_window.make_current().unwrap();
    }

    let (mut renderer, sender) = create_webrender(&gl_window, &events_loop);
    let api = sender.create_api();

    let framebuffer_size = framebuffer_size(&gl_window);
    let document_id = api.add_document(framebuffer_size, 0);
    let pipeline_id = webrender::api::PipelineId(0, 0);
    let epoch = Epoch(0);

    let text_size = 16;
    let text = get_text();
    let (font_key, font_instance_key) = init_font(&api, document_id, pipeline_id, text_size);

    let mut txn = Transaction::new();
    let layout_size = framebuffer_size.to_f32() / euclid::TypedScale::new(gl_window.get_hidpi_factor() as f32);
    let mut builder = DisplayListBuilder::new(pipeline_id, layout_size);

    let mut info = LayoutPrimitiveInfo::new(LayoutRect::new(LayoutPoint::zero(), builder.content_size()));
    let root_space_and_clip = SpaceAndClipInfo::root_scroll(pipeline_id);
    builder.push_simple_stacking_context(&info, root_space_and_clip.spatial_id);

    let scroll_content_box = euclid::TypedRect::new(
        euclid::TypedPoint2D::zero(),
        euclid::TypedSize2D::new(100000.0, 100000.0),
    );
    let scroll_space_and_clip = builder.define_scroll_frame(
        &root_space_and_clip,
        None,
        scroll_content_box,
        euclid::TypedRect::new(euclid::TypedPoint2D::zero(), layout_size),
        vec![],
        None,
        webrender::api::ScrollSensitivity::ScriptAndInputEvents,
    );

    let mut info = LayoutPrimitiveInfo::new(scroll_content_box);
    info.tag = Some((0, 1));
    builder.push_rect(&info,
                      &scroll_space_and_clip,
                      ColorF::new(0.9, 0.9, 0.91, 1.0));

    show_text(&api,
              font_key,
              text_size,
              font_instance_key,
              &mut builder,
              &scroll_space_and_clip,
              text.as_str(),
              LayoutPoint::new(0.0, 0.0));

    builder.pop_stacking_context();

    txn.set_display_list(
        epoch,
        None,
        layout_size,
        builder.finalize(),
        true,
    );

    txn.set_root_pipeline(pipeline_id);
    txn.generate_frame();
    api.send_transaction(document_id, txn);

    let mut cursor_position = webrender::api::WorldPoint::zero();
    events_loop.run_forever(|event| {
        match event {
            Event::Awakened => {},
            Event::WindowEvent { event: window_event, .. } => match window_event {
                glutin::WindowEvent::CloseRequested => {
                    return glutin::ControlFlow::Break;
                }
                glutin::WindowEvent::CursorMoved {
                    position: glutin::dpi::LogicalPosition { x, y },
                    ..
                } => {
                    cursor_position = webrender::api::WorldPoint::new(x as f32, y as f32);
                    return glutin::ControlFlow::Continue;
                }
                glutin::WindowEvent::MouseWheel { delta, .. } => {
                    const LINE_HEIGHT: f32 = 38.0;
                    let (dx, dy) = match delta {
                        glutin::MouseScrollDelta::LineDelta(dx, dy) => (dx, dy * LINE_HEIGHT),
                        glutin::MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                    };

                    let mut txn = Transaction::new();
                    txn.scroll(
                        webrender::api::ScrollLocation::Delta(webrender::api::LayoutVector2D::new(
                            dx, dy,
                        )),
                        cursor_position,
                    );
                    txn.generate_frame();

                    api.send_transaction(document_id, txn);
                }
                _ => {}
            },
            _ => {}
        }


        renderer.update();
        renderer.render(framebuffer_size);
        let pp_info = renderer.flush_pipeline_info();
        gl_window.swap_buffers().unwrap();
        return glutin::ControlFlow::Continue;
    });

    renderer.deinit();
}

fn show_text(api: &RenderApi,
             font_key: FontKey,
             text_size: i32,
             font_instance_key: FontInstanceKey,
             builder: &mut DisplayListBuilder,
             space_and_clip: &SpaceAndClipInfo,
             text: &str,
             origin: LayoutPoint) {
    let (indices, positions, bounding_rect) = layout_simple_ascii(&api,
                                                                  font_key,
                                                                  font_instance_key,
                                                                  text,
                                                                  Au::from_px(text_size),
                                                                  origin,
                                                                  FontInstanceFlags::default());
    let glyphs: Vec<GlyphInstance> = indices.iter().zip(positions)
        .map(|(idx, pos)| GlyphInstance { index: *idx, point: pos })
        .collect();
    let info = LayoutPrimitiveInfo::new(bounding_rect);
    builder.push_text(
        &info,
        &space_and_clip,
        glyphs.as_slice(),
        font_instance_key,
        ColorF::BLACK,
        None,
    );
}

fn get_text() -> String {
    let mut f = File::open("resources/Either.java").unwrap();
    let mut content = String::new();
    f.read_to_string(&mut content).unwrap();
    return content
}

fn main() -> std::io::Result<()> {
    env_logger::init();
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