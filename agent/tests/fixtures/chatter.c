/* Fixture: a guest that never goes quiet.
 *
 * The UART is written in a loop with only a short spin between lines, so the
 * `read_serial` quiet window (~150 ms) never closes. A guest like this is the
 * regression case for v0.3.1 #2: when the overall wait elapsed, the tool used to
 * answer "No serial output yet" even though the buffer was full.
 */
#define UART0 0x10000000UL

static void spin(unsigned long n) {
    volatile unsigned long i;
    for (i = 0; i < n; i++) {
    }
}

static void puts_uart(const char *s) {
    volatile unsigned char *uart = (volatile unsigned char *)UART0;
    while (*s) {
        *uart = (unsigned char)(*s++);
    }
}

int main(void) {
    for (;;) {
        puts_uart("CHATTER\n");
        spin(500000UL);
    }
}
