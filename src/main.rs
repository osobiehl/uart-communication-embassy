#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(future_join)]
#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

mod async_timer;
mod communication;
mod init;
mod locator;
mod service;
mod uart_ip;

use core::str;
use embassy_net_driver::Driver;
use locator::locator::Locator;
mod backoff_handler;
use async_timer::timer::AsyncBasicTimer;
use communication::serial;
use communication::serial::{Read, Write};
use core::fmt::Write as Writefmt;
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::udp::UdpSocket;
use embassy_net::{ConfigStrategy, Ipv4Address, Ipv4Cidr, PacketMetadata, Stack, StackResources};
use rand_core::RngCore;

use embassy_net_driver_channel::{self, Device};
use embassy_stm32::interrupt::TIM6 as TIM6I;
use embassy_stm32::peripherals::{DMA2_CH1, DMA2_CH2, DMA2_CH3, DMA2_CH4, TIM6, USART2, USART3};

use embassy_stm32::usart::{UartRx, UartTx};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::{Channel, Receiver, Sender},
    signal::Signal,
};
use embassy_time::{Duration, Timer};
use heapless::String;
use heapless::Vec;

use static_cell::StaticCell;
use uart_ip::{AsyncDevice, CommunicationState, IP_FRAME_SIZE};

use crate::service::service::CoreServiceLocator;
use crate::uart_ip::AsyncHalfDuplexUart;
use {defmt_rtt as _, panic_probe as _};

type Frame = String<128>;
const NUM_FRAMES: usize = 16;
type ReadySignal = Signal<CriticalSectionRawMutex, ()>;

// type Channel = channel::Channel<blocking_mutex::NoopMutex<Frame>, Frame, NUM_FRAMES>;
macro_rules! singleton {
    ($val:expr) => {{
        type T = impl Sized;
        static STATIC_CELL: StaticCell<T> = StaticCell::new();
        STATIC_CELL.init_with(move || $val)
    }};
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut locator = init::init::initialize();
    info!("starting...");
    let (stack_one, driver_one) = locator
        .comm_stack_one()
        .expect("could not initialize first comm stack");

    let (stack_two, driver_two) = locator
        .comm_stack_two()
        .expect("could not start second comm stack");

    unwrap!(spawner.spawn(hello_world_task(stack_one)));

    unwrap!(spawner.spawn(net_task_one(stack_one)));
    unwrap!(spawner.spawn(driver_task_one(driver_one)));

    unwrap!(spawner.spawn(net_task_two(stack_two)));
    unwrap!(spawner.spawn(driver_task_two(driver_two)));
}

type NetDriverOne = Stack<impl Driver>;
#[embassy_executor::task]
async fn net_task_one(stack: &'static NetDriverOne) {
    stack.run().await;
}

type DeviceDriverOne = impl AsyncDevice;

#[embassy_executor::task]
async fn driver_task_one(mut task: DeviceDriverOne) {
    task.start().await;
}

type NetDriverTwo = Stack<impl Driver>;
#[embassy_executor::task]
async fn net_task_two(stack: &'static NetDriverTwo) {
    stack.run().await;
}

type DeviceDriverTwo = impl AsyncDevice;

#[embassy_executor::task]
async fn driver_task_two(mut task: DeviceDriverTwo) {
    task.start().await;
}

type DriverStackHelloWorld = Stack<impl Driver>;

#[embassy_executor::task]
async fn hello_world_task(stack: &'static DriverStackHelloWorld) {
    let mut rx_meta = [PacketMetadata::EMPTY; 16];
    let mut rx_buffer = [0; 1096];
    let mut tx_meta = [PacketMetadata::EMPTY; 16];
    let mut tx_buffer = [0; 1096];
    let mut buf: [u8; 1096] = [0; 1096];

    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );
    socket.bind(9400).unwrap();

    loop {
        let mut to_write: String<128> = String::new();

        let written = core::write!(&mut to_write, "hello, world!");
        let r = socket
            .send_to(to_write.as_bytes(), (Ipv4Address::BROADCAST, 9600))
            .await;
        if r.is_err() {
            info!("error: {}", r);
        }
        Timer::after(Duration::from_millis(1000)).await;
        // if let Ok(s) = core::str::from_utf8(&buf[..n]) {
        //     info!("ECHO (to {}): {}", ep, s);
        // } else {
        //     info!("ECHO (to {}): bytearray len {}", ep, n);
        // }
        // socket.send_to(&buf[..n], ep).await.unwrap();
    }
}

/**
 * serial::Read is just a wrapper around uart for now
 */
async fn read_subroutine<R: serial::Read>(
    mut usart: R,
    sender: Sender<'static, NoopRawMutex, Frame, NUM_FRAMES>,
    signal: &'static ReadySignal,
) {
    let mut buf = [0; 1024];
    loop {
        signal.signal(());
        let bytes_read = usart.read_until_idle(&mut buf).await.unwrap();
        info!("read: {}", bytes_read);
        let x = unsafe { str::from_utf8_unchecked(&buf[..bytes_read]) };
        info!("received: {=[u8]:a}", &buf[..bytes_read]);
        let string: Frame = heapless::String::from(x);
        unwrap!(sender.try_send(string));
    }
}
