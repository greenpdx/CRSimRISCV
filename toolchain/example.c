/*
 * Example program for 8-register RV32I CPU.
 *
 * Compile:
 *   riscv32-unknown-elf-gcc $(RV8_CFLAGS) -c crt0-8reg.S -o crt0.o
 *   riscv32-unknown-elf-gcc $(RV8_CFLAGS) -c example.c -o example.o
 *   riscv32-unknown-elf-ld $(RV8_LDFLAGS) crt0.o example.o -o example.elf
 *   riscv32-unknown-elf-objcopy -O binary example.elf example.bin
 */

/* I/O LED peripheral at 0x40020000 */
#define LED_ADDR ((volatile unsigned int *)0x40020000)

void main(void)
{
    int sum = 0;           /* will live in x3 (t0) or x6 (s0) */

    for (int i = 1; i <= 10; i++) {
        sum += i;
    }

    /* Write result to LEDs */
    *LED_ADDR = sum;       /* should show 55 = 0x37 */

    /* Halt */
    while (1) {}
}
