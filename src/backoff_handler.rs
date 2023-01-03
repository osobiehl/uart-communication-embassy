pub mod backoff {
    use crate::async_timer::timer::AsyncTimer;
    use defmt::*;

    use embassy_time::Duration;
    use rand_core::{impls, RngCore};

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

    pub struct DummyRng {}
    impl rand_core::RngCore for DummyRng {
        fn next_u32(&mut self) -> u32 {
            5
        }
        fn next_u64(&mut self) -> u64 {
            5
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            impls::fill_bytes_via_next(self, dest)
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
            Ok(self.fill_bytes(dest))
        }
    }
}
