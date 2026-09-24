//! A freestanding Rust fixture for the RISC-V bare-metal target (v0.9 F3b-1).
//!
//! `compile_freestanding` injects only the linker script for a `.rs` source, so the source
//! owns `_start` — and it has to be the first thing at the load address, because the
//! `-bios none` guest jumps there instead of to the ELF entry point. That is what the
//! `#[link_section = ".text.start"]` below is for: `link.ld` lists that section first and
//! defines `_stack_top` at the top of the stack. A `no_std` program also needs its own
//! `#[panic_handler]`.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

/// UART0 on the QEMU `virt` machine.
const UART: *mut u8 = 0x1000_0000 as *mut u8;

#[no_mangle]
#[link_section = ".text.start"]
pub extern "C" fn _start() -> ! {
    unsafe {
        core::arch::asm!(
            "la sp, _stack_top",
            "call main",
            "1: j 1b",
            options(noreturn)
        );
    }
}

#[no_mangle]
pub extern "C" fn main() {
    let message = b"hello from rust\n";
    for byte in message {
        unsafe { core::ptr::write_volatile(UART, *byte) };
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
