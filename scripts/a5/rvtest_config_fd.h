/* ACT4 requires this header separately from rvmodel_macros.h.
 *
 * The F/D and Zcd selections declare the floating-point capability the run
 * profile actually has, so ACT4's RVTEST_BEGIN enables `mstatus.FS` before any
 * F, D or Zcd instruction executes (contract D6) and derives `CONFIG_FLEN`
 * from FLEN=64 instead of FLEN=0. The RV64I and Zca selections keep the
 * unchanged header without these defines, so their runs still declare no
 * floating-point support.
 *
 * No privileged or further optional extension defines are enabled: no
 * PMP entries, no S-mode, no misaligned-success environment.
 */
#define RVMODEL_PMP_GRAIN 0
#define RVMODEL_NUM_PMPS 0
#define F_SUPPORTED
#define D_SUPPORTED
