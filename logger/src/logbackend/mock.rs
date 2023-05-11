use crate::error::GtsLoggerError;
use crate::logbackend::LogBackend;
use std::cell::RefCell;
use std::collections::VecDeque;

pub struct MockLogBacked<T: Copy> {
    queue: RefCell<VecDeque<T>>,
}

impl<T: Copy> MockLogBacked<T> {
    pub fn new() -> Self {
        MockLogBacked {
            queue: RefCell::new(VecDeque::new()),
        }
    }

    pub fn pop_front(&self) -> Option<T> {
        self.queue.borrow_mut().pop_front()
    }
}

impl<T: Copy> LogBackend<T> for MockLogBacked<T> {
    fn log(&self, event: T) -> Result<(), GtsLoggerError> {
        self.queue.borrow_mut().push_back(event);
        Ok(())
    }
}
