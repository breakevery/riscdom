/* Fixture: a Hello World the way the AI is expected to write it (main only). */
#define UART0 0x10000000UL

static void puts_uart(const char *s) {
    volatile unsigned char *uart = (volatile unsigned char *)UART0;
    while (*s) {
        *uart = (unsigned char)(*s++);
    }
}

int main(void) {
    puts_uart("HELLO RISCV\n");
    for (;;) {
    }
}
