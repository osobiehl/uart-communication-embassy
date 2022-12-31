pub mod init {
    use crate::async_timer::timer::AsyncBasicTimer;
    use crate::locator::locator;
    use embassy_stm32::pac::RCC;
    use embassy_stm32::rcc::{
        AHBPrescaler, APBPrescaler, ClockSrc, MSIRange, PLLClkDiv, PLLMul, PLLSAI1PDiv,
        PLLSAI1QDiv, PLLSAI1RDiv, PLLSource, PLLSrcDiv,
    };
    use embassy_stm32::rng::Rng;
    use embassy_stm32::time::Hertz;
    use embassy_stm32::usart::{Config as UartConfig, Uart};
    use embassy_stm32::{interrupt, Config};
    use {defmt_rtt as _, panic_probe as _};
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

    pub fn initialize() -> locator::Locator {
        let mut config = Config::default();
        config.rcc.mux = ClockSrc::MSI(MSI_RANGE);
        config.rcc.ahb_pre = AHBPrescaler::NotDivided;
        config.rcc.hsi48 = false;

        config.rcc.apb1_pre = APBPrescaler::NotDivided;
        config.rcc.apb2_pre = APBPrescaler::NotDivided;

        let peripherals = embassy_stm32::init(config);
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

        let lpuart = Uart::new(
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

        let usart3 = Uart::new(
            peripherals.USART3,
            peripherals.PC11,
            peripherals.PC10,
            irq_usart3,
            peripherals.DMA2_CH1,
            peripherals.DMA2_CH2,
            config_usart3,
        );

        let irq_usart2 = interrupt::take!(USART2);
        let mut config_usart2: UartConfig = Default::default();
        config_usart2.baudrate = 115200;

        let usart2 = Uart::new(
            peripherals.USART2,
            peripherals.PA3,
            peripherals.PA2,
            irq_usart2,
            peripherals.DMA2_CH3,
            peripherals.DMA2_CH4,
            config_usart2,
        );

        let timer = AsyncBasicTimer::new(peripherals.TIM5, interrupt::take!(TIM5), Hertz::mhz(1));

        let loc: locator::Locator = locator::Locator {
            tim15: Some(timer),
            lpuart: Some(lpuart),
            rng: Some(Rng::new(peripherals.RNG)),
            usart3: Some(usart3),
            usart2: Some(usart2),
        };

        return loc;
    }
}
