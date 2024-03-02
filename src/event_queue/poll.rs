use std::{io::{self, Result}, net::TcpStream, os::fd::AsRawFd};
use crate::event_queue::ffi;

type Events = Vec<ffi::Event>;

pub struct Poll {
    registry: Registry,
}

impl Poll {
    pub fn new() -> Result<Self> {
        todo!()
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn poll(&mut self, events: &mut Events, timeout: Option<i32>) -> Result<()> {
        todo!()
    }
}

pub struct Registry {
    raw_fd: i32,
}

impl Registry {
    // NOTE that we only allow to register TcpStream. To generalize this we would need to make
    // generic Trait for other types to implement.
    pub fn register(&self, source: &TcpStream, token: usize, intersets: i32) -> Result<()> {
        todo!()
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        todo!()
    }
}
