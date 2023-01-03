pub mod timer {
    // use embassy_stm32::pac::TIM15;
    use defmt::*;
    use embassy_cortex_m::interrupt::Priority;

    use embassy_stm32::rcc::low_level::RccPeripheral;
    use embassy_stm32::time::Hertz;
    use embassy_stm32::timer::Basic16bitInstance;

    use core::future::Future;

    use core::mem::{self};
    use core::sync::atomic::AtomicBool;
    use core::sync::atomic::Ordering;
    use core::task::Poll;

    use embassy_stm32::interrupt::InterruptExt;
    use embassy_time::Duration;

    pub trait AsyncTimer {
        type AsyncOutput<'a>: Future<Output = ()> + 'a
        where
            Self: 'a;
        fn duration<'a>(&'a mut self, duration: Duration) -> Option<Self::AsyncOutput<'a>>;
        fn get_handle<'a>(&'a mut self) -> Option<Self::AsyncOutput<'a>>;
    }

    impl<INS, INT> AsyncTimer for AsyncBasicTimer<INS, INT>
    where
        INS: Basic16bitInstance + 'static,
        INT: InterruptExt + 'static,
    {
        type AsyncOutput<'a> = TimerFuture<'a, INS, INT>;
        fn duration<'a>(&'a mut self, duration: Duration) -> Option<Self::AsyncOutput<'a>> {
            AsyncBasicTimer::duration(self, duration)
        }
        fn get_handle<'a>(&'a mut self) -> Option<Self::AsyncOutput<'a>> {
            AsyncBasicTimer::get_handle(self)
        }
    }

    pub struct AsyncBasicTimer<INS, INT>
    where
        INS: Basic16bitInstance,
        INT: InterruptExt,
    {
        timer_instance: INS,
        interrupt_instance: INT,
        run_once: AtomicBool,
        expired: AtomicBool,
        initialized: AtomicBool,
        context: Option<core::task::Waker>,
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
            if false == self.0.run_once.load(Ordering::Relaxed) {
                self.0.context = Some(cx.waker().clone());
                unsafe {
                    // self.0
                    //     .interrupt_instance
                    //     .set_handler_context(mem::transmute(
                    //         self.0 as *const AsyncBasicTimer<INS, INT>,
                    //     ));
                }
                self.0.timer_instance.start();
                self.0.run_once.store(true, Ordering::Relaxed);
                Poll::Pending
            } else if self.0.expired.load(Ordering::Relaxed) {
                self.0.initialized.store(false, Ordering::Relaxed);
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

    impl<'a, INS, INT> Unpin for TimerFuture<'a, INS, INT>
    where
        INS: Basic16bitInstance,
        INT: InterruptExt,
    {
    }

    // pub struct PersistentTimerFuture<'a, INS, INT>(&'a mut AsyncBasicTimer<INS, INT>)
    // where
    //     INS: Basic16bitInstance,
    //     INT: InterruptExt;

    impl<INS, INT> AsyncBasicTimer<INS, INT>
    where
        INS: Basic16bitInstance,
        INT: InterruptExt,
    {
        //safety: this runs in interrupt context and single threaded
        unsafe fn handler(arg: *mut ()) {
            let cls: &mut Self = mem::transmute(arg);
            cls.interrupt_instance.unpend();
            cls.expired.store(true, Ordering::Relaxed);
            let waker = &mut cls.context;
            cls.interrupt_instance.unpend();
            cls.timer_instance.stop();
            cls.timer_instance.clear_update_interrupt();
            cls.timer_instance.reset();
            if let Some(waker) = waker {
                waker.wake_by_ref();
            }
        }

        fn prescaler() -> u16 {
            unsafe { INS::regs().psc().read().psc() + 1 }
        }

        pub fn new(mut timer_instance: INS, mut interrupt_instance: INT, frequency: Hertz) -> Self {
            <INS as RccPeripheral>::enable();
            <INS as RccPeripheral>::reset();
            interrupt_instance.set_handler(Self::handler);
            interrupt_instance.set_priority(Priority::P0);
            interrupt_instance.enable();
            timer_instance.set_frequency(frequency);
            timer_instance.reset();
            timer_instance.enable_update_interrupt(true);

            Self {
                timer_instance,
                interrupt_instance,
                run_once: AtomicBool::new(false),
                context: None,
                expired: AtomicBool::new(false),
                initialized: AtomicBool::new(false),
            }
        }
        #[allow(unused)]
        pub fn duration<'a>(&'a mut self, duration: Duration) -> Option<TimerFuture<'a, INS, INT>> {
            self.initialized.store(true, Ordering::Relaxed);
            self.expired.store(false, Ordering::Relaxed);
            self.run_once = AtomicBool::new(false);
            self.timer_instance.reset();
            let ticks = Self::to_ticks(duration)?;
            unsafe {
                INS::regs().arr().write(|w| w.set_arr(ticks));
                self.interrupt_instance
                    .set_handler_context(mem::transmute(self as *const Self))
            }

            Some(TimerFuture(self))
        }

        pub fn get_handle<'a>(&'a mut self) -> Option<TimerFuture<'a, INS, INT>> {
            if self.initialized.load(Ordering::Relaxed) {
                return Some(TimerFuture(self));
            }
            None
        }

        fn to_ticks(duration: Duration) -> Option<u16> {
            let freq: u64 = (INS::frequency().0 / Self::prescaler() as u32)
                .try_into()
                .ok()?;
            const ONE_MILLION: u64 = 1_000_000;
            let __ticks = (duration.as_micros() * freq / ONE_MILLION);

            let ticks: Option<u16> = (duration.as_micros() * freq / ONE_MILLION).try_into().ok();
            return ticks;
        }
    }
}
