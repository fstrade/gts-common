use arrayvec::ArrayString;
use gts_logger::logbackend::consolelogger::ConsoleThreadLogBacked;
use gts_logger::logclient::LogClient;
use log::info;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct LogOneStruct {
    some_num: u64,
    some_other_num: u64,
    some_string: ArrayString<16>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct LogTwoStruct {
    some_string: ArrayString<16>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
#[serde(tag = "t", content = "c")]
pub enum LogEvent {
    LogOneOne(LogOneStruct),
    LogTwo(LogTwoStruct),
}

fn main() {
    let anc = minstant::Anchor::new();
    env_logger::init();

    let event = LogEvent::LogOneOne(LogOneStruct {
        some_num: 5,
        some_other_num: 7,
        some_string: ArrayString::from("333").unwrap(),
    });

    let _copy_event = event;

    let log_client = LogClient::<_, LogEvent>::new(ConsoleThreadLogBacked::<5000, _>::new());

    info!(">> println with logger:");

    let ulog_iters = 2000;
    let gstart = minstant::Instant::now();
    for _ in 0..ulog_iters {
        log_client.log(event).unwrap();
    }
    let gend = minstant::Instant::now();
    let ulog_end_ts = gend.as_unix_nanos(&anc);
    let ulog_start_ts = gstart.as_unix_nanos(&anc);

    info!(">> println with logger(without timestamp):");

    let ulogs_iters = 2000;
    let gsstart = minstant::Instant::now();
    for _ in 0..ulogs_iters {
        log_client.log_same(event).unwrap();
    }
    let gsend = minstant::Instant::now();
    let ulogs_end_ts = gsend.as_unix_nanos(&anc);
    let ulogs_start_ts = gsstart.as_unix_nanos(&anc);

    info!(">> regular println");
    let gstart = minstant::Instant::now();
    let mut timestamp = gstart.as_unix_nanos(&anc);

    let iters = 2000;
    for _ in 0..iters {
        let nstart = minstant::Instant::now();
        let ntimestamp = nstart.as_unix_nanos(&anc);
        println!("{} {:?}", ntimestamp - timestamp, event);
        timestamp = ntimestamp;
    }
    let gend = minstant::Instant::now();
    let end_ts = gend.as_unix_nanos(&anc);
    let start_ts = gstart.as_unix_nanos(&anc);
    std::thread::sleep(Duration::from_millis(2200));

    info!(
        "CONSOLE total {} in {}ns, {} ps per iter",
        iters,
        end_ts - start_ts,
        (end_ts - start_ts) * 1000 / iters
    );
    info!(
        "ULOG total {} in {}ns, {} ps per iter",
        ulog_iters,
        ulog_end_ts - ulog_start_ts,
        (ulog_end_ts - ulog_start_ts) * 1000 / ulog_iters
    );
    info!(
        "ULOG_same total {} in {}ns, {} ps per iter",
        ulogs_iters,
        ulogs_end_ts - ulogs_start_ts,
        (ulogs_end_ts - ulogs_start_ts) * 1000 / ulogs_iters
    );
}
