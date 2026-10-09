/* calibration-workloads/1; existing native/flat RAM placement only. */
ENTRY(_start)
SECTIONS {
 . = 0x80000000; .text : { *(.text*) }
 . = 0x80001000; .tohost : { *(.tohost) }
 . = 0x80002000; .signature : { *(.signature) }
 . = 0x80003000; .data : { *(.data*) }
 .work_bss (NOLOAD) : { *(.work_bss) }
}
