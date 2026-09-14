/* Guest that prints in two separated bursts, so the host sees at least two
 * serial frames. Used by the serial-observer test.
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
    puts_uart("HELLO ");
    /* Busy-wait so the host has time to read the first frame separately. */
    for (volatile unsigned long i = 0; i < 40000000UL; i++) {
    }
    puts_uart("RISCV\n");
    for (;;) {
    }
}
