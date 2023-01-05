pub mod service {

    use crate::locator::locator::{HardwareLocator, Locator};
    use crate::uart_ip::{AsyncDevice, AsyncHalfDuplexUart, CommunicationState};
    use embassy_net::{ConfigStrategy, Ipv4Address, Ipv4Cidr, Stack, StackResources};
    use embassy_net_driver::Driver;
    use embassy_stm32::peripherals::{DMA2_CH4, USART2};
    use embassy_stm32::usart::UartRx;
    use heapless::Vec;
    use rand_core::RngCore;
    use static_cell::StaticCell;

    macro_rules! singleton {
        ($val:expr) => {{
            type T = impl Sized;
            static STATIC_CELL: StaticCell<T> = StaticCell::new();
            STATIC_CELL.init_with(move || $val)
        }};
    }

    pub trait CoreServiceLocator {
        fn comm_stack_one(&mut self)
            -> Option<(&'static mut Stack<impl Driver>, impl AsyncDevice)>;

        fn comm_stack_two(&mut self)
            -> Option<(&'static mut Stack<impl Driver>, impl AsyncDevice)>;
    }

    impl CoreServiceLocator for HardwareLocator {
        fn comm_stack_one(
            &mut self,
        ) -> Option<(&'static mut Stack<impl Driver>, impl AsyncDevice)> {
            const IP_ADDRESS_ONE: Ipv4Cidr = Ipv4Cidr::new(Ipv4Address::new(192, 168, 69, 3), 24);
            const MAC_ADDRESS_ONE: [u8; 6] = [0, 2, 3, 4, 5, 7];
            let state = singleton!(CommunicationState::new());
            let (runner, device) = embassy_net_driver_channel::new(state, MAC_ADDRESS_ONE);
            static CELL: StaticCell<UartRx<USART2, DMA2_CH4>> = StaticCell::new();
            let y = self.usart2_rx.take().unwrap();
            let usart2_tx = self.tx_channel_one()?;
            let usart2_rx = self.rx_channel_one()?;
            let tim6 = self.timer_channel_one()?;
            let mut rng = self.rng_channel_one()?;
            let mut seed = [0; 8];
            rng.try_fill_bytes(&mut seed).ok()?;
            let seed = u64::from_le_bytes(seed);

            let uart_driver = AsyncHalfDuplexUart::new(usart2_rx, usart2_tx, tim6, runner, rng);
            let config = ConfigStrategy::Static(embassy_net::Config {
                address: IP_ADDRESS_ONE,
                dns_servers: Vec::new(),
                gateway: Some(Ipv4Address::new(192, 168, 69, 1)),
            });

            let stack = singleton!(Stack::new(
                device,
                config,
                singleton!(StackResources::<1, 2, 8>::new()),
                seed
            ));
            Some((stack, uart_driver))
        }

        fn comm_stack_two(
            &mut self,
        ) -> Option<(&'static mut Stack<impl Driver>, impl AsyncDevice)> {
            const IP_ADDRESS_TWO: Ipv4Cidr = Ipv4Cidr::new(Ipv4Address::new(192, 168, 69, 2), 24);
            const MAC_ADDRESS_TWO: [u8; 6] = [0, 2, 3, 4, 5, 6];
            let state = singleton!(CommunicationState::new());
            let (runner, device) = embassy_net_driver_channel::new(state, MAC_ADDRESS_TWO);
            let usart3_tx = self.tx_channel_two()?;
            let usart3_rx = self.rx_channel_two()?;
            let tim7 = self.timer_channel_two()?;
            let mut rng = self.rng_channel_two()?;
            let mut seed = [0; 8];
            rng.try_fill_bytes(&mut seed).ok()?;
            let seed = u64::from_le_bytes(seed);

            let uart_driver = AsyncHalfDuplexUart::new(usart3_rx, usart3_tx, tim7, runner, rng);
            let config = ConfigStrategy::Static(embassy_net::Config {
                address: IP_ADDRESS_TWO,
                dns_servers: Vec::new(),
                gateway: Some(Ipv4Address::new(192, 168, 69, 1)),
            });

            let stack = singleton!(Stack::new(
                device,
                config,
                singleton!(StackResources::<1, 2, 8>::new()),
                seed
            ));
            Some((stack, uart_driver))
        }
    }
}
