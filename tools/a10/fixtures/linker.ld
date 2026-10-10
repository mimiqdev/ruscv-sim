/* P0-only deterministic layout, no production profile change. */
ENTRY(_start)
SECTIONS {
  . = 0x80000000;
  .text : { *(.text*) }
  . = 0x80001000;
  .tohost : { *(.tohost) }
  . = 0x80002000;
  .signature : { *(.signature) }
  . = 0x80003000;
  .data : { *(.data*) }
  .bss (NOLOAD) : { *(.bss*) }
}
