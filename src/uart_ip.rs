use crate::communication::serial::{Read, ReadError, Write, WriteError};
    use defmt::*;
    use core::borrow::BorrowMut;
    use core::sync::atomic::{AtomicBool, Ordering};
    use core::task::{Context, Waker};
    use core::future::poll_fn;
    use embassy_futures::{select::select, select::Either};
    use embassy_net_driver_channel::{Device, driver, State, RxToken, TxToken, StateRunner, RxRunner, TxRunner, Runner};
    use embassy_sync::{
        blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
        channel::{Channel, Receiver, Sender},
        signal::Signal,
    };
    use crate::async_timer::timer::AsyncTimer;


    use heapless::pool::{self, Box, Init, Pool};
    pub type FrameBox = Box<Frame<IP_FRAME_SIZE>, Init>;
    pub struct IPDeviceWrapper<R, W>
    where
        R: Read,
        W: Write, {}

    pub struct AsyncHalfDuplexUart<R, W, T>
    where
        R: Read,
        W: Write,
        T: AsyncTimer
    {
        runner: Runner<'static, IP_FRAME_SIZE>, 
        read: R,
        write: W,
        timer: T,
        backoff_state: BackoffState,
        
    }

    pub struct BackoffState{
        pub in_backoff_state: bool,
        pub number_backoffs_attempted: usize,
        pub max_backoffs: usize,
    }
    impl Default for BackoffState{
        fn default() -> Self {
            Self { in_backoff_state: false, number_backoffs_attempted: 0, max_backoffs: 5 }
        }
    }
    impl BackoffState{
        pub fn clear(&mut self) {
            self.in_backoff_state = false;
            self.number_backoffs_attempted = 0;
        }
    }

    struct TxHandler<T, W> where T: AsyncTimer, W: Write 
    {
        timer: T,
        write: W,
        tx_runner: TxRunner<'static, IP_FRAME_SIZE>,
        backoff_state: BackoffState
    }

    impl<T, W> TxHandler where T: AsyncTimer, W: Write {
        pub fn new(timer: T, write: w, tx_runner: TxRunner<'static, IP_FRAME_SIZE> ) -> Self {
            Self{timer, write, tx_runner, backoff_state: Default::default()}
        }
        pub async fn transmit(&mut self) -> WriteError {
            
        }
    }


    impl<R, W, T> AsyncHalfDuplexUart<R, W, T>
    where
        R: Read,
        W: Write,
        T: AsyncTimer
    {
        pub async fn new(read: R, write: W, timer: T, runner: Runner<'static, IP_FRAME_SIZE>) -> Self {
            let (state, rx, tx  ) = runner.split();
            return Self{
                read, write, runner, timer, backoff_state: Default::default()
            }
        }

        async fn wait_for_tx_or_rx_reenable(&self) {
            let wait_for_rx_reenable = poll_fn(|ctx| 
                self.runner.poll_rx_buf(ctx));

    
                match select(wait_for_rx_reenable, self.runner.tx_buf()).await {
                    Either::First( buf ) => self.wait_for_tx_or_rx(),
                    Either::Second() => todo!()
    
                }

        }
        // TODO: construct a future for transmit that checks state. If it had a collision
        // it should wait on a timer. If not, it should wait on the TX runner
        async fn wait_for_tx_message_or_rx_received(& mut self, rx_buf: &mut [u8]) {
            match select(self.read.read_until_idle(rx_buf), self.runner.tx_buf() ).await {
                Either::First(read_result) => self.on_rx_done(read_result),
                Either::Second(to_send) => self.write.write(to_send).await
            }

        }

        async fn transmit(&mut self){
            
        }

        async fn write_bytes(&mut self, to_send: &mut u8) {
            match self.write.write(to_send).await {

            }
        }

        fn on_rx_done(&mut self, res: Result<usize, ReadError>) {
            match res {
                Ok(bytes_read) => self.runner.rx_done(bytes_read),
                Err(e) => info!("UART READ ERROR!")
            }

        }
        pub async fn start(&mut self) -> ! {

            loop {

                
                let mut current_rx_frame = self.pool.allocate();
                if current_rx_frame.is_none() {
                    self.disable_rx();
                }

                if (self.rx_enabled.load(Ordering::Relaxed) == true) {
                    let current_rx_frame = current_rx_frame.unwrap();
                    let event = select(
                        self.read.read_until_idle(&mut current_rx_frame.0),
                        self.tx_request_receiver.recv(),
                    )
                    .await;
                    match event {
                        First(read_result) => {
                            self.handle_read_result(current_rx_frame, read_result)
                        }
                        Second(queue_request) => self.handle_queue_request(queue_request),
                    }
                } else {
                    // todo: queue request should be an enum in case  of OOM reenable
                    self.handle_queue_request(self.tx_request_receiver.recv().await);
                }
            }
        }

        pub fn handle_read_result(&self, frame: FrameBox, res: Result<usize, ReadError>) {
            match res{
                Ok(bytes_read) => 
            }
        }
        pub fn handle_queue_request(&self, req: FrameBox) {}

        pub fn disable_rx(&self) {
            self.rx_enabled.store(false, Ordering::Relaxed);
        }
        pub fn enable_rx(&self) {
            // TODO send an enable rx message
            self.rx_enabled.store(true, Ordering::Relaxed);
        }
    }

    struct Frame<const N: usize>(pub [u8; N]);

    const IP_FRAME_SIZE: usize = 2048;
    const CHANNEL_SIZE: usize = 20;
    const TRANSMIT_CHANNEL_SIZE: usize = CHANNEL_SIZE;
    const RECEIVE_SENDER_SIZE: usize = CHANNEL_SIZE;

    struct FramePool<const N: usize> {
        pool: Pool<Frame<N>>,
    }
    impl<const N: usize> FramePool<N> {
        pub fn new(bytes: &'static mut [u8]) -> Self {
            let mut pool = Pool::new();
            pool.grow(bytes);
            let c: Channel<NoopRawMutex, usize, 3> = Channel::new();

            Self { pool }
        }

        pub fn allocate(&self) -> Option<Box<Frame<N>, Init>> {
            let frame = self.pool.alloc().map(|b| b.init(Frame([0; N])));
            return frame;
        }

        pub fn free<S>(&self, value: Box<Frame<N>, S>) {
            self.pool.free(value);
        }
    }

    struct UartRxToken(pub FrameBox);

    impl RxToken for UartRxToken {
        fn consume<R, F>(self, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
            let result = f(&mut self.0 .0);
            result
        }
    }
    struct UartTxToken { 
        sender:  Sender<'static, CriticalSectionRawMutex, FrameBox, TRANSMIT_CHANNEL_SIZE>, 
        value: FrameBox
    }

    impl TxToken for UartTxToken {
        fn consume<R, F>(self, len: usize, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
            let r = F(&mut self.value.0);
            self.sender.send(self.value);
            r

        }
    }

