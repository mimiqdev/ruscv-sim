/* Stage 4 Sv39 guest linker script.
 *
 * One contiguous LOAD region holds the boot code, the three page tables,
 * the data page, and the translated program page, all page-aligned inside
 * .text.  The HTIF tohost word sits on its own page after the image so it
 * can never alias a page-table entry.  Guests discover every region with
 * `la`; the only fixed constraint is that the whole image stays inside the
 * 1 GiB window at PA 0x80000000.
 */
SECTIONS
{
    . = 0x80000000;
    .text : { *(.text*) *(.data*) *(.rodata*) }
    .signature : { *(.signature) }
    . = ALIGN(0x1000);
    .tohost : { *(.tohost) }
}
