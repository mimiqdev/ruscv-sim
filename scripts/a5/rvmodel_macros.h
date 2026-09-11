/* Adapted from ACT4 a7c9930 config/sail/sail-rv64-max/rvmodel_macros.h.
 * SPDX-License-Identifier: BSD-3-Clause
 * Only HTIF exit is used. No trap, CSR, timer, interrupt or UART promises.
 */
#ifndef _RVMODEL_MACROS_H
#define _RVMODEL_MACROS_H
#define RVMODEL_DATA_SECTION \
  .pushsection .tohost,"aw",@progbits; \
  .balign 8; .global tohost; tohost: .dword 0; \
  .balign 8; .global fromhost; fromhost: .dword 0; \
  .popsection
#define RVMODEL_BOOT
#define RVMODEL_HALT_PASS \
  li x1, 1; la t0, tohost; sd x1, 0(t0); 1: j 1b;
#define RVMODEL_HALT_FAIL \
  li x1, 3; la t0, tohost; sd x1, 0(t0); 1: j 1b;
/* Diagnostics are intentionally suppressed; failure still writes HTIF 3. */
#define RVMODEL_IO_INIT(_R1, _R2, _R3)
#define RVMODEL_IO_WRITE_STR(_R1, _R2, _R3, _STR_PTR)
/* check_defines.h requires these names even for nontrapping I tests.
 * Fail assembly if any test tries to use an unavailable operation. */
#define RVMODEL_INTERRUPT_LATENCY 0
#define RVMODEL_TIMER_INT_SOON_DELAY 0
#define RVMODEL_SET_MEXT_INT(_R1, _R2) .error "No external interrupts";
#define RVMODEL_CLR_MEXT_INT(_R1, _R2) .error "No external interrupts";
#define RVMODEL_SET_MSW_INT(_R1, _R2) .error "No software interrupts";
#define RVMODEL_CLR_MSW_INT(_R1, _R2) .error "No software interrupts";
#define RVMODEL_SET_SEXT_INT(_R1, _R2) .error "No supervisor interrupts";
#define RVMODEL_CLR_SEXT_INT(_R1, _R2) .error "No supervisor interrupts";
#define RVMODEL_SET_SSW_INT(_R1, _R2) .error "No supervisor interrupts";
#define RVMODEL_CLR_SSW_INT(_R1, _R2) .error "No supervisor interrupts";
#endif
