use crossbeam::queue::SegQueue;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum LogMessage {
    Raw(String),
    GetMouseWheel,
    SendMouseWheel{ log_id: u64 },
    GetNoriaMessage,
    SendTransaction { log_ids: Vec<u64> },
    NewFrameReady { log_ids: Vec<u64> },
    NewFrameDone,
}

struct LogState {
    messages: SegQueue<(Duration, LogMessage)>,
    log_ids: SegQueue<Vec<u64>>,
    instant: Instant,
}

static mut PERF_LOG: Option<LogState> = None;

pub fn init() {
    unsafe {
        PERF_LOG = Some(LogState { messages: SegQueue::new(), log_ids: SegQueue::new(), instant: Instant::now(), });
    }
}

fn get_state() -> &'static LogState {
    unsafe {
        return PERF_LOG.as_ref().unwrap()
    }
}

pub fn log(msg: LogMessage) {
    let perf_state = get_state();
    perf_state.messages.push((perf_state.instant.elapsed(), msg));
}

pub fn print() {
    loop {
        if let Ok((duration, msg)) = get_state().messages.pop() {
            println!("{} {:?}", duration.as_nanos(), msg);
        } else {
            break;
        }
    }
}

pub fn push_log_ids(log_ids: Vec<u64>) {
    get_state().log_ids.push(log_ids);
}

pub fn pop_log_ids() -> Vec<u64> {
    get_state().log_ids.pop().unwrap_or_else(|_| Vec::new())
}