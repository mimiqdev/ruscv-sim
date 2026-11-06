# quantum U3 (quantum D6/D8/D10) session, under `run --transport tlm
# --quantum 1024` with `riscv set_mem_access sysbus` forced, so every memory
# transfer below is a system-bus transfer and appears in the OpenOCD log.
#
# The caller has already run `file`, `target remote` and the sysbus switch.
# Lines that start with Q3q are the named outputs.
#
# The session does what a live attach does on a decoupled run: it reads
# `mtime` at the initial halt, resumes to the guest's armed compare, halts
# inside the spin, reads `mtime` again through the system bus, resumes through
# the timer trap, and reads the lateness the guest recorded. Every halt is F4's
# forced sync, so committed time equals local time at each read.
#
# D6: the debugger's F4 syncs, including the ones a DMI poll causes, move the
# phase of the threshold syncs. One exact lateness therefore belongs only to a
# run whose sync points the guest and `Q` determine. This session compares the
# guest's lateness with the D8 bound alone and never with one fixed value;
# `assert_quantum_session` in scripts/debug_attach.sh holds that rule.

printf "Q3q connect=ok\n"
set $pc = _start

printf "Q3q mtime_halted="
output/x *(unsigned long long *)0x0200bff8
printf "\n"

break *q3_spin
continue
printf "Q3q halted=ok\n"

printf "Q3q mtime_break="
output/x *(unsigned long long *)0x0200bff8
printf "\n"
printf "Q3q arm="
output/x *(unsigned long long *)&q3_arm_time
printf "\n"
printf "Q3q compare="
output/x *(unsigned long long *)&q3_compare
printf "\n"

delete breakpoints
break *q3_report
continue

printf "Q3q mcause="
output/x *(unsigned long long *)&q3_mcause
printf "\n"
printf "Q3q mepc="
output/x *(unsigned long long *)&q3_mepc
printf "\n"
printf "Q3q traps="
output/x *(unsigned long long *)&q3_traps
printf "\n"
printf "Q3q trap_time="
output/x *(unsigned long long *)&q3_trap_time
printf "\n"
printf "Q3q lateness="
output/x *(unsigned long long *)&q3_lateness
printf "\n"

delete breakpoints
printf "Q3q resume=ok\n"
continue
printf "Q3q continue_returned=unexpected\n"
