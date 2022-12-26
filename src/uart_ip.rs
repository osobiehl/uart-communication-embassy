pub mod IP {

    use crate::communication::serial::{Read, Write};
    use core::borrow::BorrowMut;
    use core::task::{Context, Waker};
    use embassy_net::device::{Device, LinkState, RxToken, TxToken};
    use embassy_sync::{
        blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
        channel::{Channel, Receiver, Sender},
        signal::Signal,
    };

    use heapless::pool::{self, Box, Init, Pool};
    pub struct IPDevice<R, W>
    where
        R: Read,
        W: Write,
    {
        write: W,
        read: R,
    }

    struct Frame<const N: usize>(pub [u8; N]);
    const IP_FRAME_SIZE: usize = 2048;

    struct FramePool<const N: usize> {
        pool: Pool<Frame<N>>,
    }
    impl<const N: usize> FramePool<N> {
        pub fn new(bytes: &'static mut [u8]) -> Self {
            let mut pool = Pool::new();
            pool.grow(bytes);
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

    struct UartRxToken(pub Box<Frame<IP_FRAME_SIZE>, Init>);

    impl RxToken for UartRxToken {
        fn consume<R, F>(self, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
            let result = f(&mut self.0 .0);
            result
        }
    }
    struct UartTxToken {}
    impl TxToken for UartTxToken {
        fn consume<R, F>(self, len: usize, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R,
        {
        }
    }
}
