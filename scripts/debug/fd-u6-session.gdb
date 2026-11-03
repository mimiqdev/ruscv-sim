# F/D U6, contract D12: one halted rv64imafdc session. The caller has already
# run `file` and `target remote`. Lines that start with U6 are the named
# outputs.
#
# The session writes f0-f31 and fcsr through DM abstract access while
# mstatus.FS is Off, then reads every value back and continues. The guest
# verifies that FS is still Off, sets FS, checks the written values, and
# parks on three FP instructions in turn. The session breaks on each,
# single-steps it, and reads mstatus.FS before and after.
#
# Spike --halted stops in its boot ROM at 0x1000. This profile stops at the
# ELF entry. The guest operations below run from _start on both targets.
#
# GDB names FPRs by ABI name: f0-f7 are ft0-ft7, f8-f9 are fs0-fs1, f10-f17
# are fa0-fa7, f18-f27 are fs2-fs11, and f28-f31 are ft8-ft11.

printf "U6 connect=ok\n"
printf "U6 pc="
output/x $pc
printf "\n"
printf "U6 mstatus="
output/x $mstatus
printf "\n"
printf "U6 fcsr_init="
output/x $fcsr
printf "\n"
printf "U6 f0_init="
output/x $ft0.double
printf "\n"

# f_i = i + 0.5. Every value is exact in binary64, so the guest can compare
# the raw bits against its table.
set $ft0.double = 0.5
set $ft1.double = 1.5
set $ft2.double = 2.5
set $ft3.double = 3.5
set $ft4.double = 4.5
set $ft5.double = 5.5
set $ft6.double = 6.5
set $ft7.double = 7.5
set $fs0.double = 8.5
set $fs1.double = 9.5
set $fa0.double = 10.5
set $fa1.double = 11.5
set $fa2.double = 12.5
set $fa3.double = 13.5
set $fa4.double = 14.5
set $fa5.double = 15.5
set $fa6.double = 16.5
set $fa7.double = 17.5
set $fs2.double = 18.5
set $fs3.double = 19.5
set $fs4.double = 20.5
set $fs5.double = 21.5
set $fs6.double = 22.5
set $fs7.double = 23.5
set $fs8.double = 24.5
set $fs9.double = 25.5
set $fs10.double = 26.5
set $fs11.double = 27.5
set $ft8.double = 28.5
set $ft9.double = 29.5
set $ft10.double = 30.5
set $ft11.double = 31.5
set $fcsr = 0x7b

# Read every FPR back through the debugger before the guest runs. OpenOCD
# caches FPR values after a write, so a GDB-side flush alone would return that
# cache. `monitor halt` invalidates OpenOCD's register cache even when the
# hart is already halted; the flush then makes GDB request each value.
monitor halt
maintenance flush register-cache
printf "U6 f0="
output/x $ft0.double
printf "\n"
printf "U6 f1="
output/x $ft1.double
printf "\n"
printf "U6 f2="
output/x $ft2.double
printf "\n"
printf "U6 f3="
output/x $ft3.double
printf "\n"
printf "U6 f4="
output/x $ft4.double
printf "\n"
printf "U6 f5="
output/x $ft5.double
printf "\n"
printf "U6 f6="
output/x $ft6.double
printf "\n"
printf "U6 f7="
output/x $ft7.double
printf "\n"
printf "U6 f8="
output/x $fs0.double
printf "\n"
printf "U6 f9="
output/x $fs1.double
printf "\n"
printf "U6 f10="
output/x $fa0.double
printf "\n"
printf "U6 f11="
output/x $fa1.double
printf "\n"
printf "U6 f12="
output/x $fa2.double
printf "\n"
printf "U6 f13="
output/x $fa3.double
printf "\n"
printf "U6 f14="
output/x $fa4.double
printf "\n"
printf "U6 f15="
output/x $fa5.double
printf "\n"
printf "U6 f16="
output/x $fa6.double
printf "\n"
printf "U6 f17="
output/x $fa7.double
printf "\n"
printf "U6 f18="
output/x $fs2.double
printf "\n"
printf "U6 f19="
output/x $fs3.double
printf "\n"
printf "U6 f20="
output/x $fs4.double
printf "\n"
printf "U6 f21="
output/x $fs5.double
printf "\n"
printf "U6 f22="
output/x $fs6.double
printf "\n"
printf "U6 f23="
output/x $fs7.double
printf "\n"
printf "U6 f24="
output/x $fs8.double
printf "\n"
printf "U6 f25="
output/x $fs9.double
printf "\n"
printf "U6 f26="
output/x $fs10.double
printf "\n"
printf "U6 f27="
output/x $fs11.double
printf "\n"
printf "U6 f28="
output/x $ft8.double
printf "\n"
printf "U6 f29="
output/x $ft9.double
printf "\n"
printf "U6 f30="
output/x $ft10.double
printf "\n"
printf "U6 f31="
output/x $ft11.double
printf "\n"

# The GDB view after the abstract FP writes. The guest independently checks
# that FS is still Off when it runs.
printf "U6 fcsr="
output/x $fcsr
printf "\n"
printf "U6 mstatus_writes="
output/x $mstatus
printf "\n"

# Both targets run the guest from _start. The marker tells the guest that
# the debugger wrote the FP state. The three FP stops are 4-byte aligned and
# 4-byte long, so a 4-byte breakpoint and a 4-byte step are exact on both
# targets.
set {unsigned int}&fd_u6_marker = 0x55433644
set $pc = _start
set riscv use-compressed-breakpoints off
break *fd_u6_bp_load
break *fd_u6_bp_add
break *fd_u6_bp_store

continue
printf "U6 load_pc="
output/x $pc
printf "\n"
printf "U6 load_mstatus="
output/x $mstatus
printf "\n"
printf "U6 load_fcsr="
output/x $fcsr
printf "\n"

# Fresh reads of the first and last FPR at the stop, where the register
# cache is invalidated by the guest execution.
printf "U6 load_f0="
output/x $ft0.double
printf "\n"
printf "U6 load_f31="
output/x $ft11.double
printf "\n"

si
printf "U6 load_step_pc="
output/x $pc
printf "\n"
printf "U6 load_step_mstatus="
output/x $mstatus
printf "\n"
printf "U6 load_f8="
output/x $fs0.double
printf "\n"

continue
printf "U6 add_pc="
output/x $pc
printf "\n"
printf "U6 add_mstatus="
output/x $mstatus
printf "\n"
si
printf "U6 add_step_pc="
output/x $pc
printf "\n"
printf "U6 add_step_mstatus="
output/x $mstatus
printf "\n"
printf "U6 add_f10="
output/x $fa0.double
printf "\n"

continue
printf "U6 store_pc="
output/x $pc
printf "\n"
printf "U6 store_mstatus="
output/x $mstatus
printf "\n"
si
printf "U6 store_step_pc="
output/x $pc
printf "\n"
printf "U6 store_step_mstatus="
output/x $mstatus
printf "\n"
printf "U6 store_word="
output/x {unsigned int}((char *)&fd_u6_vals + 12)
printf "\n"
printf "U6 fcsr_end="
output/x $fcsr
printf "\n"

# The guest's remaining checks pass, it clears the marker and exits through
# tohost. GDB's continue does not return once the target is gone.
delete breakpoints
printf "U6 continue_sent=ok\n"
continue
printf "U6 continue_returned=unexpected\n"
