pub mod locator {
    use crate::async_timer::timer::AsyncTimer;
    use crate::backoff_handler::backoff::DummyRng;
    use crate::communication::serial::{Read, Write};
    use crate::half_duplex::uart::{HalfDuplexUartRx, HalfDuplexUartTx};

    use crate::AsyncBasicTimer;

    use embassy_stm32::interrupt::{TIM6 as TIM6I, TIM7 as TIM7I};
    use embassy_stm32::peripherals::{
        DMA1_CH1, DMA1_CH2, DMA2_CH1, DMA2_CH2, DMA2_CH3, DMA2_CH4, LPUART1, RNG, TIM6, TIM7,
        USART2, USART3,
    };
    use embassy_stm32::rng::Rng;
    use embassy_stm32::usart::Uart;

    use rand_core::RngCore;

    pub type _LpUart = Uart<'static, LPUART1, DMA1_CH1, DMA1_CH2>;
    pub type Usart3Rx = HalfDuplexUartRx<USART3, DMA2_CH2>;
    pub type Usart3Tx = HalfDuplexUartTx<USART3, DMA2_CH1, DMA2_CH2>;

    pub type Usart2Rx = HalfDuplexUartRx<USART2, DMA2_CH4>;
    pub type Usart2Tx = HalfDuplexUartTx<USART2, DMA2_CH3, DMA2_CH4>;

    // #[derive(Default)]
    pub struct HardwareLocator {
        pub rng: Option<Rng<'static, RNG>>,
        pub dummy_rng: Option<DummyRng>,
        pub usart3_rx: Option<Usart3Rx>,
        pub usart3_tx: Option<Usart3Tx>,
        pub usart2_rx: Option<Usart2Rx>,
        pub usart2_tx: Option<Usart2Tx>,
        pub tim6: Option<AsyncBasicTimer<TIM6, TIM6I>>,
        pub tim7: Option<AsyncBasicTimer<TIM7, TIM7I>>,
    }

    impl Locator for HardwareLocator {
        fn rng_channel_one(&mut self) -> Option<impl RngCore> {
            self.rng.take()
        }
        fn rng_channel_two(&mut self) -> Option<impl RngCore> {
            self.dummy_rng.take()
        }
        fn rx_channel_one(&mut self) -> Option<impl Read> {
            self.usart2_rx.take()
        }
        fn tx_channel_one(&mut self) -> Option<impl Write> {
            self.usart2_tx.take()
        }
        fn rx_channel_two(&mut self) -> Option<impl Read> {
            self.usart3_rx.take()
        }
        fn tx_channel_two(&mut self) -> Option<impl Write> {
            self.usart3_tx.take()
        }
        fn timer_channel_one(&mut self) -> Option<impl AsyncTimer> {
            self.tim6.take()
        }
        fn timer_channel_two(&mut self) -> Option<impl AsyncTimer> {
            self.tim7.take()
        }
    }

    pub trait Locator {
        fn tx_channel_one(&mut self) -> Option<impl Write>;
        fn rx_channel_one(&mut self) -> Option<impl Read>;
        fn tx_channel_two(&mut self) -> Option<impl Write>;
        fn rx_channel_two(&mut self) -> Option<impl Read>;
        fn timer_channel_one(&mut self) -> Option<impl AsyncTimer>;
        fn timer_channel_two(&mut self) -> Option<impl AsyncTimer>;
        fn rng_channel_one(&mut self) -> Option<impl RngCore>;
        fn rng_channel_two(&mut self) -> Option<impl RngCore>;
    }
}
