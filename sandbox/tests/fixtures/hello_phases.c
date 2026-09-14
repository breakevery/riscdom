/* Guest that prints two clearly separated phases, so a snapshot can be taken
 * during the long gap and the restored VM can be observed continuing.
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
    puts_uart("PHASE1\n");
    /* Long busy-wait: a snapshot is taken in here. */
    for (volatile unsigned long i = 0; i < 200000000UL; i++) {
    }
    puts_uart("PHASE2\n");
    for (;;) {
    }
}
