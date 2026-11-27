#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

#[cfg(feature = "axstd")]
#[cfg_attr(feature = "axstd", no_mangle)]
fn main() {
    use axstd::os::arceos::modules::axlog::println_with_color;

    println_with_color("Green", "Hello, Arceos!");
}
