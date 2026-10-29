# U4 program-buffer session. The caller has already run `file`, `target
# remote`, and forced `riscv set_mem_access progbuf`, so every memory access
# before the switch below has the program buffer as its only method. Named
# outputs start with U4.
#
# The writes cover one four-word 64-bit transfer (one program, three address
# increments), then 4-, 2-, and 1-byte transfers of four adjacent distinct
# values each. Every value is read back at its own address after switching to
# abstract Access Memory, so a repeated value, a stale value, or a wrong
# increment fails the check.
#
# The unmapped write exercises an exception inside the program buffer and the
# recovery after OpenOCD clears `cmderr`. s0 and s1 are markers: the buffer
# programs use both registers, and OpenOCD must restore them.

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

# 0x7ffffff0 is outside the image on this profile and on Spike. The store must
# stop the program with cmderr=3 instead of reaching the next word.
python
import gdb

try:
    gdb.selected_inferior().write_memory(0x7ffffff0, b"\x01\x02\x03\x04")
    print("U4 fault=accepted")
except gdb.error:
    print("U4 fault=refused")
end

# The same transfer must work once OpenOCD has cleared the sticky error.
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

# Abstract Access Memory is now the only enabled method. These reads are the
# independent check of what the program buffer wrote.
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

# Resume and halt exercise run control after program-buffer work, including
# OpenOCD's fence program on the resume path.
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
