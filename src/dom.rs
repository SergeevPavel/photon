
extern crate fxhash;
use fxhash::FxHashMap;


use std::io::prelude::*;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{SystemTime, UNIX_EPOCH};

use byteorder::{ReadBytesExt, BigEndian};
use serde_json::Value;
use webrender::api::*;
use webrender::api::units::*;

use crate::{text, perf};
use crate::transport::*;

use euclid::TypedSize2D;
use std::sync::{Mutex, Arc};
use serde::{Serialize};
use crate::text::FontsManager;
use glutin::MouseScrollDelta;
use thread_profiler::{register_thread_with_profiler};

fn read_msg(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let size = stream.read_u32::<BigEndian>().unwrap();
    let mut buf = vec![0u8; size as usize];
    if stream.read_exact(&mut buf).is_ok() {
        Some(buf)
    } else {
        None
    }
}

fn current_ts() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()
}

type NodeId = u64;

#[derive(Debug)]
enum Callback {
    Sync,
    Async,
    None,
}

impl Callback {
    fn is_some(&self) -> bool {
        match self {
            Callback::None => false,
            _ => true
        }
    }
}

const FANCY_GREEN: ColorF = ColorF {
    r: 0.1,
    g: 0.8,
    b: 0.5,
    a: 1.0,
};

#[derive(Debug)]
enum NodeType {
    Root,
    Div { color: ColorF, rect: LayoutRect, on_click: Callback, on_wheel: Callback },
    Text { text: String, origin: LayoutPoint, layouted_text: Option<text::LayoutedText>, color: ColorF },
    Scroll { position: LayoutRect,
             content: LayoutRect,
             on_wheel: Callback },
}

fn parse_rect(value: &Value) -> LayoutRect {
    euclid::rect(value["x"].as_f64().unwrap() as f32,
                 value["y"].as_f64().unwrap() as f32,
                 value["width"].as_f64().unwrap() as f32,
                 value["height"].as_f64().unwrap() as f32)
}

fn parse_point(value: &Value) -> LayoutPoint {
    LayoutPoint::new(value["x"].as_f64().unwrap() as f32,
                     value["y"].as_f64().unwrap() as f32)
}

fn parse_callback(value: &Value) -> Callback {
    match value.as_str().unwrap() {
        "noria-handler-sync" => { Callback::Sync }
        "noria-handler-async" => { Callback::Async }
        "-noria-handler" => { Callback::None }
        _ => unreachable!()
    }
}

#[derive(Serialize)]
struct CallbackMessage<'a, T: Serialize> {
    log_id: u64,
    ts: u64,
    node: NodeId,
    key: &'a str,
    arguments: T,
}

impl NodeType {
    fn create(node_type: &str) -> NodeType {
        // TODO support constructor params
        match node_type {
            "root" => {
                NodeType::Root
            }
            "text" => {
                let default_text = "".to_string();
                let default_origin = LayoutPoint::new(0.0, 0.0);
                NodeType::Text {
                    text: default_text,
                    origin: default_origin,
                    layouted_text: None,
                    color: ColorF::BLACK,
                }
            }
            "div" => {
                NodeType::Div {
                    color: ColorF::BLACK,
                    rect: LayoutRect::new(LayoutPoint::zero(),
                                          LayoutSize::new(0.0, 0.0)),
                    on_click: Callback::None,
                    on_wheel: Callback::None
                }
            }
            "scroll" => {
                NodeType::Scroll {
                    position: LayoutRect::new(LayoutPoint::zero(),
                                              LayoutSize::new(0.0, 0.0)),
                    content: LayoutRect::new(LayoutPoint::zero(),
                                             LayoutSize::new(0.0, 0.0)),
                    on_wheel: Callback::None
                }
            }
            _ => unreachable!("Unknown type {}", node_type)
        }
    }

