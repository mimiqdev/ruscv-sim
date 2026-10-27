# §7 item 5, one halted session. The caller has already run `file` and
# `target remote`. Lines that start with S6 are the named outputs.
# The fence warning is an OpenOCD message, not a line from here.

printf "S6 connect=ok\n"
printf "S6 pc="
output/x $pc
printf "\n"
printf "S6 gpr="
output/x $a0
printf "\n"
printf "S6 mstatus="
output/x $mstatus
printf "\n"
printf "S6 dcsr="
output/x $dcsr
printf "\n"

# Spike --halted stops in its boot ROM at 0x1000. This profile stops at
# the ELF entry. The guest operations below run from _start on both.
set $pc = _start

si
printf "S6 step_pc="
output/x $pc
printf "\n"
printf "S6 step_gpr="
output/x $a0
printf "\n"

break *s6_break
continue
printf "S6 break_pc="
output/x $pc
printf "\n"

set $a2 = 0x1234
si
printf "S6 observe_gpr="
output/x $a2
printf "\n"

set {unsigned long long}&s6_scratch = 0x1122334455667788
printf "S6 ram="
output/x {unsigned long long}&s6_scratch
printf "\n"

set $pc = s6_spin
monitor resume
shell sleep 0.3
monitor halt
maintenance flush register-cache
printf "S6 halt_pc="
output/x $pc
printf "\n"

delete breakpoints
monitor reset halt
maintenance flush register-cache
printf "S6 reset_pc="
output/x $pc
printf "\n"

continue
printf "S6 continue_returned=unexpected\n"
