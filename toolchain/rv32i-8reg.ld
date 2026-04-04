/* ─────────────────────────────────────────────────────────────────────────────
 * Linker script for 8-register RV32I educational CPU
 *
 * Memory map:
 *   ROM  0x00000000 – 0x0000007F   (32 words = 128 bytes)
 *   RAM  0x00010000 – 0x0001007F   (32 words = 128 bytes)
 *   I/O  0x40020000                (LED peripheral)
 * ───────────────────────────────────────────────────────────────────────────── */

ENTRY(_start)

MEMORY
{
  ROM (rx)  : ORIGIN = 0x00000000, LENGTH = 128
  RAM (rwx) : ORIGIN = 0x00010000, LENGTH = 128
}

SECTIONS
{
  .text : {
    *(.text.init)
    *(.text .text.*)
  } > ROM

  .rodata : {
    *(.rodata .rodata.*)
  } > ROM

  .data : {
    *(.data .data.*)
    *(.sdata .sdata.*)
  } > RAM AT > ROM

  .bss (NOLOAD) : {
    *(.bss .bss.*)
    *(.sbss .sbss.*)
  } > RAM

  /* Stack at top of RAM */
  _stack_top = ORIGIN(RAM) + LENGTH(RAM);
}
