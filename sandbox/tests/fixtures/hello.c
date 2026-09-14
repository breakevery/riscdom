/* Minimal RISC-V bare-metal guest used by the sandbox smoke test.
 * Prints "HELLO RISCV" on the NS16550 UART at 0x10000000 and then spins,
 * so the host can capture output and exercise stop().
 */
#define UART0 0x10000000UL
#define STACK_TOP 0x88000000UL

static void puts_uart(const char *s) {
    volatile unsigned char *uart = (volatile unsigned char *)UART0;
    while (*s) {
        *uart = (unsigned char)(*s++);
    }
}

void kern_main(void);

__attribute__((naked, section(".text.start")))
void _start(void) {
    __asm__ volatile("li sp, %0" :: "i"(STACK_TOP));
    __asm__ volatile("call kern_main");
}

void kern_main(void) {
    puts_uart("HELLO RISCV\n");
    for (;;) {
    }
}
