/* P0-only deterministic layout for Sv39 fixtures: one contiguous LOAD with
 * page-aligned tables inside .text, tohost and signature on their own pages. */
ENTRY(_start)
SECTIONS {
  . = 0x80000000;
  .text : { *(.text*) *(.data*) }
  . = ALIGN(0x1000);
  .tohost : { *(.tohost) }
  . = 0x80004000;
  .signature : { *(.signature) }
  . = 0x80005000;
  .bss (NOLOAD) : { *(.bss*) }
}
