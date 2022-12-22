pub mod locator {
    use embassy_stm32::peripherals::{
        DMA1_CH1, DMA1_CH2, DMA2_CH1, DMA2_CH2, DMA2_CH3, DMA2_CH4, LPUART1, RNG, USART2, USART3,
    };

    use embassy_stm32::rng::Rng;
    use embassy_stm32::usart::Uart;
    pub type LpUart = Uart<'static, LPUART1, DMA1_CH1, DMA1_CH2>;
    pub type Usart3 = Uart<'static, USART3, DMA2_CH1, DMA2_CH2>;
    pub type Usart2 = Uart<'static, USART2, DMA2_CH3, DMA2_CH4>;

    pub struct Locator {
        pub lpuart: Option<LpUart>,
        pub rng: Option<Rng<'static, RNG>>,
        pub usart3: Option<Usart3>,
        pub usart2: Option<Usart2>,
    }
}
