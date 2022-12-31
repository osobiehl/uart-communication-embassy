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
// mod uart_ip;
use core::future;
use core::str;

use async_timer::timer::{AsyncBasicTimer, TimerFuture};
use communication::serial;
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::peripherals::{DMA2_CH1, DMA2_CH2, DMA2_CH3, DMA2_CH4, USART2, USART3};
use embassy_stm32::usart::{UartRx, UartTx};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::{Channel, Receiver, Sender},
    signal::Signal,
};
use embassy_time::{Duration, Instant, Timer};
use heapless::String;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

type Frame = String<128>;
const NUM_FRAMES: usize = 16;
type ReadySignal = Signal<CriticalSectionRawMutex, ()>;

// type Channel = channel::Channel<blocking_mutex::NoopMutex<Frame>, Frame, NUM_FRAMES>;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let mut locator = init::init::initialize();

    let mut tim = locator.tim15.unwrap();
    tim.duration(Duration::from_micros(0xf0))
        .expect("timer start failed!")
        .await;
    tim.duration(Duration::from_micros(256))
        .expect("second timer init failed")
        .await;
    // let mut _rng = locator.rng.take().expect("taking rng peripheral failed!");
    // let _lpuart = locator.lpuart.take().expect("taking lpuart failed!");
    let usart3 = locator.usart3.take().expect("taking usart3 failed!");
    let usart2 = locator.usart2.take().expect("taking usart2 failed!");

    let (usart2_tx, usart2_rx) = usart2.split();
    let (usart3_tx, usart3_rx) = usart3.split();

    static USART3_CHANNEL: StaticCell<Channel<NoopRawMutex, Frame, NUM_FRAMES>> = StaticCell::new();
    let mut usart3_channel = USART3_CHANNEL.init(Channel::new());

    let usart3_sender = usart3_channel.sender();
    let usart3_receiver = usart3_channel.receiver();

    static USART2_CHANNEL: StaticCell<Channel<NoopRawMutex, Frame, NUM_FRAMES>> = StaticCell::new();
    let usart2_channel = USART2_CHANNEL.init(Channel::new());

    let usart2_sender = usart2_channel.sender();
    let usart2_receiver = usart2_channel.receiver();

    static USART2_READY: ReadySignal = Signal::new();
    static USART3_READY: ReadySignal = Signal::new();

    let usart3_task_signal: Signal<CriticalSectionRawMutex, ()> = Signal::new();

    // spawn receive tasks:
    unwrap!(spawner.spawn(usart2_read_task(usart2_rx, usart2_sender, &USART2_READY)));
    unwrap!(spawner.spawn(usart3_read_task(usart3_rx, usart3_sender, &USART3_READY)));

    unwrap!(spawner.spawn(ping_task(usart2_tx, usart2_receiver, &USART2_READY)));
    unwrap!(spawner.spawn(pong_task(usart3_tx, usart3_receiver, &USART3_READY)));
}

#[embassy_executor::task]
async fn usart2_read_task(
    usart: UartRx<'static, USART2, DMA2_CH4>,
    sender: Sender<'static, NoopRawMutex, Frame, NUM_FRAMES>,
    signal: &'static ReadySignal,
) {
    read_subroutine(usart, sender, signal).await;
}

#[embassy_executor::task]
async fn usart3_read_task(
    usart: UartRx<'static, USART3, DMA2_CH2>,
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
    let mut buf: [u8; 128] = [0; 128];
    loop {
        signal.signal(());
        let bytes_read = usart.read_until_idle(&mut buf).await.unwrap();
        let x = str::from_utf8(&mut buf[..bytes_read]).unwrap();
        let string: Frame = heapless::String::from(x);
        unwrap!(sender.try_send(string));
    }
}

#[embassy_executor::task]
async fn pong_task(
    mut usart: UartTx<'static, USART3, DMA2_CH1>,
    receiver: Receiver<'static, NoopRawMutex, Frame, NUM_FRAMES>,
    signal: &'static ReadySignal,
) {
    signal.wait().await;
    loop {
        usart.write(b"PONG!").await.unwrap();
        let received = receiver.recv().await;

        info!("received {} in pong task", received.as_str());
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
