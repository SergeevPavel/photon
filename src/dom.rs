
extern crate fxhash;
use fxhash::FxHashMap;


use std::thread;

use std::io::prelude::*;
use std::net::{TcpStream, ToSocketAddrs, Shutdown};
use std::time::{SystemTime, UNIX_EPOCH, Instant};

use byteorder::{ReadBytesExt, BigEndian};
use serde_json::Value;
use webrender::api::*;

use crate::text;

use euclid::TypedSize2D;
use std::sync::{Mutex, Arc};
use serde::Serialize;
use crate::text::FontsManager;

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
    Text { text: String, origin: LayoutPoint, layouted_text: Option<text::LayoutedText> },
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
                    layouted_text: None

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

    fn set_attr(&mut self, context: &ApplyUpdatesContext, attribute: &str, value: &Value) {
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

                    "on-wheel" => {
                        *on_wheel = parse_callback(value);
                    }

                    _ => ()
                }
            }
            NodeType::Text { ref mut text, origin, layouted_text } => {
                match attribute {
                    "text" => {
                        *text = value.as_str().unwrap().to_string();
                        *layouted_text = Some(context.fonts_manager.layout_simple_ascii(text,
                                                                                        origin.clone(),
                                                                                        FontInstanceFlags::default()));
                    }
                    "origin" => {
                        *origin = parse_point(value);
                        *layouted_text = Some(context.fonts_manager.layout_simple_ascii(text,
                                                                                        origin.clone(),
                                                                                        FontInstanceFlags::default()));
                    }
                    _ => ()
                }
            }
        }
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
                                                                                webrender::api::ScrollSensitivity::ScriptAndInputEvents);
                context.space_and_clip_stack.push(scroll_space_and_clip);
                let mut info = LayoutPrimitiveInfo::new(*content);
                if on_wheel.is_some() {
                    info.tag = Some((node_id, 0));
                }
                context.builder.push_rect(&info,
                                          &scroll_space_and_clip,
                                          ColorF::TRANSPARENT);
            }
            NodeType::Text { text, origin, layouted_text } => {
                if let Some(parent_space_and_clip) = context.space_and_clip_stack.last() {
                    if let Some(layouted_text) = layouted_text {
                        let info = LayoutPrimitiveInfo::new(layouted_text.bounding_rect);
                        context.builder.push_text(&info,
                                                  &parent_space_and_clip,
                                                  layouted_text.glyphs.as_slice(),
                                                  context.fonts_manager.font_instance_key,
                                                  ColorF::BLACK,
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

            _ => ()
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
                    stream.write(serde_json::to_string(&msg).unwrap().as_bytes());
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
                    stream.write(serde_json::to_string(&msg).unwrap().as_bytes());
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
                    stream.write(serde_json::to_string(&msg).unwrap().as_bytes());
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
    fonts_manager: &'a FontsManager
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

fn apply_updates(dom: &mut Dom, context: &ApplyUpdatesContext, message: Vec<u8>) {
    if let Ok(Value::Array(message)) = serde_json::from_slice(&message) {
        let _log_ids = message.first();
        for update in message.iter().skip(1) {
//            println!("{:?}", update);
            let update_type = update["update-type"].as_str().unwrap();
            match update_type {
                "make-node" => {
                    let node_id = update["node"].as_u64().unwrap();
                    let node_type = NodeType::create(update["type"].as_str().unwrap());
                    if let NodeType::Root = node_type {
                        dom.root_node = Some(node_id);
                    }
                    let mut node = Node {
                        id: node_id,
                        node_type: node_type,
                        children: Vec::new(),
                    };
                    dom.nodes.insert(node_id, node);
                }
                "destroy" => {
                    let node_id = update["node"].as_u64().unwrap();
                    assert!(dom.nodes.remove(&node_id).is_some());
                }
                "add" => {
                    let node_id = update["node"].as_u64().unwrap();
                    let attribute = update["attr"].as_str().unwrap();
                    let index = update["index"].as_u64().unwrap();
                    if attribute == "children" {
                        let value = update["value"].as_u64().unwrap();
                        assert!(dom.nodes.contains_key(&value));
                        let node = dom.nodes.get_mut(&node_id).unwrap();
                        node.children.insert(index as usize, value);
                    }
                }
                "remove" => {
                    let node_id = update["node"].as_u64().unwrap();
                    let attribute = update["attr"].as_str().unwrap();
                    if attribute == "children" {
                        let value = update["value"].as_u64().unwrap();
                        let node = dom.nodes.get_mut(&node_id).expect(format!("No node with {}", node_id).as_str());
                        if let Some(index) = node.children.iter().position(|x| *x == value) {
                            node.children.remove(index as usize);
                        }
                    }
                }
                "set-attr" => {
                    let node_id = update["node"].as_u64().unwrap();
                    let attribute = update["attr"].as_str().unwrap();
                    let node = dom.nodes.get_mut(&node_id).unwrap();
                    node.node_type.set_attr(context, attribute, &update["value"]);
                }
                _ => ()
            }
        }
    }
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
    log_id: u64,
}

impl Controller {
    pub fn mouse_click(&mut self, hit_result: HitTestResult) {
        for item in hit_result.items {
            let (node_id, _) = item.tag;
            let dom = self.dom_mutex.lock().unwrap();
            let node_type = &dom.nodes.get(&node_id).unwrap().node_type;
            self.log_id += 1;
            node_type.on_click(&mut self.stream, self.log_id, node_id, &item.point_relative_to_item);

        }
    }

    pub fn mouse_wheel(&mut self, hit_result: HitTestResult, delta: LayoutVector2D) {
        for item in hit_result.items {
            let (node_id, _) = item.tag;
            let dom = self.dom_mutex.lock().unwrap();
            let node_type = &dom.nodes.get(&node_id).unwrap().node_type;
            self.log_id += 1;
            node_type.on_wheel(&mut self.stream, self.log_id, node_id, &delta);
        }
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
        stream.write("{kind : \"webrender\"}".as_bytes());

        let mut read_stream = stream.try_clone().unwrap();
        std::thread::spawn(move || {
            let mut epoch = Epoch(0);
            loop {
                let msg = read_msg(&mut read_stream);
                if let Some(msg) = msg {
                    let mut dom = updater.dom_mutex.lock().unwrap();
                    let context = ApplyUpdatesContext {
                        fonts_manager: &updater.fonts_manager
                    };
                    apply_updates(&mut dom, &context, msg);
                    updater.update(&mut dom, epoch);
                    epoch.0 += 1;
                } else {
                    break;
                }
            }
        });
        Controller {
            dom_mutex: dom_mutex,
            stream: stream,
            log_id: 0
        }
    }

    fn update(&self, dom: &Dom, epoch: Epoch) {
        if let Some(root_node_id) = dom.root_node {
            let mut txn = Transaction::new();
            let mut builder = DisplayListBuilder::new(self.pipeline_id, self.content_size);
            let mut visitor_context = VisitorContext {
                nodes: &dom.nodes,
                builder: builder,
                space_and_clip_stack: Vec::new(),
                api: &self.api,
                fonts_manager: &self.fonts_manager,
            };

            dom.nodes.get(&root_node_id).unwrap().visit(&mut visitor_context);

            txn.set_display_list(
                epoch,
                None,
                self.content_size,
                visitor_context.builder.finalize(),
                true,
            );
            txn.set_root_pipeline(self.pipeline_id);
            txn.generate_frame();
            self.api.send_transaction(self.document_id, txn);
        }
    }
}