pub mod uart {
    use core::cell::{Cell, UnsafeCell};
    use core::cmp::min;
    use core::pin::Pin;

    use crate::communication::serial::{Read, ReadError, Write, WriteError};
    use core::mem;
    use defmt::info;
    use embassy_futures::select::{select, Either};
    use embassy_stm32::i2c::RxDma;
    use embassy_stm32::usart::{
        BasicInstance, Config, CtsPin, Error, RtsPin, RxPin, TxPin, Uart, UartRx, UartTx,
    };
    use embassy_stm32::{self, Peripheral};

    use static_cell::StaticCell;
    pub struct HalfDuplexUartRx<T, RxDma>(*mut UartRx<'static, T, RxDma>)
    where
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance;

    impl<'d, T, RxDma> Read for HalfDuplexUartRx<T, RxDma>
    where
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance,
    {
        async fn read_until_idle<'a>(
            &'a mut self,
            buf: &'a mut [u8],
        ) -> Result<usize, crate::communication::serial::ReadError>
        where
            Self: Sized,
        {
            unsafe { Read::read_until_idle(&mut *self.0, buf).await }
        }
    }

    impl<T, RxDma> HalfDuplexUartRx<T, RxDma>
    where
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance,
    {
        pub(crate) fn new(rx: *mut UartRx<'static, T, RxDma>) -> Self {
            Self(rx)
        }
    }

    pub struct HalfDuplexUartTx<T, TxDma, RxDma>
    where
        TxDma: embassy_stm32::usart::TxDma<T>,
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance,
    {
        tx_dma: TxDma,
        rx_dma: RxDma,
        tx: &'static mut UartTx<'static, T, TxDma>,
        rx: *mut UartRx<'static, T, RxDma>,
    }

    impl<T, TxDma, RxDma> HalfDuplexUartTx<T, TxDma, RxDma>
    where
        TxDma: embassy_stm32::usart::TxDma<T>,
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance,
    {
        pub(crate) fn new(
            tx: &'static mut UartTx<'static, T, TxDma>,
            rx: *mut UartRx<'static, T, RxDma>,
            rx_dma: RxDma,
            tx_dma: TxDma,
        ) -> Self {
            Self {
                tx,
                rx,
                rx_dma,
                tx_dma,
            }
        }

        fn disable_rx(&mut self) {
            self.rx_dma.request_stop();
            while self.rx_dma.is_running() {}
        }
        unsafe fn disable_tx(&mut self) {
            self.tx_dma.request_stop();
            while self.tx_dma.is_running() {}
        }

        fn collision_occurred(rx: &[u8; 5], tx: &[u8]) -> bool {
            let min_len = min(rx.len(), tx.len());
            for i in 0..min_len {
                if rx[i] != tx[i] {
                    return true;
                }
            }
            return false;
        }

        async unsafe fn duplex_transmit(&mut self, buffer: &[u8]) -> Result<(), WriteError> {
            self.disable_rx();
            let transmit_stolen = self.rx.as_mut().expect("cannot get rx pointer...");
            let mut rx_buf: [u8; 5] = [0; 5];
            let mut five_byte_read = Read::read_until_idle(transmit_stolen, &mut rx_buf);
            let mut transmit = self.tx.write(buffer);
            let p_transmit = Pin::new_unchecked(&mut transmit);
            let collision_result = select(p_transmit, five_byte_read).await;
            let res: Result<(), WriteError> = match collision_result {
                Either::First(_) => {
                    info!("tx finished first? is board properly set up?");

                    Err(WriteError::FramingError)
                }
                Either::Second(rx_res) => {
                    if rx_res.is_err() || Self::collision_occurred(&rx_buf, buffer) {
                        Err(WriteError::FramingError)
                    } else {
                        Ok(())
                    }
                }
            };
            if res.is_err() {
                // stop dma transfer
                self.tx_dma.request_stop();
                while self.tx_dma.is_running() {}
                return res;
            }
            // do comparison, cancel dma if err

            if let Err(e) = transmit.await {
                info!("error in receipt: {}", &e);
                return Err(WriteError::FramingError);
            }

            return Ok(());
        }
    }

    impl<T, TxDma, RxDma> Write for HalfDuplexUartTx<T, TxDma, RxDma>
    where
        TxDma: embassy_stm32::usart::TxDma<T>,
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance,
    {
        fn is_line_free(&self) -> bool {
            return true; //todo improve
        }
        async fn write<'a>(&'a mut self, buf: &'a [u8]) -> Result<(), WriteError>
        where
            Self: Sized,
        {
            unsafe { self.duplex_transmit(buf).await }
        }
    }

    pub fn new<T, TxDma, RxDma>(
        rx_: &'static mut UartRx<'static, T, RxDma>,
        tx_: &'static mut UartTx<'static, T, TxDma>,
        tx_dma: TxDma,
        rx_dma: RxDma,
    ) -> (
        HalfDuplexUartRx<T, RxDma>,
        HalfDuplexUartTx<T, TxDma, RxDma>,
    )
    where
        TxDma: embassy_stm32::usart::TxDma<T>,
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance,
    {
        let rx_mut_ptr: *mut UartRx<T, RxDma> =
            unsafe { mem::transmute(rx_ as *const UartRx<T, RxDma>) };

        let rx_mut_ptr_2: *mut UartRx<T, RxDma> =
            unsafe { mem::transmute(rx_ as *const UartRx<T, RxDma>) };
        let tx_component = HalfDuplexUartTx::new(tx_, rx_mut_ptr, rx_dma, tx_dma);
        let rx_component = HalfDuplexUartRx::new(rx_mut_ptr_2);
        return (rx_component, tx_component);
    }
}
