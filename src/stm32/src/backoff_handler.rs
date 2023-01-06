pub mod backoff {
    use rand_core::impls;
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
