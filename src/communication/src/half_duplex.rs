
use crate::BackoffHandler;
use crate::{AsyncDevice, AsyncTimer};
use crate::{Read, Write, WriteError};

use core::future;

use core::sync::atomic::{AtomicBool, Ordering};

use defmt::*;
use embassy_futures::select::select;
use embassy_net_driver_channel::{Runner, RxRunner, State, StateRunner, TxRunner};

use rand_core::RngCore;

pub type CommunicationState = State<IP_FRAME_SIZE, RECEIVE_SENDER_SIZE, TRANSMIT_CHANNEL_SIZE>;
struct TxHandler<T, W, R>
where
    T: AsyncTimer,
    W: Write,
    R: RngCore,
{
    write: W,
    tx_runner: TxRunner<'static, IP_FRAME_SIZE>,
    backoff_handler: BackoffHandler<T, R>,
    in_backoff: AtomicBool,
}

impl<T, W, R> TxHandler<T, W, R>
where
    T: AsyncTimer,
    W: Write,
    R: RngCore,
{
    pub fn new(timer: T, write: W, tx_runner: TxRunner<'static, IP_FRAME_SIZE>, rng: R) -> Self {
        Self {
            write,
            tx_runner,
            backoff_handler: BackoffHandler::new(timer, rng),
            in_backoff: AtomicBool::new(false),
        }
    }
    /*  CORRECTNESS:
    this function runs in 'parallel' with a receive function using a select! loop. Select works by dropping
    the future that does not complete. If this future is dropped: the following happens ->
    if we are in backoff mode and awiting backoff, this is not problematic b/c resume_backoff is stateful
    if we are in await_idle, the future will always be dropped when rx receives something
    if we try to increment backoff this is done synchronously so it cannot be dropped. i.e, if internal transmit is
    run after the await, then we are guaranteed to increment the backoff
     */
    pub async fn transmit(&mut self) {
        if self.in_backoff.load(Ordering::Relaxed) {
            self.backoff_handler
                .resume_backoff()
                .await
                .expect("timer should never be uninitialized!");
            self.in_backoff.store(false, Ordering::Relaxed);
        }
        if !self.write.is_line_free() {
            self.increment_backoff();
            self.await_idle().await;
        }
        let buf = self.tx_runner.tx_buf().await;
        let transmit_result = self.write.write(buf).await;
        // if an error happened: try again / cancel if too many errors
        match transmit_result {
            Ok(_) => self.on_transmit_complete(),
            Err(err) => match err {
                WriteError::FramingError => self.increment_backoff(),
                WriteError::CollisionError => self.increment_backoff(),
                _ => self.increment_backoff(),
            },
        };
    }

    fn on_transmit_complete(&mut self) {
        self.tx_runner.tx_done();
        self.backoff_handler.clear();
        self.in_backoff.store(false, Ordering::Relaxed);
    }
    /**
     * correctness: Since this is used in a select with the rx component in a loop,
     * having the rx future complete will drop this future, and the next future constructed
     * will have "waited for idle"
     */
    async fn await_idle(&mut self) {
        let () = future::pending().await;
    }

    /**
     * note: The smolltcp stack does not have any way for a device to report errors to the stack.
     * If you are really worried about this, use a protocol like tcp or introduce an on_error function
     */
    fn increment_backoff(&mut self) {
        self.in_backoff.store(true, Ordering::Relaxed);

        if let Err(_) = self.backoff_handler.increment_backoff() {
            info!("too many backoffs attempted...");
            // TODO: add a user defined error handler for this ? ?
            self.on_transmit_complete();
        }
    }
}

pub struct RxHandler<R: Read> {
    rx_runner: RxRunner<'static, IP_FRAME_SIZE>,
    read: R,
}
impl<R: Read> RxHandler<R> {
    pub fn new(read: R, rx_runner: RxRunner<'static, IP_FRAME_SIZE>) -> Self {
        Self { read, rx_runner }
    }
    pub async fn read(&mut self) {
        let buf = self.rx_runner.rx_buf().await;
        let r = self.read.read_until_idle(buf).await;
        if let Ok(s) = r {
            self.rx_runner.rx_done(s);
        } else {
            info!("read lost...");
        }
    }
}

pub struct AsyncHalfDuplexUart<R, W, T, RN>
where
    R: Read,
    W: Write,
    T: AsyncTimer,
    RN: RngCore,
{
    tx_handler: TxHandler<T, W, RN>,
    rx_handler: RxHandler<R>,
    state: StateRunner<'static>,
}

impl<R, W, T, RN> AsyncHalfDuplexUart<R, W, T, RN>
where
    R: Read,
    W: Write,
    T: AsyncTimer,
    RN: RngCore,
{
    pub fn new(
        read: R,
        write: W,
        timer: T,
        runner: Runner<'static, IP_FRAME_SIZE>,
        rng: RN,
    ) -> Self {
        let (state, rx, tx) = runner.split();
        return Self {
            tx_handler: TxHandler::new(timer, write, tx, rng),
            rx_handler: RxHandler::new(read, rx),
            state,
        };
    }

    pub async fn start(&mut self) -> ! {
        loop {
            select(self.tx_handler.transmit(), self.rx_handler.read()).await;
        }
    }
}

impl<R, W, T, RN> AsyncDevice for AsyncHalfDuplexUart<R, W, T, RN>
where
    R: Read,
    W: Write,
    T: AsyncTimer,
    RN: RngCore,
{
    async fn start(&mut self) -> ! {
        AsyncHalfDuplexUart::start(self).await
    }
}

pub const IP_FRAME_SIZE: usize = 1048;
const CHANNEL_SIZE: usize = 10;
const TRANSMIT_CHANNEL_SIZE: usize = CHANNEL_SIZE;
const RECEIVE_SENDER_SIZE: usize = CHANNEL_SIZE;
