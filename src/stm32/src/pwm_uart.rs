pub mod pwm_uart {
    use core::cmp::min;
    use core::pin::Pin;
    use core::sync::atomic::{AtomicBool, Ordering};
    use defmt::debug;

    use crate::half_duplex::uart::{new, HalfDuplexUartRx, HalfDuplexUartTx};
    use crate::stm32_uart::serial::{BasicUartRx, BasicUartTx};
    use communication::{Read, ReadError, Write, WriteError};
    use core::mem;
    use defmt::info;
    use embassy_futures::select::{select, Either};
    use embassy_stm32::pac::timer::regs::CcmrOutput;
    use embassy_stm32::pac::timer::vals::{CcmrInputCcs, Icf, Ocm, Opm, Sms, Ts};
    use embassy_stm32::pwm::simple_pwm::{Ch1, Ch2, PwmPin, SimplePwm};
    use embassy_stm32::time::Hertz;
    use embassy_stm32::usart::BasicInstance;
    use embassy_stm32::{self};
    use embassy_stm32::{
        pwm::{CaptureCompare16bitInstance, Channel, OutputCompareMode},
        timer::GeneralPurpose16bitInstance,
        Peripheral,
    };

    pub struct PwmOutputTimer<T: CaptureCompare16bitInstance> {
        tim: T,
    }
    #[derive(Debug)]
    pub enum PwmError {
        DividerTooLarge,
        DividerIsZero,
        Other,
    }
    impl<T> PwmOutputTimer<T>
    where
        T: CaptureCompare16bitInstance,
    {
        pub fn try_new<'d>(
            mut tim: T,
            output_pin: PwmPin<'d, T, Ch1>,
            input_pin: PwmPin<'d, T, Ch2>,
            uart_frequency: Hertz,
            pulse_width_divider: u8,
        ) -> Result<Self, PwmError> {
            T::enable();
            tim.reset();
            tim.set_frequency(uart_frequency);
            tim.start();

            unsafe {
                tim.enable_outputs(true);

                let max_arr = tim.get_max_compare_value();
                if pulse_width_divider == 0 {
                    return Err(PwmError::DividerIsZero);
                } else if pulse_width_divider as u16 > max_arr {
                    return Err(PwmError::DividerTooLarge);
                }
                tim.set_compare_value(Channel::Ch1, max_arr / pulse_width_divider as u16)
            }

            let regs = T::regs_gp16();
            let mut this = Self { tim };

            unsafe {
                this.setup_input_trigger();
                this.setup_output()
            }

            unsafe {
                // TODO set output compare mode
            }

            Ok(this)
        }
        unsafe fn setup_input_trigger(&mut self) {
            // page 1201 of stm32l5 reference manual -> configure channel to detect falling edge
            //Select the active input: TIMx_CCR1 must be linked to the TI1 input, so write the CC1S
            // bits to 01 in the TIMx_CCMR1 register

            let regs = T::regs_gp16();
            regs.ccmr_input(0)
                .modify(|ccmr| ccmr.set_ccs(1, CcmrInputCcs(1)));
            // Program the appropriate input filter duration in relation with the signal connected to the
            // timer
            regs.ccmr_input(0)
                .modify(|ccmr| ccmr.set_icf(1, Icf::NOFILTER));

            //seelct only the negative transition of uart (uart is usually inverted :) )
            regs.ccer().modify(|ccer| {
                ccer.set_ccp(1, true);
                ccer.set_ccnp(0, false)
            });

            // enable capture from the counter into the capture register  by setting the CC1E bit in the
            //TIMx_CCER register.
            self.tim.enable_channel(Channel::Ch2, true);
            // regs.ccer().modify(|ccer| ccer.set_cce(0, true));

            // /2. Configure the timer in reset mode by writing SMS=100 in TIMx_SMCR register. Select
            // TI2 as the input source by writing TS=110 in TIMx_SMCR register

            regs.smcr().modify(|smcr| {
                smcr.set_ts(Ts::TI2FP2);
                smcr.set_sms(Sms::RESET_MODE)
            });
        }

        unsafe fn setup_output(&mut self) {
            self.tim
                .set_output_compare_mode(Channel::Ch1, OutputCompareMode::PwmMode1);
            self.tim.enable_channel(Channel::Ch1, true);
        }
    }

    // tim3 for transmission timer
    // tx timer input: A7
    // tx timer output: A6
    mod sealed {

        use super::*;

        pub trait RetriggerableOPMTimer: CaptureCompare16bitInstance {
            unsafe fn enable_combined_reset_and_trigger() {
                let regs = Self::regs_gp16();
                const SMS_POS: u32 = 16;

                regs.smcr().modify(|smcr| {
                    smcr.0 |= (1 << SMS_POS);
                });
            }
            unsafe fn set_retriggerable_opm_mode(channel: Channel, mode: RetriggerableOpmMode) {
                let r = Self::regs_gp16();
                let raw_channel: usize = channel.raw();
                r.ccmr_output(raw_channel / 2)
                    .modify(|w| set_ocm_retriggerable(w, raw_channel % 2, mode));
            }
        }
    }
    unsafe fn set_ocm_retriggerable(ccmro: &mut CcmrOutput, n: usize, mode: RetriggerableOpmMode) {
        let ocm_bit = 16 + n * 8usize;
        let offs = 4usize + n * 8usize;
        ccmro.0 =
            (ccmro.0 & !(0b111 << offs)) | (((mode.raw() as u32) & 0b111) << offs) | (1 << ocm_bit);
    }

    #[derive(Clone, Copy)]
    pub enum RetriggerableOpmMode {
        Mode1,
        Mode2,
    }

    impl RetriggerableOpmMode {
        pub fn raw(&self) -> u32 {
            match self {
                RetriggerableOpmMode::Mode1 => 0b1000,
                RetriggerableOpmMode::Mode2 => 0b1001,
            }
        }
    }

    pub trait RetriggerableOPMTimer: sealed::RetriggerableOPMTimer {}

    pub struct PwmInputModulationTimer<T: CaptureCompare16bitInstance> {
        tim: T,
    }
    // tim15 for rx timer
    //A3 for input // ch2
    //A2 for output // ch1
    impl<T> PwmInputModulationTimer<T>
    where
        T: CaptureCompare16bitInstance,
    {
        pub fn try_new<'d>(
            mut tim: T,
            output_pin: PwmPin<'d, T, Ch1>,
            input_pin: PwmPin<'d, T, Ch2>,
            uart_frequency: Hertz,
            pulse_width_divider: u8,
        ) -> Result<Self, PwmError> {
            T::enable();
            tim.reset();
            tim.set_frequency(uart_frequency);
            tim.start();

            unsafe {
                tim.enable_outputs(true);

                let max_arr = tim.get_max_compare_value();
                if pulse_width_divider == 0 {
                    return Err(PwmError::DividerIsZero);
                } else if pulse_width_divider as u16 > max_arr {
                    return Err(PwmError::DividerTooLarge);
                }
                tim.set_compare_value(Channel::Ch1, max_arr / pulse_width_divider as u16)
            }

            let regs = T::regs_gp16();
            let mut this = Self { tim };

            unsafe {
                this.setup_input_trigger();
                this.setup_output()
            }

            unsafe {
                // TODO set output compare mode
            }

            Ok(this)
        }
        unsafe fn setup_input_trigger(&mut self) {
            // page 1201 of stm32l5 reference manual -> configure channel to detect falling edge
            //Select the active input: TIMx_CCR1 must be linked to the TI1 input, so write the CC1S
            // bits to 01 in the TIMx_CCMR1 register

            let regs = T::regs_gp16();
            regs.ccmr_input(0)
                .modify(|ccmr| ccmr.set_ccs(1, CcmrInputCcs(1)));
            // Program the appropriate input filter duration in relation with the signal connected to the
            // timer
            regs.ccmr_input(0)
                .modify(|ccmr| ccmr.set_icf(1, Icf::NOFILTER));

            //seelct only the negative transition of uart (uart is usually inverted :) )
            regs.ccer().modify(|ccer| {
                ccer.set_ccp(0, false);
                ccer.set_ccnp(0, false)
            });

            // enable capture from the counter into the capture register  by setting the CC1E bit in the
            //TIMx_CCER register.
            self.tim.enable_channel(Channel::Ch2, true);
            // regs.ccer().modify(|ccer| ccer.set_cce(0, true));

            // /2. Configure the timer in reset mode by writing SMS=100 in TIMx_SMCR register. Select
            // TI2 as the input source by writing TS=110 in TIMx_SMCR register

            regs.smcr().modify(|smcr| {
                smcr.set_ts(Ts::TI2FP2);
            });
            const SMS_POS: u32 = 16;

            regs.smcr().modify(|smcr| {
                smcr.0 |= 1 << SMS_POS;
            });
        }

        unsafe fn setup_output(&mut self) {
            // self.tim
            //     .set_output_compare_mode(Channel::Ch1, OutputCompareMode::PwmMode2);

            let mut regs = T::regs_gp16();
            // regs.ccmr_output(0)
            //     .modify(|ccmr| ccmr.set_ocm(0, Ocm(0b1001)));
            regs.ccr(0).modify(|ccr1| ccr1.set_ccr(0));
            regs.cr1().modify(|cr1| cr1.set_opm(Opm::ENABLED));

            let raw_channel: usize = Channel::Ch1.raw();
            regs.ccmr_output(raw_channel / 2)
                .modify(|w| set_ocm_retriggerable(w, raw_channel % 2, RetriggerableOpmMode::Mode2));

            self.tim.enable_channel(Channel::Ch1, true);
            regs.egr().write(|egr| egr.set_ug(true));
        }
    }
}
