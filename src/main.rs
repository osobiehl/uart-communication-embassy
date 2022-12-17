#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod init;
use cortex_m::interrupt::enable;
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::pac::{
    rcc,
    rng::{self, Rng as RawRng},
    RCC, RNG,
};
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, ClockSrc, MSIRange, PLLClkDiv, PLLMul, PLLSAI1PDiv, PLLSAI1QDiv,
    PLLSAI1RDiv, PLLSource, PLLSrcDiv, RccPeripheral,
};
use embassy_stm32::rng::Rng;
use embassy_stm32::Config;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("walla billa");
    let peripherals = init::init::initialize();
    let mut rng = Rng::new(peripherals.RNG);

    info!("doesnt work after here :(");
    let mut buf = [0u8; 16];
    loop {
        unwrap!(rng.async_fill_bytes(&mut buf).await);
        info!("random bytes: {:02x}", buf);
    }
}
