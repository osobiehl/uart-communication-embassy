pub mod uart {

    use core::cmp::min;
    use core::pin::Pin;
    use core::sync::atomic::{AtomicBool, Ordering};

    use crate::communication::serial::{Read, Write, WriteError};
    use core::mem;
    use defmt::info;
    use embassy_futures::select::{select, Either};

    use embassy_stm32::usart::{BasicInstance, UartRx, UartTx};
    use embassy_stm32::{self};

    use static_cell::StaticCell;
    pub struct HalfDuplexUartRx<T, RxDma>
    where
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance,
    {
        ptr: *mut UartRx<'static, T, RxDma>,
        stolen_signal: &'static AtomicBool,
    }

    impl<'d, T, RxDma> Read for HalfDuplexUartRx<T, RxDma>
    where
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance,
    {
        /**
         * read until idle interrupt. If this struct was signalled that rx was stolen,
         * wait for tx to complete instead and drop this future
         */
        async fn read_until_idle<'a>(
            &'a mut self,
            buf: &'a mut [u8],
        ) -> Result<usize, crate::communication::serial::ReadError>
        where
            Self: Sized,
        {
            self.stolen_signal.store(false, Ordering::SeqCst);
            let res = unsafe { Read::read_until_idle(&mut *self.ptr, buf).await };
            if true == self.stolen_signal.load(Ordering::SeqCst) {
                let () = core::future::pending().await;
            }
            res
        }
    }

    impl<T, RxDma> HalfDuplexUartRx<T, RxDma>
    where
        RxDma: embassy_stm32::usart::RxDma<T>,
        T: BasicInstance,
    {
        pub(crate) fn new(
            rx: *mut UartRx<'static, T, RxDma>,
            stolen_signal: &'static AtomicBool,
        ) -> Self {
            Self {
                ptr: rx,
                stolen_signal,
            }
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
        rx_stolen_signal: &'static AtomicBool,
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
            rx_stolen_signal: &'static AtomicBool,
        ) -> Self {
            Self {
                tx,
                rx,
                rx_dma,
                tx_dma,
                rx_stolen_signal,
            }
        }

        fn disable_rx(&mut self) {
            self.rx_dma.request_stop();
            while self.rx_dma.is_running() {}
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
            self.rx_stolen_signal.store(true, Ordering::SeqCst);
            let transmit_stolen = self.rx.as_mut().expect("cannot get rx pointer...");
            let mut rx_buf: [u8; 5] = [0; 5];
            let five_byte_read = Read::read_until_idle(transmit_stolen, &mut rx_buf);
            let mut transmit = self.tx.write(buffer);
            let p_transmit = Pin::new_unchecked(&mut transmit);
            let collision_result = select(p_transmit, five_byte_read).await;
            let res: Result<(), WriteError> = match collision_result {
                Either::First(_) => {
                    info!("tx finished first? is board properly set up?");

                    Err(WriteError::FramingError)
                }
                Either::Second(rx_res) => {
                    info!("{:?}", &rx_res);
                    if rx_res.is_err() || Self::collision_occurred(&rx_buf, buffer) {
                        Err(WriteError::CollisionError)
                    } else {
                        Ok(())
                    }
                }
            };
            // being extra safe to not have ordering issues...
            self.rx_stolen_signal.store(true, Ordering::SeqCst);

            info!("RESULT: {:?}", &res);
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

    // safety: we take a mutable reference to rx and tx adn taken_flag to ensure no other process can use them,
    // then we use them internally :)
    pub fn new<T, TxDma, RxDma>(
        rx_: &'static mut UartRx<'static, T, RxDma>,
        tx_: &'static mut UartTx<'static, T, TxDma>,
        taken_flag: &'static mut AtomicBool,
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
        let tx_component = HalfDuplexUartTx::new(tx_, rx_mut_ptr, rx_dma, tx_dma, taken_flag);
        let rx_component = HalfDuplexUartRx::new(rx_mut_ptr_2, taken_flag);
        return (rx_component, tx_component);
    }
}
