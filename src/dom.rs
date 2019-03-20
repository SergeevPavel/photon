use std::thread;

use std::io::prelude::*;
use std::net::TcpStream;

use byteorder::{ReadBytesExt, BigEndian};
use serde_json::Value;
use webrender::api::*;
use std::collections::HashMap;

use crate::text;
use euclid::TypedSize2D;

fn read_msg(stream: &mut TcpStream) -> Vec<u8> {
    let size = stream.read_u32::<BigEndian>().unwrap();
    let mut buf = vec![0u8; size as usize];
    stream.read_exact(&mut buf);
    buf
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
    Div { color: ColorF, rect: LayoutRect, on_click: Callback },
    Text { text: String, origin: LayoutPoint },
    Scroll { position: LayoutRect,
             content: LayoutRect,
             on_wheel: Callback },
    Unknown
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

impl NodeType {
    fn create(node_type: &str) -> NodeType {
        // TODO support constructor params
        match node_type {
            "root" => {
                NodeType::Root
            }
            "text" => {
                NodeType::Text {
                    text: "".to_string(),
                    origin: LayoutPoint::new(0.0, 0.0)
                }
            }
            "div" => {
                NodeType::Div {
                    color: ColorF::BLACK,
                    rect: LayoutRect::new(LayoutPoint::zero(),
                                          LayoutSize::new(0.0, 0.0)),
                    on_click: Callback::None
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

    fn set_attr(&mut self, attribute: &str, value: &Value) {
        match self {
            NodeType::Root => {

            }
            NodeType::Div { ref mut color, rect } => {
                match attribute {
                    "color" => {
                        *color = ColorF::WHITE; // parse color
                    }
                    "rect" => {
                        *rect = parse_rect(value);
                    }

                    _ => {}
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
                        *on_wheel = match value.as_str().unwrap() {
                            "noria-handler-sync" => { Callback::Sync }
                            "noria-handler-async" => { Callback::Async }
                            "-noria-handler" => { Callback::None }
                            _ => unreachable!()
                        }
                    }

                    _ => {}
                }
            }
            NodeType::Text { ref mut text, origin } => {
                match attribute {
                    "text" => {
                        *text = value.as_str().unwrap().to_string();
                    }
                    "origin" => {
                        *origin = parse_point(value);
                    }
                    _ => {}
                }
            }
            _ => {}
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
            NodeType::Div { color, rect, on_click } => {
                let mut info = LayoutPrimitiveInfo::new(*rect);
                let space_and_clip = context.space_and_clip_stack.last().unwrap();
                let widths = LayoutSideOffsets::new(1.0, 1.0, 1.0, 1.0);
                let border_details = BorderDetails::Normal(NormalBorder {
                    left: BorderSide {
                        color: ColorF::BLACK,
                        style: BorderStyle::Solid
                    },
                    right: BorderSide {
                        color: ColorF::BLACK,
                        style: BorderStyle::Solid
                    },
                    top: BorderSide {
                        color: ColorF::BLACK,
                        style: BorderStyle::Solid
                    },
                    bottom: BorderSide {
                        color: ColorF::BLACK,
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
//                context.builder.push_rect(&info, &space_and_clip, ColorF::BLACK);
                if on_click.is_some() {
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
            NodeType::Text { text, origin } => {
                if let Some(parent_space_and_clip) = context.space_and_clip_stack.last() {
                    text::show_text(context.api,
                                    context.default_font_key,
                                    context.default_font_size,
                                    context.default_font_instance_key,
                                    &mut context.builder,
                                    &parent_space_and_clip,
                                    text,
                                    origin.clone());
                } else {
                    unreachable!("No parent space and clip");
                }
            }
            _ => {}
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

            _ => {}
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
    default_font_key: FontKey,
    default_font_instance_key: FontInstanceKey,
    default_font_size: i32,
    api: &'a RenderApi,
    nodes: &'a HashMap<NodeId, Node>
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

#[derive(Debug)]
struct Dom {
    nodes: HashMap<NodeId, Node>,
    root_node: Option<NodeId>,
}

fn apply_updates(dom: &mut Dom, message: Vec<u8>) {
    if let Ok(Value::Array(message)) = serde_json::from_slice(&message) {
        let _log_ids = message.first();
        for update in message.iter().skip(1) {
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
                    let index = update["index"].as_u64().unwrap();
                    if attribute == "children" {
                        let node = dom.nodes.get_mut(&node_id).unwrap();
                        node.children.remove(index as usize);
                    }
                }
                "set-attr" => {
                    let node_id = update["node"].as_u64().unwrap();
                    let attribute = update["attr"].as_str().unwrap();
                    let node = dom.nodes.get_mut(&node_id).unwrap();
                    node.node_type.set_attr(attribute, &update["value"]);
                }
                _ => {}
            }
            println!("{:?}", update);
        }
    }
}

pub struct Updater {
    dom: Dom,
    api: RenderApi,
    pipeline_id: PipelineId,
    document_id: DocumentId,
    content_size: LayoutSize,
    default_font_size: i32,
    default_font_key: FontKey,
    default_font_instance_key: FontInstanceKey,
}

impl Updater {
    pub fn spawn(api: RenderApi, pipeline_id: PipelineId, document_id: DocumentId, content_size: LayoutSize) {
        let default_font_size = 16;
        let (default_font_key, default_font_instance_key) = text::init_font(&api, pipeline_id, document_id, default_font_size);
        let mut updater = Updater {
            dom: Dom {
                nodes: HashMap::new(),
                root_node: None,
            },
            api,
            pipeline_id,
            document_id,
            content_size,
            default_font_size,
            default_font_key,
            default_font_instance_key
        };

        std::thread::spawn(move || {
            let mut epoch = Epoch(0);
            let mut stream = TcpStream::connect("127.0.0.1:61567").unwrap();
            stream.set_nodelay(true);
            stream.write("{kind : \"webrender\"}".as_bytes());

            loop {
                let msg = read_msg(&mut stream);
                apply_updates(&mut updater.dom, msg);
                updater.update(epoch);
                epoch.0 += 1;
            }
        });
    }

    fn update(&self, epoch: Epoch) {
        if let Some(root_node_id) = self.dom.root_node {
            let mut txn = Transaction::new();
            let mut builder = DisplayListBuilder::new(self.pipeline_id, self.content_size);
            let mut visitor_context = VisitorContext {
                nodes: &self.dom.nodes,
                builder: builder,
                space_and_clip_stack: Vec::new(),
                default_font_key: self.default_font_key,
                default_font_size: self.default_font_size,
                default_font_instance_key: self.default_font_instance_key,
                api: &self.api,
            };

            self.dom.nodes.get(&root_node_id).unwrap().visit(&mut visitor_context);

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