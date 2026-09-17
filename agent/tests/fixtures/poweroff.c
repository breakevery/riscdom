/* Fixture: a guest that prints its banner and then powers the machine off.
 *
 * The SiFive test/finisher device of the QEMU `virt` machine lives at 0x100000:
 * a 32-bit write of 0x5555 asks QEMU to shut down, so the QEMU **process** exits
 * while the host still holds its handle. That is exactly the state a "VM 运行中"
 * badge must not report as running (v0.3.1 #3).
 */
#define UART0 0x10000000UL
#define TEST_DEV 0x100000UL
#define FINISHER_PASS 0x5555u

static void puts_uart(const char *s) {
    volatile unsigned char *uart = (volatile unsigned char *)UART0;
    while (*s) {
        *uart = (unsigned char)(*s++);
    }
}

int main(void) {
    puts_uart("BYE RISCV\n");
    *(volatile unsigned int *)TEST_DEV = FINISHER_PASS;
    for (;;) {
    }
}
