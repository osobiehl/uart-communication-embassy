pub mod timer {
    // use embassy_stm32::pac::TIM15;
    use embassy_stm32::peripherals::TIM15;
    use embassy_stm32::time::Hertz;
    use embassy_stm32::timer::{self, Basic16bitInstance, GeneralPurpose16bitInstance};

    use core::future::{Future, IntoFuture};
    use core::marker::PhantomData;
    use core::mem::{self, transmute, MaybeUninit};
    use core::sync::atomic::AtomicBool;
    use core::task::{Context, Poll, Waker};
    use embassy_stm32::interrupt::InterruptExt;
    use embassy_time::Duration;
    use static_cell::StaticCell;

    use cortex_m;

    trait AsyncTimer {
        async fn duration<'a>(&'a mut self, duration: Duration) -> Option<impl Future + 'a>;
    }

    impl<INS, INT> AsyncTimer for AsyncBasicTimer<INS, INT>
    where
        INS: Basic16bitInstance,
        INT: InterruptExt,
    {
        async fn duration<'a>(&'a mut self, duration: Duration) -> Option<impl Future + 'a> {
            AsyncBasicTimer::duration(self, duration)
        }
    }
    pub struct AsyncBasicTimer<INS, INT>
    where
        INS: Basic16bitInstance,
        INT: InterruptExt,
    {
        timer_instance: INS,
        interrupt_instance: INT,
        run_once: bool,
        expired: bool,
        context: MaybeUninit<core::task::Waker>,
    }

    impl<'a, INS, INT> Future for TimerFuture<'a, INS, INT>
    where
        INS: Basic16bitInstance,
        INT: InterruptExt,
    {
        type Output = ();
        fn poll(
            mut self: core::pin::Pin<&mut Self>,
            cx: &mut core::task::Context<'_>,
        ) -> core::task::Poll<Self::Output> {
            if false == self.0.run_once {
                self.0.context.write(cx.waker().clone());
                unsafe {
                    self.0
                        .interrupt_instance
                        .set_handler_context(mem::transmute(
                            self.0 as *const AsyncBasicTimer<INS, INT>,
                        ));
                }
                self.0.timer_instance.start();
                self.0.run_once = true;
                Poll::Pending
            } else if self.0.expired {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }
    }

    pub struct TimerFuture<'a, INS, INT>(&'a mut AsyncBasicTimer<INS, INT>)
    where
        INS: Basic16bitInstance,
        INT: InterruptExt;

    impl<INS, INT> AsyncBasicTimer<INS, INT>
    where
        INS: Basic16bitInstance,
        INT: InterruptExt,
    {
        unsafe fn handler(arg: *mut ()) {
            let cls: &mut Self = mem::transmute(arg);
            cls.interrupt_instance.unpend();
            cls.expired = true;
            let waker = cls.context.assume_init_read();
            cls.interrupt_instance.unpend();
            cls.timer_instance.stop();
            cls.timer_instance.clear_update_interrupt();
            cls.timer_instance.reset();
            waker.wake();
        }

        pub fn new(mut timer_instance: INS, mut interrupt_instance: INT, frequency: Hertz) -> Self {
            interrupt_instance.enable();
            interrupt_instance.set_handler(Self::handler);
            timer_instance.set_frequency(frequency);
            timer_instance.reset();
            timer_instance.enable_update_interrupt(true);

            Self {
                timer_instance,
                interrupt_instance,
                run_once: false,
                context: unsafe { MaybeUninit::uninit() },
                expired: false,
            }
        }
        #[allow(unused)]
        pub fn duration<'a>(&'a mut self, duration: Duration) -> Option<TimerFuture<'a, INS, INT>> {
            let freq = INS::frequency();
            const ONE_MILLION: u64 = 1_000_000;
            self.expired = false;
            self.run_once = false;
            let ticks: u16 = (duration.as_micros() * freq.0 as u64 / ONE_MILLION)
                .try_into()
                .ok()?;
            unsafe {
                INS::regs().arr().write(|w| w.set_arr(ticks));
            }

            Some(TimerFuture(self))
        }
    }
}
