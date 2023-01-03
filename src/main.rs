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
mod uart_ip;
use core::future;
use core::str;
mod backoff_handler;
use async_timer::timer::{AsyncBasicTimer, TimerFuture};
use communication::serial;
use core::fmt::Write;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_net::udp::UdpSocket;
use embassy_net::{ConfigStrategy, Ipv4Address, Ipv4Cidr, PacketMetadata, Stack, StackResources};
use embassy_net_driver::Driver;
use embassy_net_driver_channel::{self, Device, State};
use embassy_stm32::interrupt::{TIM6 as TIM6I, TIM7 as TIM7I};
use embassy_stm32::peripherals::{DMA2_CH1, DMA2_CH2, DMA2_CH3, DMA2_CH4, TIM6, USART2, USART3};
use embassy_stm32::rng::Rng;
use embassy_stm32::usart::{UartRx, UartTx};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::{Channel, Receiver, Sender},
    signal::Signal,
};
use embassy_time::{Duration, Instant, Timer};
use heapless::String;
use heapless::Vec;
use rand_core::RngCore;
use static_cell::StaticCell;
use uart_ip::{AsyncHalfDuplexUart, CommunicationState, IP_FRAME_SIZE};
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
    let state = singleton!(CommunicationState::new());
    let address: [u8; 6] = [0, 2, 3, 4, 5, 6];
    let (runner, device) = embassy_net_driver_channel::new(state, address);
    let usart3 = locator.usart3.take().expect("taking usart3 failed!");
    let (usart3_tx, usart3_rx) = usart3.split();
    let tim6 = locator.tim6.unwrap();
    let mut rng = locator.rng.take().expect("taking rng peripheral failed!");
    let mut seed = [0; 8];
    rng.async_fill_bytes(&mut seed).await;
    let seed = u64::from_le_bytes(seed);

    let uart_driver = AsyncHalfDuplexUart::new(usart3_rx, usart3_tx, tim6, runner, rng);
    let config = ConfigStrategy::Static(embassy_net::Config {
        address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 69, 2), 24),
        dns_servers: Vec::new(),
        gateway: Some(Ipv4Address::new(192, 168, 69, 1)),
    });

    let stack = singleton!(Stack::new(
        device,
        config,
        singleton!(StackResources::<1, 2, 8>::new()),
        seed
    ));
    unwrap!(spawner.spawn(uart3_hello_world(stack)));
    unwrap!(spawner.spawn(net_task_1(stack)));
    unwrap!(spawner.spawn(uart3_driver_task(uart_driver)));

    let usart2 = locator.usart2.take().expect("taking usart2 failed!");

    let (usart2_tx, usart2_rx) = usart2.split();

    static USART2_CHANNEL: StaticCell<Channel<NoopRawMutex, Frame, NUM_FRAMES>> = StaticCell::new();
    let usart2_channel = USART2_CHANNEL.init(Channel::new());

    let usart2_sender = usart2_channel.sender();
    let usart2_receiver = usart2_channel.receiver();

    static USART2_READY: ReadySignal = Signal::new();

    // spawn receive tasks:
    unwrap!(spawner.spawn(usart2_read_task(usart2_rx, usart2_sender, &USART2_READY)));

    unwrap!(spawner.spawn(ping_task(usart2_tx, usart2_receiver, &USART2_READY)));
}

#[embassy_executor::task]
async fn net_task_1(stack: &'static Stack<Device<'static, IP_FRAME_SIZE>>) {
    stack.run().await;
}

#[embassy_executor::task]
async fn uart3_driver_task(
    mut task: AsyncHalfDuplexUart<
        UartRx<'static, USART3, DMA2_CH2>,
        UartTx<'static, USART3, DMA2_CH1>,
        AsyncBasicTimer<TIM6, TIM6I>,
    >,
) {
    task.start().await;
}

#[embassy_executor::task]
async fn uart3_hello_world(stack: &'static Stack<Device<'static, IP_FRAME_SIZE>>) {
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

#[embassy_executor::task]
async fn usart2_read_task(
    usart: UartRx<'static, USART2, DMA2_CH4>,
    sender: Sender<'static, NoopRawMutex, Frame, NUM_FRAMES>,
    signal: &'static ReadySignal,
) {
    read_subroutine(usart, sender, signal).await;
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

#[embassy_executor::task]
async fn ping_task(
    mut usart: UartTx<'static, USART2, DMA2_CH3>,
    receiver: Receiver<'static, NoopRawMutex, Frame, NUM_FRAMES>,
    signal: &'static ReadySignal,
) {
    signal.wait().await;

    loop {
        let received = receiver.recv().await;
        info!("received {} in ping task", received.as_str());
        usart.write(b"PING!").await.unwrap();
        Timer::after(Duration::from_millis(1000)).await;
    }
}
