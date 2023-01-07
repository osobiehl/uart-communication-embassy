#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(future_join)]
#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

mod half_duplex;
mod init;
mod locator;
mod stm32_service;
mod stm32_timer;
mod stm32_uart;
mod uart_ip;

use core::str;
use embassy_net_driver::Driver;

mod backoff_handler;

use core::fmt::Write as Writefmt;
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::udp::UdpSocket;
use embassy_net::{Ipv4Address, PacketMetadata, Stack};

use embassy_time::{Duration, Timer};
use heapless::String;

use communication::AsyncDevice;

use communication::CoreServiceLocator;

use {defmt_rtt as _, panic_probe as _};

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
    unwrap!(spawner.spawn(hello_world_response_task(stack_two)));

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

        let Ok(()) = core::write!(&mut to_write, "hello, world!") else {
            info!("failed send!");
            continue
        };
        let r = socket
            .send_to(to_write.as_bytes(), (Ipv4Address::BROADCAST, 9400))
            .await;
        if r.is_err() {
            info!("error: {}", r);
        }
        Timer::after(Duration::from_millis(3000)).await;
    }
}

type DriverStackResponse = Stack<impl Driver>;

#[embassy_executor::task]
async fn hello_world_response_task(stack: &'static DriverStackResponse) {
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
        let Ok((num_read, endpoint)) = socket.recv_from(&mut buf).await else {
            info!("failed read!");
            continue;
        };
        info!(
            "received: {} from {}",
            str::from_utf8(&buf[..num_read]).unwrap(),
            endpoint
        );
    }
}
