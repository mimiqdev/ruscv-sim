# U4 system-bus session. The caller has already run `file`, `target remote`,
# and forced `riscv set_mem_access sysbus`, so every memory access before the
# switch below has the system bus as its only method. Named outputs start
# with U4.
#
# The writes cover one 32-byte 64-bit transfer, then 4-, 2-, and 1-byte
# transfers of four adjacent distinct values each. A system-bus read of the
# bulk array proves the read path. Every value is read back at its own
# address after switching to abstract Access Memory, so a repeated value, a
# stale value, or a wrong increment fails the check.
#
# The unmapped write must stop at sberror 2 and the next write must succeed
# once OpenOCD clears the sticky error. s0 and s1 are markers: system bus
# access must not touch hart registers.
#
# The raw-DMI block records the sbcs profile and the sberror taxonomy by
# name. Two of those rows are the pre-identified pinned-Spike deviations:
# Spike reports sberror 3 (not the specification's 4) for an unsupported
# sbaccess size, and performs no alignment check, so its misaligned word
# load traps as a bad address (sberror 2).

printf "U4 connect=ok\n"
set $pc = _start
set $s0 = 0x5a5a5a5a5a5a5a5a
set $s1 = 0xa5a5a5a5a5a5a5a5

python
import gdb

inferior = gdb.selected_inferior()
base = int(gdb.parse_and_eval("&s6_pb64"))
values = [0x1111111111111111, 0x2222222222222222, 0x3333333333333333, 0x4444444444444444]
inferior.write_memory(base, b"".join(value.to_bytes(8, "little") for value in values))
print("U4 bulk64=written")
end

set {unsigned int}((char *)&s6_pb32 + 0) = 0x11223344
set {unsigned int}((char *)&s6_pb32 + 4) = 0x55667788
set {unsigned int}((char *)&s6_pb32 + 8) = 0x99aabbcc
set {unsigned int}((char *)&s6_pb32 + 12) = 0xddeeff00
printf "U4 width32=written\n"

set {unsigned short}((char *)&s6_pb16 + 0) = 0x1234
set {unsigned short}((char *)&s6_pb16 + 2) = 0x5678
set {unsigned short}((char *)&s6_pb16 + 4) = 0x9abc
set {unsigned short}((char *)&s6_pb16 + 6) = 0xdef0
printf "U4 width16=written\n"

set {unsigned char}((char *)&s6_pb8 + 0) = 0x5a
set {unsigned char}((char *)&s6_pb8 + 1) = 0xa5
set {unsigned char}((char *)&s6_pb8 + 2) = 0x3c
set {unsigned char}((char *)&s6_pb8 + 3) = 0xc3
printf "U4 width8=written\n"

python
import gdb

inferior = gdb.selected_inferior()
base = int(gdb.parse_and_eval("&s6_pb64"))
value = int.from_bytes(inferior.read_memory(base, 8), "little")
print("U4 sysbus_readback=%#x" % value)
end

# 0x7ffffff0 is outside the image on this profile and on Spike. The write
# must stop at sberror 2 instead of reaching memory.
python
import gdb

try:
    gdb.selected_inferior().write_memory(0x7ffffff0, b"\x01\x02\x03\x04")
    print("U4 fault=accepted")
except gdb.error:
    print("U4 fault=refused")
end

# The same width must transfer once OpenOCD has cleared the sticky error.
set {unsigned int}((char *)&s6_pb32 + 0) = 0x11223344
printf "U4 recovery=written\n"

# The markers were set before the transfers, so GDB holds them in its register
# cache. Drop that cache to read what the target really holds.
maintenance flush register-cache
python
import gdb

mask = (1 << 64) - 1
print("U4 s0=%#x" % (int(gdb.parse_and_eval("$s0")) & mask))
print("U4 s1=%#x" % (int(gdb.parse_and_eval("$s1")) & mask))
end

# Raw DMI now: the sbcs profile and the sberror taxonomy. Each case rewrites
# the full sbcs, triggers through sbaddress0 or sbdata0, and waits for
# sbbusy to drop before naming the sticky sberror. These echo lines must
# stay space-free: GDB's monitor path drops the connection on spaced
# output, so the keys join with underscores.
monitor set sbcs_now [riscv dmi_read 0x38]
monitor echo U4SBCS_PROFILE=[format 0x%08x $sbcs_now]

monitor riscv dmi_write 0x38 [expr {1 << 20 | 2 << 17}]
monitor riscv dmi_write 0x39 0x80000102
monitor for {set sbcs_i 0} {$sbcs_i < 50} {incr sbcs_i} { set sbcs_now [riscv dmi_read 0x38]; if {[expr {$sbcs_now & 0x200000}] == 0} { break } }
monitor echo U4SBERROR_MISALIGNED=[expr {($sbcs_now >> 12) & 7}]

monitor riscv dmi_write 0x38 [expr {7 << 12 | 1 << 20 | 2 << 17}]
monitor riscv dmi_write 0x39 0x7fff0000
monitor for {set sbcs_i 0} {$sbcs_i < 50} {incr sbcs_i} { set sbcs_now [riscv dmi_read 0x38]; if {[expr {$sbcs_now & 0x200000}] == 0} { break } }
monitor echo U4SBERROR_BADADDR=[expr {($sbcs_now >> 12) & 7}]

monitor riscv dmi_write 0x38 [expr {7 << 12 | 4 << 17}]
monitor riscv dmi_write 0x3c 0x1
monitor for {set sbcs_i 0} {$sbcs_i < 50} {incr sbcs_i} { set sbcs_now [riscv dmi_read 0x38]; if {[expr {$sbcs_now & 0x200000}] == 0} { break } }
monitor echo U4SBERROR_UNSUPPORTED=[expr {($sbcs_now >> 12) & 7}]

# Leave the sticky error clear for the sessions that follow.
monitor riscv dmi_write 0x38 [expr {7 << 12}]

# Abstract Access Memory is now the only enabled method. These reads are the
# independent check of what the system bus wrote.
monitor riscv set_mem_access abstract

python
import gdb

inferior = gdb.selected_inferior()

def read_array(name, width, count):
    base = int(gdb.parse_and_eval("&%s" % name))
    for index in range(count):
        address = base + index * width
        value = int.from_bytes(inferior.read_memory(address, width), "little")
        print("U4 %s_%d=%#x@%#x" % (name, index, value, address))

read_array("s6_pb64", 8, 4)
read_array("s6_pb32", 4, 4)
read_array("s6_pb16", 2, 4)
read_array("s6_pb8", 1, 4)
end

# Resume and halt exercise run control after system-bus work.
set $pc = s6_spin
monitor resume
shell sleep 0.3
monitor halt
maintenance flush register-cache
printf "U4 halt_pc="
output/x $pc
printf "\n"

delete breakpoints
monitor reset halt
maintenance flush register-cache
printf "U4 reset_pc="
output/x $pc
printf "\n"

continue
printf "U4 continue_returned=unexpected\n"