    fn set_attr(&mut self, context: &mut ApplyUpdatesContext, node_id: NodeId, attribute: &str, value: &Value) -> bool {
        match self {
            NodeType::Root => {

            }
            NodeType::Div { ref mut color, rect, on_click, on_wheel } => {
                match attribute {
                    "color" => {
                        *color = ColorF::WHITE; // parse color
                    }
                    "rect" => {
                        *rect = parse_rect(value);
                    }
                    "on-click" => {
                        *on_click = parse_callback(value);
                    }
                    "on-wheel" => {
                        *on_wheel = parse_callback(value);
                    }

                    _ => ()
                }
            }
            NodeType::Scroll { ref mut position, content, on_wheel } => {
                match attribute {
                    "position" => {
                        *position = parse_rect(value);
                    }
                    "content" => {
                        *content = parse_rect(value);
                    }
                    "scroll-position" => {
                        let x = value["x"].as_f64().unwrap() as f32;
                        let y = value["y"].as_f64().unwrap() as f32;
                        context.txn.scroll_node_with_id(LayoutPoint::new(x, y), ExternalScrollId(node_id, context.pipeline_id), ScrollClamping::ToContentBounds);
                        return false;
                    }
                    "on-wheel" => {
                        *on_wheel = parse_callback(value);
                    }

                    _ => ()
                }
            }
            NodeType::Text { ref mut text, origin, layouted_text, color } => {
                match attribute {
                    "text" => {
                        *text = value.as_str().unwrap().to_string();
                        *layouted_text = Some(context.fonts_manager.layout_simple_ascii(text,
                                                                                        LayoutPoint::new(0.0, 0.0),
                                                                                        FontInstanceFlags::default()));
                    }
                    "origin" => {
                        *origin = parse_point(value);
                    }
                    "color" => {
                        *color = ColorU::new(value["r"].as_u64().unwrap() as u8,
                                             value["g"].as_u64().unwrap() as u8,
                                             value["b"].as_u64().unwrap() as u8,
                                             value["a"].as_u64().unwrap() as u8).into();
                    }
                    _ => ()
                }
            }
        }
        return true;
    }

    fn visit_down(&self, node_id: NodeId, context: &mut VisitorContext) {
        match self {
            NodeType::Root => {
                let info = LayoutPrimitiveInfo::new(LayoutRect::new(LayoutPoint::zero(), context.builder.content_size()));
                let root_space_and_clip = SpaceAndClipInfo::root_scroll(context.builder.pipeline_id);
                context.space_and_clip_stack.push(root_space_and_clip);
                context.builder.push_simple_stacking_context(&info, root_space_and_clip.spatial_id);
            }
            NodeType::Div { color, rect, on_click, on_wheel } => {
                let mut info = LayoutPrimitiveInfo::new(*rect);
                let space_and_clip = context.space_and_clip_stack.last().unwrap();
                let widths = LayoutSideOffsets::new(1.0, 1.0, 1.0, 1.0);
                let border_color = ColorF::TRANSPARENT;
                let border_details = BorderDetails::Normal(NormalBorder {
                    left: BorderSide {
                        color: border_color,
                        style: BorderStyle::Solid
                    },
                    right: BorderSide {
                        color: border_color,
                        style: BorderStyle::Solid
                    },
                    top: BorderSide {
                        color: border_color,
                        style: BorderStyle::Solid
                    },
                    bottom: BorderSide {
                        color: border_color,
                        style: BorderStyle::Solid
                    },

                    radius: BorderRadius {
                        top_left: TypedSize2D::new(3.0, 3.0),
                        top_right: TypedSize2D::new(3.0, 3.0),
                        bottom_left: TypedSize2D::new(3.0, 3.0),
                        bottom_right: TypedSize2D::new(3.0, 3.0)
                    },
                    do_aa: true
                });
                if on_click.is_some() || on_wheel.is_some() {
                    info.tag = Some((node_id, 0));
                }
                context.builder.push_border(&info, &space_and_clip, widths, border_details);
                context.builder.push_simple_stacking_context(&info, space_and_clip.spatial_id);
            }
            NodeType::Scroll { position, content, on_wheel, .. } => {
                let parent_space_and_clip = context.space_and_clip_stack.last().unwrap();
                let scroll_space_and_clip = context.builder.define_scroll_frame(&parent_space_and_clip,
                                                                                Some(ExternalScrollId(node_id, context.builder.pipeline_id)),
                                                                                *content,
                                                                                *position,
                                                                                vec![],
                                                                                None,
                                                                                webrender::api::ScrollSensitivity::ScriptAndInputEvents,
                                                                                LayoutVector2D::new(0.0, 0.0));
                context.space_and_clip_stack.push(scroll_space_and_clip);
                let mut info = LayoutPrimitiveInfo::new(*content);
                if on_wheel.is_some() {
                    info.tag = Some((node_id, 0));
                }
                context.builder.push_rect(&info,
                                          &scroll_space_and_clip,
                                          ColorF::TRANSPARENT);
            }
            NodeType::Text { text, origin, layouted_text, color } => {
                if let Some(parent_space_and_clip) = context.space_and_clip_stack.last() {
                    if let Some(layouted_text) = layouted_text {
                        let info = LayoutPrimitiveInfo::new(LayoutRect::new(*origin, layouted_text.bounding_rect.size));
                        context.builder.push_simple_stacking_context(&info, parent_space_and_clip.spatial_id);
                        context.builder.push_text(&LayoutPrimitiveInfo::new(layouted_text.bounding_rect),
                                                  &parent_space_and_clip,
                                                  layouted_text.glyphs.as_slice(),
                                                  context.fonts_manager.font_instance_key,
                                                  *color,
                                                  None);
                    }
                } else {
                    unreachable!("No parent space and clip");
                }
            }
        }
    }

