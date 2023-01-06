pub mod serial {
    use communication::{Read, ReadError, Write, WriteError};
    use defmt::*;
    use embassy_stm32::usart::{BasicInstance, UartRx, UartTx};
    pub struct BasicUartRx<'d, T, RxDma>(UartRx<'d, T, RxDma>)
    where
        T: BasicInstance,
        RxDma: embassy_stm32::usart::RxDma<T>;

    impl<'d, T, RxDma> Read for BasicUartRx<'d, T, RxDma>
    where
        T: BasicInstance,
        RxDma: embassy_stm32::usart::RxDma<T>,
    {
        async fn read_until_idle<'a>(&'a mut self, buf: &'a mut [u8]) -> Result<usize, ReadError> {
            match self.0.read_until_idle(buf).await {
                Ok(x) => Ok(x),
                Err(_) => Err(ReadError::FramingError),
            }
        }
    }

    impl<'d, T, RxDma> From<UartRx<'d, T, RxDma>> for BasicUartRx<'d, T, RxDma>
    where
        T: BasicInstance,
        RxDma: embassy_stm32::usart::RxDma<T>,
    {
        fn from(value: UartRx<'d, T, RxDma>) -> Self {
            Self(value)
        }
    }

    pub struct BasicUartTx<'d, T, TxDma>(UartTx<'d, T, TxDma>)
    where
        T: BasicInstance,
        TxDma: embassy_stm32::usart::TxDma<T>;

    impl<'d, T: BasicInstance, TxDma> Write for BasicUartTx<'d, T, TxDma>
    where
        TxDma: embassy_stm32::usart::TxDma<T>,
    {
        async fn write<'a>(&'a mut self, buf: &'a [u8]) -> Result<(), WriteError> {
            match self.0.write(buf).await {
                Ok(_) => Ok(()),
                Err(_) => Err(WriteError::FramingError),
            }
        }
        fn is_line_free(&self) -> bool {
            true
        }
    }

    impl<'d, T, TxDma> From<UartTx<'d, T, TxDma>> for BasicUartTx<'d, T, TxDma>
    where
        T: BasicInstance,
        TxDma: embassy_stm32::usart::TxDma<T>,
    {
        fn from(value: UartTx<'d, T, TxDma>) -> Self {
            Self(value)
        }
    }
}
