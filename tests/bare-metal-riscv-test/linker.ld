SECTIONS
{
    . = 0x80000000;
    .text : { *(.text*) }
    .data : { *(.data*) }
    .rodata : { *(.rodata*) }
    . = 0x80001000;
    .tohost : { *(.tohost) }
}
