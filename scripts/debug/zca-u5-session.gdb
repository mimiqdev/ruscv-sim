# Zca U5, contract D10: one halted mixed-length rv64imac session. The caller
# has already run `file` and `target remote`. Lines that start with Z5 are the
# named outputs.
#
# The guest fixes its instruction mix, so these stops are exact. The session
# covers a 2-byte software breakpoint (OpenOCD writes C.EBREAK), a 4-byte
# software breakpoint at pc % 4 == 2, and single-steps across 2- and 4-byte
# instructions, including a 4-byte HINT at pc % 4 == 2.
#
# Spike --halted stops in its boot ROM at 0x1000. This profile stops at the
# ELF entry. The guest operations below run from _start on both.
#
# Register trail, fixed by the guest: a0 0 -> 0x1f -> 0x20, a1 0 -> 1 -> 7,
# and s0 takes the 4-byte link value zca_second_link.

printf "Z5 connect=ok\n"
printf "Z5 pc="
output/x $pc
printf "\n"
printf "Z5 gpr="
output/x $a0
printf "\n"
printf "Z5 dcsr="
output/x $dcsr
printf "\n"

# Both targets run the guest from _start. The guest has no debug info, so
# its labels are data addresses: resolve each breakpoint label once through
# an address expression the target-independent path can evaluate.
set $pc = _start
set $zca_bp2 = (unsigned long)&zca_len2
set $zca_bp4 = (unsigned long)&zca_len4

# A 2-byte instruction: the software breakpoint OpenOCD writes is C.EBREAK.
break *$zca_bp2
continue
printf "Z5 break2_pc="
output/x $pc
printf "\n"
printf "Z5 break2_dpc="
output/x $dpc
printf "\n"
printf "Z5 break2_dcsr="
output/x $dcsr
printf "\n"
printf "Z5 break2_a0="
output/x $a0
printf "\n"
printf "Z5 break2_a1="
output/x $a1
printf "\n"

set $a1 = 7
si
printf "Z5 step2_pc="
output/x $pc
printf "\n"
printf "Z5 step2_a0="
output/x $a0
printf "\n"
printf "Z5 step2_a1="
output/x $a1
printf "\n"

# A 4-byte instruction at pc % 4 == 2. Retire the 2-byte breakpoint first;
# then disable compressed breakpoints so the debugger writes the 4-byte
# EBREAK here. The 2-byte stop above already recorded the compressed
# C.EBREAK path a 2-byte instruction needs.
delete breakpoints
set riscv use-compressed-breakpoints off
break *$zca_bp4
continue
printf "Z5 break4_pc="
output/x $pc
printf "\n"
printf "Z5 break4_dpc="
output/x $dpc
printf "\n"
printf "Z5 break4_dcsr="
output/x $dcsr
printf "\n"
printf "Z5 break4_s0="
output/x $s0
printf "\n"

# Step over the 4-byte breakpoint. GDB's RISC-V software single-step plants a
# breakpoint at the next PC (0xe), which is again 2-byte aligned. A 4-byte
# breakpoint there would need a multi-word abstract write, which the pinned
# Spike rejects; a 2-byte C.EBREAK there is a single 2-byte write on both
# targets, so the step runs the same way on both.
set riscv use-compressed-breakpoints on
si
printf "Z5 step4_pc="
output/x $pc
printf "\n"
printf "Z5 step4_s0="
output/x $s0
printf "\n"

# The next 4-byte step is the link that lands on the 4-byte HINT.
si
printf "Z5 link_pc="
output/x $pc
printf "\n"
printf "Z5 link_s0="
output/x $s0
printf "\n"

# HINT-form addi with rd=0 at pc % 4 == 2: an ordinary retired step with no
# architectural effect.
printf "Z5 hint_before_a1="
output/x $a1
printf "\n"
si
printf "Z5 hint_pc="
output/x $pc
printf "\n"
printf "Z5 hint_after_a1="
output/x $a1
printf "\n"

# Park the hart so the same-debugger halt reads dpc, then let the reset image
# run to the tohost exit with no breakpoint left.
set $pc = &zca_spin
monitor resume
shell sleep 0.3
monitor halt
maintenance flush register-cache
printf "Z5 halt_pc="
output/x $pc
printf "\n"
printf "Z5 halt_dpc="
output/x $dpc
printf "\n"

delete breakpoints
monitor reset halt
maintenance flush register-cache
printf "Z5 reset_pc="
output/x $pc
printf "\n"

continue
printf "Z5 continue_returned=unexpected\n"
