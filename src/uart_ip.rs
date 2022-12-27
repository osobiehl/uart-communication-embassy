pub mod IP {

    use crate::communication::serial::{Read, ReadError, Write, WriteError};
    use core::borrow::BorrowMut;
    use core::sync::atomic::{AtomicBool, Ordering};
    use core::task::{Context, Waker};
    use embassy_futures::{select::select, select::Either};
    use embassy_net::device::{Device, LinkState, RxToken, TxToken};
    use embassy_stm32::pac::bdma::Ch;
    use embassy_sync::{
        blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
        channel::{Channel, Receiver, Sender},
        signal::Signal,
    };

    use heapless::pool::{self, Box, Init, Pool};
    pub type FrameBox = Box<Frame<IP_FRAME_SIZE>, Init>;
    pub struct IPDeviceWrapper<R, W>
    where
        R: Read,
        W: Write, {}

    pub struct AsyncHalfDuplexUart<R, W>
    where
        R: Read,
        W: Write,
    {
        pool: &'static FramePool<IP_FRAME_SIZE>,
        tx_request_sender:
            Sender<'static, CriticalSectionRawMutex, FrameBox, TRANSMIT_CHANNEL_SIZE>,
        tx_request_receiver:
            Receiver<'static, CriticalSectionRawMutex, FrameBox, TRANSMIT_CHANNEL_SIZE>,
        rx_sender: Sender<'static, NoopRawMutex, UartRxToken, RECEIVE_SENDER_SIZE>,
        write: W,
        read: R,
        rx_enabled: AtomicBool,
    }

    impl<R, W> AsyncHalfDuplexUart<R, W>
    where
        R: Read,
        W: Write,
    {
        pub async fn new(
            pool: &'static FramePool<IP_FRAME_SIZE>,
            tx_request_sender: Sender<
                'static,
                CriticalSectionRawMutex,
                FrameBox,
                TRANSMIT_CHANNEL_SIZE,
            >,
            tx_request_receiver: Receiver<
                'static,
                CriticalSectionRawMutex,
                FrameBox,
                TRANSMIT_CHANNEL_SIZE,
            >,
            rx_sender: Sender<'static, NoopRawMutex, FrameBox, RECEIVE_SENDER_SIZE>,
            write: W,
            read: R,
        ) -> Self {
            Self {
                pool,
                tx_request_receiver,
                tx_request_sender,
                rx_sender,
                write,
                read,
                rx_enabled: AtomicBool::new(false),
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
}
