use arrayvec::ArrayString;
use gts_logger::logbackend::consolelogger::{ConsoleThreadLogBacked, LogEventTs};
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

    let log_client =
        LogClient::<_, LogEventTs<LogEvent>>::new(ConsoleThreadLogBacked::<3000, _>::new());

    info!(">> println with logger:");
    let start = minstant::Instant::now();
    let timestamp = start.as_unix_nanos(&anc);
    log_client.log(LogEventTs::new(timestamp, event)).unwrap();

    let start = minstant::Instant::now();
    let timestamp = start.as_unix_nanos(&anc);
    log_client.log(LogEventTs::new(timestamp, event)).unwrap();

    let start = minstant::Instant::now();
    let timestamp = start.as_unix_nanos(&anc);
    log_client.log(LogEventTs::new(timestamp, event)).unwrap();

    info!(">> regular println");
    let start = minstant::Instant::now();
    let timestamp = start.as_unix_nanos(&anc);
    info!("{:?}", LogEventTs::new(timestamp, event));

    let start = minstant::Instant::now();
    let timestamp = start.as_unix_nanos(&anc);
    info!("{:?}", LogEventTs::new(timestamp, event));

    let start = minstant::Instant::now();
    let timestamp = start.as_unix_nanos(&anc);
    info!("{:?}", LogEventTs::new(timestamp, event));

    std::thread::sleep(Duration::from_millis(2200));
}
