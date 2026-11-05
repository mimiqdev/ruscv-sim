# direct-memory U3 (direct-memory D11) session. The caller has already run
# `file`, `target remote`, and forced `riscv set_mem_access sysbus`, so the
# system bus is the debugger's only memory-access method: every transfer
# OpenOCD reports here is a system-bus transfer.
#
# The session does what a debugger's software breakpoint does, one step at a
# time: it reads the word at `s6_break`, writes EBREAK over it, resumes, hits
# it, reads the planted word back, restores the original word, reads that back,
# and then re-executes `s6_break` to the public tohost exit. The run after the
# removal only completes if the fetch returned the restored instruction: a
# leftover EBREAK would trap to mtvec and never reach the exit.
#
# The insert and the removal are ordinary memory writes, so they take the
# forced system-bus path and appear in the OpenOCD log. The read-backs go
# through raw system-bus access instead of GDB's memory layer: GDB substitutes
# its own shadow bytes for an address at which it holds a software breakpoint,
# and a GDB read could not tell the planted EBREAK from the original word.
#
# Lines that start with U3 are the named outputs, following the S6, U4, Z5
# and U6 prefixes of the other sessions.

printf "U3 connect=ok\n"
set $pc = _start

python
import gdb
import re


def raw_sba_word(address):
    """Read one word through raw system-bus access, bypassing GDB's shadow."""

    def send(command):
        gdb.execute("monitor riscv " + command, to_string=True)

    def read(offset):
        text = gdb.execute("monitor riscv dmi_read 0x%x" % offset, to_string=True)
        numbers = re.findall(r"0x[0-9a-fA-F]+|[0-9]+", text)
        if not numbers:
            raise gdb.GdbError("no value in monitor output: %r" % text)
        return int(numbers[-1], 0)

    # sbaccess = 32-bit, sbreadonaddr: the address write triggers the read.
    send("dmi_write 0x38 %d" % ((2 << 17) | (1 << 20)))
    send("dmi_write 0x39 0x%x" % (address & 0xffffffff))
    send("dmi_write 0x3a 0x%x" % (address >> 32))
    for _ in range(50):
        if (read(0x38) & (1 << 21)) == 0:
            break
    else:
        raise gdb.GdbError("the system bus stayed busy for address %#x" % address)
    value = read(0x3c)
    # Leave the DM with word access and no read-on-address.
    send("dmi_write 0x38 %d" % (2 << 17))
    return value


site = int(gdb.parse_and_eval("&s6_break"))
original = raw_sba_word(site)
gdb.set_convenience_variable("u3_original", original)
print("U3 original=%#x" % original)
end

# Plant the software breakpoint through the forced system bus.
set {unsigned int}s6_break = 0x00100073

python
import gdb

site = int(gdb.parse_and_eval("&s6_break"))
print("U3 inserted=%#x" % raw_sba_word(site))
end

continue
printf "U3 break_pc="
output/x $pc
printf "\n"
printf "U3 a2="
output/x $a2
printf "\n"

# Remove it: the original word goes back through the same forced path.
set {unsigned int}s6_break = $u3_original

python
import gdb

site = int(gdb.parse_and_eval("&s6_break"))
print("U3 restored=%#x" % raw_sba_word(site))
end

# Re-execute the restored instruction and run to the public exit.
set $pc = s6_break
printf "U3 resume=ok\n"
continue
printf "U3 continue_returned=unexpected\n"
