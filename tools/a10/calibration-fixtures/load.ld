/* calibration-workloads/1; existing native/flat RAM placement only. */
ENTRY(_start)
SECTIONS {
 . = 0x80000000; .text : { *(.text*) }
 . = 0x80001000; .tohost : { *(.tohost) }
 . = 0x80002000; .signature : { *(.signature) }
 . = 0x80003000; .data : { *(.data*) }
 .work_bss (NOLOAD) : { *(.work_bss) }
 . = 0x80004000; .payload : { FILL(0xa5a5a5a5); BYTE(0xa5); . += 131071; }
 .payload_bss (NOLOAD) : { . += 8192; }
}
