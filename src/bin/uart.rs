#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::borrow::BorrowMut;

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, ClockSrc, MSIRange, PLLClkDiv, PLLMul, PLLSource, PLLSrcDiv,
    RccPeripheral,
};
use embassy_stm32::rng::Rng;
use embassy_stm32::Config;
use {defmt_rtt as _, panic_probe as _};

// #[cortex_m_rt::entry]
// fn main() -> ! {
//     let mut config = Config::default();
//     config.rcc.mux = ClockSrc::MSI(MSIRange::Range7);
//     config.rcc.ahb_pre = AHBPrescaler::NotDivided;
//     config.rcc.hsi48 = false;
//     config.rcc.pllsai1 = None;
//     let peripherals = embassy_stm32::init(config);
//     loop {
//         info!("hello world!")
//     }
// }

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    config.rcc.mux = ClockSrc::MSI(MSIRange::Range7);
    config.rcc.ahb_pre = AHBPrescaler::NotDivided;
    config.rcc.hsi48 = false;
    config.rcc.pllsai1 = None;
    config.rcc.apb1_pre = APBPrescaler::NotDivided;
    config.rcc.apb2_pre = APBPrescaler::NotDivided;
    let peripherals = embassy_stm32::init(config);

    info!("walla billa");

    let mut rng = Rng::new(peripherals.RNG);

    info!("doesnt work after here :(");
    let mut buf = [0u8; 16];
    unwrap!(rng.async_fill_bytes(&mut buf).await);
    info!("random bytes: {:02x}", buf);
}
