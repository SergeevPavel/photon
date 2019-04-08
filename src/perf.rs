use crossbeam::queue::{SegQueue, ArrayQueue};
use crossbeam::atomic::AtomicCell;
use std::time::{Duration, Instant};
use fxhash::FxHashMap;

pub type LogId = u64;

static mut PERF_LOG: Option<PerfLog> = None;

pub enum LogMessage {
    Raw(String),
    FrameMetrics(FrameMetrics, Vec<LogId>)
}

struct FrameMetrics {
    frame_start: Instant,
    after_hit_test: Option<Instant>,
    frame_ready: Option<Instant>,
    frame_rendered: Option<Instant>,
    frame_done: Option<Instant>,
    background_thread_metrics: Option<BackgroundThreadMetrics>
}

struct PerfLog {
    // Mutated by Main thread
    next_log_id: AtomicCell<LogId>,
    frames: FxHashMap<LogId, FrameMetrics>,
    new_frame_ready: Option<Instant>,
    frame_rendered: Option<Instant>,
    messages: SegQueue<LogMessage>,

    // Forward msgs from Background to Main thread
    send_to_wr: ArrayQueue<BackgroundThreadMetrics>,

    // Mutated by background thread
    background_thread_metrics: BackgroundThreadMetrics,
}

#[derive(Clone)]
struct BackgroundThreadMetrics {
    log_ids: Option<Vec<LogId>>,
    get_noria_message: Option<Instant>,
    send_transaction: Option<Instant>,
    get_wake_up: Option<Instant>,
    cpu_backend_time: Option<Duration>
}

pub fn init() {
    unsafe {
        PERF_LOG = Some(PerfLog {
            next_log_id: AtomicCell::new(0),
            frames: Default::default(),
            new_frame_ready: None,
            frame_rendered: None,
            messages: SegQueue::new(),
            send_to_wr: ArrayQueue::new(100000),
            background_thread_metrics: BackgroundThreadMetrics {
                log_ids: None,
                get_noria_message: None,
                send_transaction: None,
                get_wake_up: None,
                cpu_backend_time: None
            }
        });
    }
}

fn get_state() -> &'static mut PerfLog {
    unsafe {
        return PERF_LOG.as_mut().unwrap()
    }
}

pub fn log(msg: LogMessage) {
    let perf_state = get_state();
    perf_state.messages.push(msg);
}

pub fn print() {
    loop {
        if let Ok(msg) = get_state().messages.pop() {
            match msg {
                LogMessage::FrameMetrics(metrics, log_ids) => {
                    let total = metrics.frame_done.unwrap() - metrics.frame_start;
                    if total.as_millis() > 0 {
                        let hit_test_time = metrics.after_hit_test.unwrap() - metrics.frame_start;
                        let background_metrics = metrics.background_thread_metrics.unwrap();
                        let noria_time = background_metrics.get_noria_message.unwrap()- metrics.after_hit_test.unwrap();
                        let prepare_transaction_time = background_metrics.send_transaction.unwrap() - background_metrics.get_noria_message.unwrap();
                        let build_frame_time = metrics.frame_ready.unwrap() - background_metrics.send_transaction.unwrap();
                        let render_time = metrics.frame_rendered.unwrap() - metrics.frame_ready.unwrap();
                        let swap_time = metrics.frame_done.unwrap() - metrics.frame_rendered.unwrap();

                        let wr_total = hit_test_time + prepare_transaction_time + build_frame_time + render_time;
                        println!("total: {:>10} \
                                  log_ids: {:>10} \
                                  wr total: {:>10} \
                                  noria: {:>10} \
                                  hit_test: {:>10} \
                                  prepare transaction: {:>10} \
                                  build_frame: {:>10} \
                                  render frame: {:>10} \
                                  swap: {:>10} \
                                  cpu backend: {:>10}",
                                 format!("{:.2?}", total),
                                 format!("{:?}", log_ids),
                                 format!("{:.2?}", wr_total),
                                 format!("{:.2?}", noria_time),
                                 format!("{:.2?}", hit_test_time),
                                 format!("{:.2?}", prepare_transaction_time),
                                 format!("{:.2?}", build_frame_time),
                                 format!("{:.2?}", render_time),
                                 format!("{:.2?}", swap_time),
                                 format!("{:.2?}", background_metrics.cpu_backend_time.unwrap()));
                    }
                }
                _ => ()
            }
        } else {
            break;
        }
    }
}

// markers

pub fn on_get_mouse_wheel() -> LogId {
    let mut state = get_state();
    let log_id = state.next_log_id.fetch_add(1);
//    state.frames.insert(log_id, FrameMetrics {
//        frame_start: Instant::now(),
//        after_hit_test: None,
//        frame_ready: None,
//        frame_rendered: None,
//        frame_done: None,
//        background_thread_metrics: None
//    });
    log_id
}

pub fn on_send_mouse_wheel(log_id: LogId) {
//    let state = get_state();
//    let frame = state.frames.get_mut(&log_id).unwrap();
//     frame.after_hit_test = Some(Instant::now());

}

pub fn on_get_noria_message(log_ids: Vec<LogId>) {
//    let mut state = get_state();
//    state.background_thread_metrics.log_ids = Some(log_ids);
//    state.background_thread_metrics.get_noria_message = Some(Instant::now());
}

pub fn on_send_transaction(log_ids: &Vec<LogId>) {
//    let mut state = get_state();
//    state.background_thread_metrics.send_transaction = Some(Instant::now());
//    state.send_to_wr.push(state.background_thread_metrics.clone()).unwrap();
}

pub fn on_wake_up(d: Duration) {
//    let mut state = get_state();
//    state.background_thread_metrics.get_wake_up = Some(Instant::now());
//    state.background_thread_metrics.cpu_backend_time = Some(d);
}

pub fn on_new_frame_ready() {
//    let state = get_state();
//    state.new_frame_ready = Some(Instant::now());
}

pub fn on_frame_rendered() {
    get_state().frame_rendered = Some(Instant::now());
}

pub fn on_new_frame_done() {
//    let state = get_state();
//    if let Ok(background_metrics) = state.send_to_wr.pop() {
//        for log_id in background_metrics.log_ids.clone().unwrap() {
//            let mut frame = state.frames.remove(&log_id).unwrap();
//            frame.background_thread_metrics = Some(background_metrics.clone());
//            frame.frame_ready = state.new_frame_ready.clone();
//            frame.frame_rendered = state.frame_rendered.clone();
//            frame.frame_done = Some(Instant::now());
//            state.messages.push(LogMessage::FrameMetrics(frame, background_metrics.log_ids.clone().unwrap()));
//        }
//    }
}
