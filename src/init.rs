pub mod init {
    use core::sync::atomic::AtomicBool;

    use crate::async_timer::timer::AsyncBasicTimer;
    use crate::half_duplex;

    use crate::locator::locator;
    use crate::service::service::CoreServiceLocator;
    use embassy_stm32::pac::RCC;
    use embassy_stm32::peripherals::{DMA2_CH1, DMA2_CH2, DMA2_CH3, DMA2_CH4, USART2, USART3};
    use embassy_stm32::rcc::{
        AHBPrescaler, APBPrescaler, ClockSrc, MSIRange, PLLClkDiv, PLLMul, PLLSAI1PDiv,
        PLLSAI1QDiv, PLLSAI1RDiv, PLLSource, PLLSrcDiv,
    };
    use embassy_stm32::rng::Rng;
    use embassy_stm32::time::Hertz;
    use embassy_stm32::usart::{Config as UartConfig, Uart, UartRx, UartTx};
    use embassy_stm32::{interrupt, Config, Peripheral};
    use static_cell::StaticCell;
    use {defmt_rtt as _, panic_probe as _};

    macro_rules! singleton {
        ($val:expr) => {{
            type T = impl Sized;
            static STATIC_CELL: StaticCell<T> = StaticCell::new();
            STATIC_CELL.init_with(move || $val)
        }};
    }
    const MSI_RANGE: MSIRange = MSIRange::Range7; // 8 MHz;

    impl ToPLL for ClockSrc {
        fn to_pll_selection(&self) -> u8 {
            match self {
                ClockSrc::HSE(_) => 0b11,
                ClockSrc::MSI(_) => 0b01,
                ClockSrc::HSI16 => 0b10,
                _ => core::panic!("invalid clk source!"),
            }
        }
    }

    trait ToHertz {
        fn to_hertz(&self) -> u32;
    }

    impl ToHertz for ClockSrc {
        fn to_hertz(&self) -> u32 {
            match self {
                ClockSrc::MSI(range) => (*range).into(),
                ClockSrc::HSE(range) => range.0,
                ClockSrc::HSI16 => 16_000_000,
                ClockSrc::PLL(p, _, _, _, _) => match p {
                    PLLSource::HSI16 => 16_000_000,
                    PLLSource::MSI(range) => u32::from(*range),
                    PLLSource::HSE(freq) => freq.0,
                },
            }
        }
    }

    trait ToPLL {
        fn to_pll_selection(&self) -> u8;
    }
    unsafe fn enable_48_mhz_pllsai1(
        pll_source: ClockSrc,
        output_multiplier: PLLMul,
        source_divider: PLLSrcDiv,
        adc_clock_divider: Option<PLLSAI1RDiv>,
        pll_clock_divider: Option<PLLSAI1QDiv>,
        sai_1_2_divider: Option<PLLSAI1PDiv>,
    ) {
        RCC.pllsai1cfgr().write(move |w| {
            w.set_pllsai1n(output_multiplier.into());
            w.set_pllsai1m(source_divider.into());
            if let Some(r_div) = adc_clock_divider {
                w.set_pllsai1r(r_div.into());
                w.set_pllsai1ren(true);
            }
            if let Some(q_div) = pll_clock_divider {
                w.set_pllsai1q(q_div.into());
                w.set_pllsai1qen(true);
                let freq = (pll_source.to_hertz() / source_divider.to_div()
                    * output_multiplier.to_mul())
                    / q_div.to_div();
                core::assert!(
                    freq == 48_000_000,
                    "inorrect frequency! got {}, expected: 48000000",
                    freq
                );

                RCC.ccipr1().modify(|w| {
                    w.set_clk48msel(0b1);
                });
            }
            if let Some(sai_1) = sai_1_2_divider {
                w.set_pllsai1pdiv(sai_1.into());
                w.set_pllsai1pen(true);
            }
            w.set_pllsai1src(pll_source.to_pll_selection());
        });

        RCC.cr().modify(|w| w.set_pllsai1on(true));
    }

    pub fn initialize() -> impl CoreServiceLocator {
        let mut config = Config::default();
        config.rcc.mux = ClockSrc::MSI(MSI_RANGE);
        config.rcc.ahb_pre = AHBPrescaler::NotDivided;
        config.rcc.hsi48 = false;

        config.rcc.apb1_pre = APBPrescaler::NotDivided;
        config.rcc.apb2_pre = APBPrescaler::NotDivided;

        let mut peripherals = embassy_stm32::init(config);
        unsafe {
            enable_48_mhz_pllsai1(
                ClockSrc::MSI(MSI_RANGE),
                PLLMul::Mul12,
                PLLSrcDiv::Div1,
                Some(PLLClkDiv::Div2),
                Some(PLLClkDiv::Div2),
                Some(PLLClkDiv::Div2),
            );
        }

        // initialize lpuart
        let irq_lpuart = interrupt::take!(LPUART1);
        let mut config_lpuart: UartConfig = Default::default();
        config_lpuart.baudrate = 115200;

        let _lpuart = Uart::new(
            peripherals.LPUART1,
            peripherals.PG8,
            peripherals.PG7,
            irq_lpuart,
            peripherals.DMA1_CH1,
            peripherals.DMA1_CH2,
            config_lpuart,
        );

        let irq_usart3 = interrupt::take!(USART3);
        let mut config_usart3: UartConfig = Default::default();
        config_usart3.baudrate = 115200;

        let u3_tx_dma = unsafe { peripherals.DMA2_CH1.clone_unchecked() };
        let u3_rx_dma = unsafe { peripherals.DMA2_CH2.clone_unchecked() };
        let usart3 = Uart::new(
            peripherals.USART3,
            peripherals.PC11,
            peripherals.PC10,
            irq_usart3,
            peripherals.DMA2_CH1,
            peripherals.DMA2_CH2,
            config_usart3,
        );

        // compiler does not let singleton! macro work off the bat, so I must do this
        let (u3tx, u3rx): (
            UartTx<'static, USART3, DMA2_CH1>,
            UartRx<'static, USART3, DMA2_CH2>,
        ) = usart3.split();

        static UART3_A: StaticCell<UartTx<'static, USART3, DMA2_CH1>> = StaticCell::new();
        static UART3_B: StaticCell<UartRx<'static, USART3, DMA2_CH2>> = StaticCell::new();
        let u3tx = UART3_A.init_with(|| u3tx);
        let u3rx = UART3_B.init_with(|| u3rx);
        let uart3_take_flag = singleton!(AtomicBool::new(false));
        let (half_duplex_uart_3_rx, half_duplex_uart_3_tx) =
            half_duplex::uart::new(u3rx, u3tx, uart3_take_flag, u3_tx_dma, u3_rx_dma);

        let irq_usart2 = interrupt::take!(USART2);
        let mut config_usart2: UartConfig = Default::default();
        config_usart2.baudrate = 115200;

        let u2_tx_dma = unsafe { peripherals.DMA2_CH3.clone_unchecked() };
        let u2_rx_dma = unsafe { peripherals.DMA2_CH4.clone_unchecked() };

        let usart2 = Uart::new(
            peripherals.USART2,
            peripherals.PA3,
            peripherals.PA2,
            irq_usart2,
            peripherals.DMA2_CH3,
            peripherals.DMA2_CH4,
            config_usart2,
        );

        static UART_2_INIT: StaticCell<(
            UartTx<'static, USART2, DMA2_CH3>,
            UartRx<'static, USART2, DMA2_CH4>,
        )> = StaticCell::new();

        let (u2tx, u2rx) = UART_2_INIT.init_with(|| usart2.split());
        let usart2_take_flag = singleton!(AtomicBool::new(false));
        let (half_duplex_uart_2_rx, half_duplex_uart_2_tx) =
            half_duplex::uart::new(u2rx, u2tx, usart2_take_flag, u2_tx_dma, u2_rx_dma);

        let timer = AsyncBasicTimer::new(peripherals.TIM6, interrupt::take!(TIM6), Hertz::mhz(1));
        let timer2 = AsyncBasicTimer::new(peripherals.TIM7, interrupt::take!(TIM7), Hertz::mhz(1));
        let loc = locator::HardwareLocator {
            tim7: Some(timer2),
            tim6: Some(timer),
            dummy_rng: Some(crate::backoff_handler::backoff::DummyRng {}),
            usart2_rx: Some(half_duplex_uart_2_rx),
            usart2_tx: Some(half_duplex_uart_2_tx),
            usart3_rx: Some(half_duplex_uart_3_rx),
            usart3_tx: Some(half_duplex_uart_3_tx),
            rng: Some(Rng::new(peripherals.RNG)),
        };

        return loc;
    }
}