    fn visit_up(&self, node_id: NodeId, context: &mut VisitorContext) {
        match self {
            NodeType::Root => {
                context.builder.pop_stacking_context();
                assert!(context.space_and_clip_stack.pop().is_some());
            }

            NodeType::Div { .. } => {
                context.builder.pop_stacking_context();
            }

            NodeType::Scroll { .. } => {
                assert!(context.space_and_clip_stack.pop().is_some());
            }

            NodeType::Text { .. } => {
                context.builder.pop_stacking_context();
            }
        }
    }

    fn on_click(&self, stream: &mut TcpStream, log_id: u64, node_id: NodeId, point: &LayoutPoint) {
        match self {
            NodeType::Div { on_click, .. } => {
                if on_click.is_some() {
                    let msg = CallbackMessage {
                        node: node_id,
                        ts: current_ts() as u64,
                        log_id,
                        key: "on-click",
                        arguments: vec![point.x, point.y]
                    };
                    stream.write(serde_json::to_string(&msg).unwrap().as_bytes()).unwrap();
                }
            }
            _ => ()
        }
    }

    fn on_wheel(&self, stream: &mut TcpStream, log_id: u64, node_id: NodeId, delta: &LayoutVector2D) {
        match self {
            NodeType::Scroll { on_wheel, .. } => {
                if on_wheel.is_some() {
                    let msg = CallbackMessage {
                        node: node_id,
                        ts: current_ts() as u64,
                        log_id,
                        key: "on-wheel",
                        arguments: vec![delta.x, delta.y]
                    };
                    stream.write(serde_json::to_string(&msg).unwrap().as_bytes()).unwrap();
                }
            }
            NodeType::Div { on_wheel, .. } => {
                if on_wheel.is_some() {
                    let msg = CallbackMessage {
                        node: node_id,
                        ts: current_ts() as u64,
                        log_id,
                        key: "on-wheel",
                        arguments: vec![delta.x, delta.y]
                    };
                    stream.write(serde_json::to_string(&msg).unwrap().as_bytes()).unwrap();
                }
            }
            _ => ()
        }
    }
}

#[derive(Debug)]
struct Node {
    id: NodeId,
    node_type: NodeType,
    children: Vec<NodeId>,
}

struct VisitorContext<'a> {
    builder: DisplayListBuilder,
    space_and_clip_stack: Vec<SpaceAndClipInfo>,
    api: &'a RenderApi,
    nodes: &'a FxHashMap<NodeId, Node>,
    fonts_manager: &'a FontsManager
}

struct ApplyUpdatesContext<'a> {
    pipeline_id: PipelineId,
    txn: &'a mut Transaction,
    fonts_manager: &'a mut FontsManager
}

impl Node {
    fn visit(&self, context: &mut VisitorContext) {
        self.node_type.visit_down(self.id, context);
        for child_id in &self.children {
            let node = context.nodes.get(&child_id).unwrap();
            node.visit(context);
        }
        self.node_type.visit_up(self.id, context);
    }
}

