SECTIONS
{
    . = 0x80000000;
    .text : { *(.text*) }
    .data : { *(.data*) }
    .rodata : { *(.rodata*) }
    . = 0x80002000;
    .tohost : { *(.tohost) }
}
