#![feature(type_alias_impl_trait)]
#![feature(future_join)]
#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]
#![no_std]
use defmt::*;
use embassy_net_driver::Driver;

pub mod half_duplex;
use core::future::Future;
use embassy_net::Stack;
use embassy_time::Duration;

use rand_core::RngCore;
pub trait AsyncTimer {
    type AsyncOutput<'a>: Future<Output = ()> + 'a
    where
        Self: 'a;
    fn duration<'a>(&'a mut self, duration: Duration) -> Option<Self::AsyncOutput<'a>>;
    fn get_handle<'a>(&'a mut self) -> Option<Self::AsyncOutput<'a>>;
}

pub trait CoreServiceLocator {
    fn comm_stack_one(&mut self) -> Option<(&'static mut Stack<impl Driver>, impl AsyncDevice)>;

    fn comm_stack_two(&mut self) -> Option<(&'static mut Stack<impl Driver>, impl AsyncDevice)>;
}

#[derive(Debug, Format)]
#[non_exhaustive]
pub enum WriteError {
    FramingError,
    CollisionError,
}
pub trait Write {
    async fn write<'a>(&'a mut self, buf: &'a [u8]) -> Result<(), WriteError>
    where
        Self: Sized;
    fn is_line_free(&self) -> bool;
}

#[derive(Debug, Format)]
#[non_exhaustive]
#[allow(dead_code)]
pub enum ReadError {
    FramingError,
    OverflowError,
}

pub trait Read {
    async fn read_until_idle<'a>(&'a mut self, buf: &'a mut [u8]) -> Result<usize, ReadError>
    where
        Self: Sized;
}

pub struct BackoffState {
    pub in_backoff_state: bool,
    pub number_backoffs_attempted: usize,
    pub max_backoffs: usize,
}
impl Default for BackoffState {
    fn default() -> Self {
        Self {
            in_backoff_state: false,
            number_backoffs_attempted: 0,
            max_backoffs: 5,
        }
    }
}

pub trait AsyncDevice {
    async fn start(&mut self) -> !;
}

impl BackoffState {
    pub fn clear(&mut self) {
        self.in_backoff_state = false;
        self.number_backoffs_attempted = 0;
    }
}

pub struct BackoffHandler<T: AsyncTimer, R: RngCore> {
    timer: T,
    rng: R,
    state: BackoffState,
}

impl<T: AsyncTimer, R: RngCore> BackoffHandler<T, R> {
    pub fn new(timer: T, rng: R) -> Self {
        Self {
            timer,
            rng,
            state: Default::default(),
        }
    }

    pub fn increment_backoff(&mut self) -> Result<(), ()> {
        self.state.in_backoff_state = true;
        self.state.number_backoffs_attempted += 1;
        if self.state.number_backoffs_attempted >= self.state.max_backoffs {
            info!("backoff error!");
            self.state.clear();
            return Err(());
        } else {
            let to_wait = self.calculate_backoff();
            info!("waiting: {:?}", &to_wait);
            self.timer
                .duration(Duration::from_micros(to_wait as u64))
                .expect("could not start backoff timer!");
            return Ok(());
        }
    }

    pub async fn resume_backoff<'a>(&'a mut self) -> Result<(), ()> {
        if let Some(handle) = self.timer.get_handle() {
            handle.await;
            return Ok(());
        }
        Err(())
    }

    pub fn calculate_backoff(&mut self) -> usize {
        return self.exponential_component() + self.random_component() as usize;
    }
    fn exponential_component(&self) -> usize {
        const ONE_MS: usize = 1000;
        return ONE_MS << self.state.number_backoffs_attempted;
    }

    fn random_component(&mut self) -> u8 {
        let res = self.rng.next_u64();
        return res as u8;
    }

    pub fn clear(&mut self) {
        self.state.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
