## how to build
change to nightly `rustup default nightly`
add rust-std for target `rustup target add thumbv8m.main-none-eabihf`
build standard library `cd src/stm32 && cargo build -Zbuild-std`
install probe-run
run in stm32 folder: `cargo build`
to flash: `cargo run -- --monitor` in stm32 folder
to run