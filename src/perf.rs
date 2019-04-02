use crossbeam::queue::{SegQueue, ArrayQueue};
use crossbeam::atomic::AtomicCell;
use std::time::{Duration, Instant};
use std::fmt::{Display, Formatter, Error};
use core::fmt::Write;
use core::borrow::Borrow;

type LogId = u64;

//struct PerfMarker {
//
//}
//
//struct PerfMonitor {
//
//}
//
//impl PerfMonitor {
//}

#[derive(Debug)]
pub enum LogMessage {
    Raw(String),
    GetMouseWheel,
    SendMouseWheel{ log_id: LogId },
    GetNoriaMessage,
    SendTransaction { log_ids: Vec<LogId> },
    NewFrameReady { log_ids: Vec<LogId> },
    NewFrameDone,
    FrameTime { t: Duration },
    SwapTime { t: Duration }
}

struct SendedMouseWheelMoment {
    frame_start: Instant,
    log_id: LogId,
    send_instant: Instant,
}

struct LogState {
    messages: SegQueue<LogMessage>,
//    frames: SegQueue<FrameCounters>,
    log_ids: ArrayQueue<Vec<LogId>>,
    frame_start_moments: ArrayQueue<(LogId, Instant)>,
}

static mut PERF_LOG: Option<LogState> = None;

pub fn init() {
    unsafe {
        PERF_LOG = Some(LogState {
            messages: SegQueue::new(),
//            frames: SegQueue::new(),
            log_ids: ArrayQueue::new(100),
            frame_start_moments: ArrayQueue::new(100),
        });
    }
}

fn get_state() -> &'static LogState {
    unsafe {
        return PERF_LOG.as_ref().unwrap()
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
                LogMessage::FrameTime { t } => {
                    if t.as_millis() > 10 {
                        println!("Frame took: {}", t.as_millis());
                    }
                }
                LogMessage::SwapTime { t } => {
                    println!("Swap took : {}", t.as_millis());
                }
                _ => ()
            }
        } else {
            break;
        }
    }
}

pub fn push_log_ids(log_ids: Vec<LogId>) {
    get_state().log_ids.push(log_ids);
}

pub fn pop_log_ids() -> Vec<u64> {
    get_state().log_ids.pop().unwrap_or_else(|_| Vec::new())
}


// markers

pub fn on_get_mouse_wheel(log_id: LogId) {
    let state = get_state();
    state.frame_start_moments.push((log_id, Instant::now()));
//    log(LogMessage::GetMouseWheel)
}

pub fn on_send_mouse_wheel(log_id: LogId) {
//    let state = get_state();
//    if let Ok(frame_start_inst) = state.mouse_wheel_moments.pop() {
//        state.send_mouse_wheel_moments.push();
//    }
//    log(LogMessage::SendMouseWheel { log_id });
}

pub fn on_get_noria_message() {
//    log(LogMessage::GetNoriaMessage);
}

pub fn on_send_transaction(log_ids: Vec<LogId>) {
    push_log_ids(log_ids);
//    log(LogMessage::SendTransaction { log_ids });
}

pub fn on_new_frame_ready() {
    let state = get_state();
    let log_ids = pop_log_ids();
    let mut idx = 0;
    loop {
        if idx < log_ids.len() {
            let log_id = log_ids[idx];
            let (queued_log_id, frame_start) = state.frame_start_moments.pop().expect("No queued updates!");
            if queued_log_id < log_id {
                continue;
            }
            assert_eq!(log_id, queued_log_id);
            log(LogMessage::FrameTime { t: frame_start.elapsed() });
            idx += 1;
        } else {
            break;
        }
    }

//    for log_id in log_ids.clone() {
//        let (queued_log_id, frame_start) = state.frame_start_moments.pop().expect("No queued updates!");
//        assert_eq!(log_id, queued_log_id);
//        println!("log_ids: {:?} queued: {}", log_ids, queued_log_id);
//        log(LogMessage::FrameTime { t: frame_start.elapsed() });
//    }
//    log(LogMessage::NewFrameReady { log_ids: pop_log_ids() })
}

pub fn on_new_frame_done() {
    log(LogMessage::NewFrameDone);
}
