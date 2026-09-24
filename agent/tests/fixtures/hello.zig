//! A freestanding Zig fixture for the RISC-V bare-metal target (v0.9 F3a).
//!
//! For a `.zig` source, `compile_freestanding` injects only the linker script, so the
//! source owns `_start` — and it has to be the first thing at the load address, because
//! the `-bios none` guest jumps there instead of to the ELF entry point. That is what
//! `linksection(".text.start")` is for: the generated `link.ld` lists that section first
//! and defines `_stack_top` at the top of the stack.

export fn _start() callconv(.naked) linksection(".text.start") noreturn {
    asm volatile (
        \\ la sp, _stack_top
        \\ call main
        \\ 1: j 1b
    );
}

export fn main() void {
    // UART0 on the QEMU `virt` machine.
    const uart: *volatile u8 = @ptrFromInt(0x1000_0000);
    const message = "hello from zig\n";
    for (message) |c| uart.* = c;
}
