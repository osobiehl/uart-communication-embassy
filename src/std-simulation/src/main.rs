#![feature(type_alias_impl_trait)]
#![feature(future_join)]
#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]
use communication::{AsyncDevice, AsyncTimer};
use communication::{Read, ReadError, Write, WriteError};
use embassy_net::udp::UdpSocket;
use embassy_net::{Ipv4Address, PacketMetadata, Stack};
use log::info;
use std::future;
use std::pin::Pin;
use tokio::sync::broadcast;
use tokio::time::{self, sleep, sleep_until, Duration, Instant};
pub struct PanicAsyncTimer(Option<Pin<Box<time::Sleep>>>);

impl PanicAsyncTimer {
    pub fn new() -> Self {
        Self(None)
    }
}

impl AsyncTimer for PanicAsyncTimer {
    type AsyncOutput<'a> = &'a mut Pin<Box<time::Sleep>>;
    fn duration<'a>(&'a mut self, duration: Duration) -> Option<Self::AsyncOutput<'a>> {
        self.0 = Some(Box::pin(time::sleep(duration)));
        return self.0.as_mut();
    }
    fn get_handle<'a>(&'a mut self) -> Option<Self::AsyncOutput<'a>> {
        return self.0.as_mut();
    }
}
pub struct InternalBusWriter(broadcast::Sender<Vec<u8>>);

impl InternalBusWriter {
    pub fn new(s: broadcast::Sender<Vec<u8>>) -> Self {
        InternalBusWriter(s)
    }
}

impl Write for InternalBusWriter {
    fn is_line_free(&self) -> bool {
        return true;
    }
    async fn write<'a>(&'a mut self, buf: &'a [u8]) -> Result<(), WriteError>
    where
        Self: Sized,
    {
        let to_send: Vec<u8> = buf.into();
        match self.0.send(to_send) {
            Ok(_) => Ok(()),
            Err(broadcast::error::SendError(x)) => {
                info!("could not broadcast to IB simulation ... is there any other device set up?");
                Ok(())
            }
        }
    }
}

pub struct InternalBusReader(broadcast::Receiver<Vec<u8>>);

impl InternalBusReader {
    pub fn new(b: broadcast::Receiver<Vec<u8>>) -> Self {
        InternalBusReader(b)
    }
}

impl Read for InternalBusReader {
    async fn read_until_idle<'a>(&'a mut self, buf: &'a mut [u8]) -> Result<usize, ReadError>
    where
        Self: Sized,
    {
        let Ok(read) =  self.0.recv().await else {
                return Err(ReadError::FramingError);
               };
        let recv_len = read.len();

        if recv_len > buf.len() {
            return Err(ReadError::OverflowError);
        }

        for i in 0..recv_len {
            buf[i] = read[i];
        }
        return Ok(recv_len);
    }
}

const INTERNAL_BUS_MAX_DEVICES: usize = 10;

#[tokio::main]
async fn main() -> Result<(), ()> {
    // Open a connection to the mini-redis address.
    println!("hello, world!");

    Ok(())
}
