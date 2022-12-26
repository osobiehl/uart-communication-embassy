pub mod serial {
    use embassy_stm32::usart::{BasicInstance, UartRx, UartTx};

    #[derive(Debug)]
    #[non_exhaustive]
    pub enum WriteError {
        FramingError,
        CollisionError,
    }
    pub trait Write {
        async fn write<'a>(&'a mut self, buf: &'a [u8]) -> Result<(), WriteError>
        where
            Self: Sized;
    }

    #[derive(Debug)]
    #[non_exhaustive]
    pub enum ReadError {
        FramingError,
        OverflowError,
    }
    pub trait Read {
        async fn read_until_idle<'a>(&'a mut self, buf: &'a mut [u8]) -> Result<usize, ReadError>
        where
            Self: Sized;
    }

    impl<'d, T: BasicInstance, RxDma> Read for UartRx<'d, T, RxDma>
    where
        RxDma: embassy_stm32::usart::RxDma<T>,
    {
        async fn read_until_idle<'a>(&'a mut self, buf: &'a mut [u8]) -> Result<usize, ReadError> {
            match self.read_until_idle(buf).await {
                Ok(x) => Ok(x),
                Err(_) => Err(ReadError::FramingError),
            }
        }
    }

    impl<'d, T: BasicInstance, TxDma> Write for UartTx<'d, T, TxDma>
    where
        TxDma: embassy_stm32::usart::TxDma<T>,
    {
        async fn write<'a>(&'a mut self, buf: &'a [u8]) -> Result<(), WriteError> {
            match self.write(buf).await {
                Ok(_) => Ok(()),
                Err(_) => Err(WriteError::FramingError),
            }
        }
    }
}
