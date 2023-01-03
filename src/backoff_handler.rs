pub mod backoff {
    use crate::async_timer::timer::AsyncTimer;
    use defmt::*;
    use embassy_stm32::peripherals::RNG;
    use embassy_stm32::rng::{Error, Rng};
    use embassy_time::Duration;
    use rand_core::RngCore;

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
    impl BackoffState {
        pub fn clear(&mut self) {
            self.in_backoff_state = false;
            self.number_backoffs_attempted = 0;
        }
    }

    pub struct BackoffHandler<T: AsyncTimer> {
        timer: T,
        rng: Rng<'static, RNG>,
        state: BackoffState,
    }

    impl<T: AsyncTimer> BackoffHandler<T> {
        pub fn new(timer: T, rng: Rng<'static, RNG>) -> Self {
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
                let to_wait = self.random_component();
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

        pub async fn calculate_backoff(&mut self) -> usize {
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
}