#[derive(Debug, Default)]
struct Dom {
    nodes: FxHashMap<NodeId, Node>,
    root_node: Option<NodeId>,
}

fn apply_updates(dom: &mut Dom, context: &mut ApplyUpdatesContext, message: &Vec<u8>) -> (bool, Vec<u64>) {
    profile_scope!("apply updates");
    let updates = if let Some(updates) = serde_json::from_slice::<NoriaUpdates>(&message).ok() {
        updates
    } else {
        println!("{}", String::from_utf8(message.clone()).unwrap());
        panic!()
    };
    let mut need_rebuild = false;
    let mut log_ids = vec![0; 0];
    for update in updates {
        match update {
            UpdateOrLogId::LogIds(ids) => {
                log_ids = ids;
                perf::on_get_noria_message(log_ids.clone());
            }
            UpdateOrLogId::Update(update) => {
                match update {
                    Update::MakeNode(MakeNode { node_id, node_type }) => {
                        let node_type = NodeType::create(node_type.as_str());
                        if let NodeType::Root = node_type {
                            dom.root_node = Some(node_id);
                        }
                        let mut node = Node {
                            id: node_id,
                            node_type: node_type,
                            children: Vec::new(),
                        };
                        dom.nodes.insert(node_id, node);
                        need_rebuild = true;
                    }
                    Update::Destroy(Destroy { node_id }) => {
                        assert!(dom.nodes.remove(&node_id).is_some());
                        need_rebuild = true;
                    }
                    Update::Add(Add { node_id, attribute, index, value }) => {
                        if attribute == "children" {
                            let value = value.as_u64().unwrap();
                            assert!(dom.nodes.contains_key(&value));
                            let node = dom.nodes.get_mut(&node_id).unwrap();
                            node.children.insert(index as usize, value);
                        }
                        need_rebuild = true;
                    }
                    Update::Remove(Remove { node_id, attribute, value }) => {
                        if attribute == "children" {
                            let value = value.as_u64().unwrap();
                            let node = dom.nodes.get_mut(&node_id).expect(format!("No node with {}", node_id).as_str());
                            if let Some(index) = node.children.iter().position(|x| *x == value) {
                                node.children.remove(index as usize);
                            }
                        }
                        need_rebuild = true;
                    }
                    Update::SetAttr(SetAttr { node_id, attribute, value }) => {
                        let node = dom.nodes.get_mut(&node_id).unwrap();
                        need_rebuild |= node.node_type.set_attr(context, node_id, attribute.as_str(), &value);
                    }
                }
            }
        }
    }
    return (need_rebuild, log_ids)
}

pub struct NoriaClient {
    dom_mutex: Arc<Mutex<Dom>>,
    api: RenderApi,
    pipeline_id: PipelineId,
    document_id: DocumentId,
    content_size: LayoutSize,
    fonts_manager: FontsManager
}

pub struct Controller {
    dom_mutex: Arc<Mutex<Dom>>,
    stream: TcpStream,
    document_id: DocumentId,
    pipeline_id: PipelineId,
    api: RenderApi,
}

impl Clone for Controller {
    fn clone(&self) -> Self {
        Controller {
            dom_mutex: self.dom_mutex.clone(),
            stream: self.stream.try_clone().expect("Can't clone stream"),
            document_id: self.document_id,
            pipeline_id: self.pipeline_id,
            api: self.api.clone_sender().create_api(),
        }
    }
}

fn leak_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

impl Controller {
    pub fn mouse_click(&mut self, cursor_position: WorldPoint) {
        let hit_result = self.api.hit_test(self.document_id, Some(self.pipeline_id), cursor_position, HitTestFlags::empty());
        let dom = self.dom_mutex.lock().unwrap();
        for item in hit_result.items {
            let (node_id, _) = item.tag;
            let node_type = &dom.nodes.get(&node_id).unwrap().node_type;
            node_type.on_click(&mut self.stream, 0, node_id, &item.point_relative_to_item); // TODO LOG_ID!!

        }
    }

