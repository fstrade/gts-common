pub mod error;
pub mod logbackend;
pub mod logclient;

#[cfg(test)]
mod tests {
    use crate::logbackend::mock::MockLogBacked;
    use crate::logclient::LogClient;
    use arrayvec::ArrayString;
    use serde::{Deserialize, Serialize};

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

    #[test]
    fn create_logger() {
        let event = LogEvent::LogOneOne(LogOneStruct {
            some_num: 5,
            some_other_num: 7,
            some_string: ArrayString::from("333").unwrap(),
        });

        let copy_event = event;

        let log_client = LogClient::<_, LogEvent>::new(MockLogBacked::new());

        log_client.log(event).unwrap();

        let rr = log_client.backend().pop_front();
        assert!(matches!(rr, Some(ev) if ev == copy_event));

        let rr = log_client.backend().pop_front();
        assert!(rr.is_none());

        log_client.log(event).unwrap();
        let rr = log_client.backend().pop_front();
        assert!(matches!(rr, Some(ev) if ev == copy_event));

        let rr = log_client.backend().pop_front();
        assert!(rr.is_none());
    }
}