    pub fn mouse_wheel(&mut self, cursor_position: WorldPoint, delta: MouseScrollDelta) {
        let log_id = perf::on_get_mouse_wheel();
        profile_scope!(leak_str(format!("Mouse wheel {}", log_id)));
        let hit_result = self.api.hit_test(self.document_id, Some(self.pipeline_id), cursor_position, HitTestFlags::empty());
        const LINE_HEIGHT: f32 = 38.0;
        let delta_vector = match delta {
            glutin::MouseScrollDelta::LineDelta(dx, dy) => LayoutVector2D::new(-dx, -dy * LINE_HEIGHT),
            glutin::MouseScrollDelta::PixelDelta(pos) => LayoutVector2D::new(-pos.x as f32, -pos.y as f32),
        };
        let dom = self.dom_mutex.lock().unwrap();
        for item in hit_result.items {
            let (node_id, _) = item.tag;
            let node_type = &dom.nodes.get(&node_id).unwrap().node_type;
            perf::on_send_mouse_wheel(log_id);
            node_type.on_wheel(&mut self.stream, log_id, node_id, &delta_vector);
        }
    }
}

struct TransactionNotificationHandler(Vec<perf::LogId>);
impl NotificationHandler for TransactionNotificationHandler {
    fn notify(&self, when: Checkpoint) {
        profile_scope!(leak_str(format!("Transaction Notify {:?} {:?}", when, self.0)));
    }
}

impl NoriaClient {
    pub fn spawn<A: ToSocketAddrs>(addr: A, sender: RenderApiSender, pipeline_id: PipelineId, document_id: DocumentId, content_size: LayoutSize) -> Controller {
        let api = sender.create_api();
        let fonts_manager = text::FontsManager::new(sender.create_api(), document_id);

        let dom_mutex = Arc::new(Mutex::new(Dom::default()));

        let mut updater = NoriaClient {
            dom_mutex: dom_mutex.clone(),
            api,
            pipeline_id,
            document_id,
            content_size,
            fonts_manager
        };
        let mut stream = TcpStream::connect(addr).expect("No server here!");
        stream.set_nodelay(true).unwrap();
        stream.write("{kind : \"webrender\"}".as_bytes()).unwrap();

        let mut read_stream = stream.try_clone().unwrap();
        std::thread::spawn(move || {
            register_thread_with_profiler("Noria updater".to_owned());
            let mut epoch = Epoch(0);
            loop {
                let msg = read_msg(&mut read_stream);
                if let Some(msg) = msg {
                    let mut dom = updater.dom_mutex.lock().unwrap();
                    let mut txn = Transaction::new();
                    let mut context = ApplyUpdatesContext {
                        pipeline_id: pipeline_id,
                        fonts_manager: &mut updater.fonts_manager,
                        txn: &mut txn
                    };
                    let (rebuild_display_list, log_ids) = apply_updates(&mut dom, &mut context, &msg);
                    profile_scope!(leak_str(format!("Send TX {:?}", log_ids)));
                    if rebuild_display_list {
                        profile_scope!("rebuild DL");
                        let builder = updater.build_display_list(&mut dom);
                        txn.set_display_list(
                            epoch,
                            Some(ColorF::BLACK),
                            updater.content_size,
                            builder.finalize(),
                            true,
                        );

                    } else {
                        txn.skip_scene_builder();
                    }
                    txn.update_epoch(updater.pipeline_id, epoch);
                    epoch.0 += 1;
                    txn.generate_frame();
                    perf::on_send_transaction(&log_ids);
                    txn.notify(NotificationRequest::new(Checkpoint::FrameRendered, Box::new(TransactionNotificationHandler(log_ids))));
                    updater.api.send_transaction(updater.document_id, txn);
                } else {
                    break;
                }
            }
        });
        Controller {
            dom_mutex: dom_mutex,
            stream: stream,
            document_id: document_id,
            pipeline_id: pipeline_id,
            api: sender.create_api(),
        }
    }

    fn build_display_list(&self, dom: &Dom) -> DisplayListBuilder {
        let mut visitor_context = VisitorContext {
            nodes: &dom.nodes,
            builder: DisplayListBuilder::new(self.pipeline_id, self.content_size),
            space_and_clip_stack: Vec::new(),
            api: &self.api,
            fonts_manager: &self.fonts_manager,
        };
        if let Some(root_node_id) = dom.root_node {
            dom.nodes.get(&root_node_id).unwrap().visit(&mut visitor_context);
        }
        visitor_context.builder
    }
}