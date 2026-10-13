"""RV64 effect model for three public-v1 witnesses, not a general Stage 4 model.

The model reads linked opcode words and applies RV64I plus the profile WARL
rules in this file. It does not call ruscv-sim and it does not copy a trace.
`s4_tvm` is the TVM illegal-instruction witness, `s4_ssip` the supervisor
software-interrupt witness, and `s4_csr_fields` the CSR field-policy witness.
Only the opcodes those three programs execute are implemented. The only TVM
path is an S-mode CSR access to satp. SFENCE.VMA, TSR, and TW are not modeled.
An opcode outside that set raises ValueError.
"""
BASE = 0x80000000
MASK = (1 << 64) - 1
MISA = 0x8000000000141101
XLEN = 0xA00000000
MPP = 0b11 << 11
WRITABLE_MSTATUS = (
    (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 8) | MPP
    | (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 21) | (1 << 22)
)
SUPERVISOR_INTERRUPTS = 0x222
SATP_ASID = 0xFFFF << 44

MSTATUS, MISA_ADDR, MIE, MTVEC = 0x300, 0x301, 0x304, 0x305
MSCRATCH, MEPC, MCAUSE, MTVAL, MIP = 0x340, 0x341, 0x342, 0x343, 0x344
SATP, MINSTRET = 0x180, 0xB02

def sext(value, bits):
    value &= (1 << bits) - 1
    if value >> (bits - 1):
        value -= 1 << bits
    return value

def load(memory, addr, width):
    value = 0
    for index in range(width):
        value |= memory.get(addr + index, 0) << (8 * index)
    return value

def store(memory, addr, value, width):
    for index in range(width):
        memory[addr + index] = (value >> (8 * index)) & 0xFF

def legal_mstatus(old, value):
    nxt = value & WRITABLE_MSTATUS
    if ((value & MPP) >> 11) == 2:
        nxt = (nxt & ~MPP) | (old & MPP)
    return nxt | XLEN

def legal_mtvec(old, value):
    mode = value & 0b11
    if mode > 1:
        mode = old & 0b11
    return (value & ~0b11) | mode

def legal_satp(old, value):
    if (value >> 60) in (0, 8):
        return value & ~SATP_ASID
    return old

class Model:
    def __init__(self, syms):
        self.csr = {
            MSTATUS: XLEN, MISA_ADDR: MISA, MIE: 0, MTVEC: 0, MSCRATCH: 0,
            MEPC: 0, MCAUSE: 0, MTVAL: 0, MIP: 0, SATP: 0,
        }
        self.privilege = 3
        self.memory = {}
        store(self.memory, syms['neighbors'], 0x123, 8)
        self.retired = 0
        self.attempts = 0
        self.traps = 0
        self.trap_at = []

    def read_csr(self, addr):
        if addr == MINSTRET:
            return self.retired
        if addr not in self.csr:
            raise ValueError(f'unmodeled CSR {addr:#x}')
        return self.csr[addr]

    def write_csr(self, addr, value):
        old = self.read_csr(addr)
        if addr == MSTATUS:
            value = legal_mstatus(old, value)
        elif addr == MISA_ADDR:
            value = MISA
        elif addr in (MIE, MIP):
            value = value & SUPERVISOR_INTERRUPTS
        elif addr == MEPC:
            value = value & ~0b11
        elif addr == MTVEC:
            value = legal_mtvec(old, value)
        elif addr == SATP:
            value = legal_satp(old, value)
        elif addr == MINSTRET:
            raise ValueError('these witnesses do not write minstret')
        self.csr[addr] = value & MASK

    def interrupt_cause(self):
        enabled = self.privilege != 3 or self.csr[MSTATUS] & (1 << 3)
        if not enabled:
            return None
        pending = self.csr[MIP] & self.csr[MIE] & SUPERVISOR_INTERRUPTS
        if pending & (1 << 9):
            return (1 << 63) | 9
        if pending & (1 << 1):
            return (1 << 63) | 1
        if pending & (1 << 5):
            return (1 << 63) | 5
        return None

    def enter(self, pc, cause, tval, attempted):
        if attempted:
            self.attempts += 1
        self.traps += 1
        self.trap_at.append(self.retired)
        mie = (self.csr[MSTATUS] >> 3) & 1
        new_mstatus = (self.csr[MSTATUS] & ~0x1888) | (mie << 7) | (self.privilege << 11)
        self.write_csr(MEPC, pc)
        self.write_csr(MCAUSE, cause)
        self.write_csr(MTVAL, tval)
        self.write_csr(MSTATUS, new_mstatus)
        self.privilege = 3
        base = self.csr[MTVEC] & ~0b11
        mode = self.csr[MTVEC] & 0b11
        # Vectored mtvec adds 4*cause for interrupts only. Exceptions use BASE.
        if mode == 1 and cause >> 63:
            return base + (cause & ((1 << 63) - 1)) * 4
        return base

def branch_imm(word):
    imm = ((word >> 31) & 1) << 12
    imm |= ((word >> 7) & 1) << 11
    imm |= ((word >> 25) & 0x3F) << 5
    imm |= ((word >> 8) & 0xF) << 1
    return sext(imm, 13)

def jal_imm(word):
    imm = ((word >> 31) & 1) << 20
    imm |= ((word >> 12) & 0xFF) << 12
    imm |= ((word >> 20) & 1) << 11
    imm |= ((word >> 21) & 0x3FF) << 1
    return sext(imm, 21)

def apply(name, opcodes, syms, emit, mem, regs):
    """Retire `name` into `trace` through `emit`. Return its oracle counts."""
    model = Model(syms)
    pc = BASE
    fail = syms['fail']
    tohost = syms['tohost']
    steps = 0
    while steps < 10000:
        steps += 1
        if pc == fail:
            raise ValueError(f'{name} reached fail at {pc:#x}')
        cause = model.interrupt_cause()
        if cause is not None:
            pc = model.enter(pc, cause, 0, attempted=False)
            continue
        offset = pc - BASE
        if offset not in opcodes:
            raise ValueError(f'{name} left the text image at {pc:#x}')
        word = opcodes[offset]
        if model.privilege == 1 and (word & 0x7F) == 0x73 and ((word >> 20) & 0xFFF) == SATP and model.csr[MSTATUS] & (1 << 20):
            pc = model.enter(pc, 2, word, attempted=True)
            continue
        model.attempts += 1
        writes, effect, next_pc = retire(name, model, regs, pc, word, mem)
        # `emit` records the pre-write register and then updates it.
        emit(offset, [(index, value) for index, value in writes if index != 0], effect, next_pc=next_pc - BASE)
        model.retired += 1
        if any(item['address'] == tohost and item['write'] is not None for item in effect):
            break
        pc = next_pc
    else:
        raise ValueError(f'{name} did not reach tohost')
    signature_addr = syms['signature']
    signature = [model.memory.get(signature_addr + index, 0) for index in range(24)]
    def dword(at):
        return load(model.memory, signature_addr + at, 8)
    if dword(16) != model.retired - 6:
        raise ValueError(f'{name} counter witness {dword(16)} != retirements-6 {model.retired - 6}')
    ram = []
    for symbol, length in (('neighbors', 8), ('scratch', 16)):
        addr = syms[symbol]
        ram.append(dict(offset=addr - BASE, bytes=[model.memory.get(addr + index, 0) for index in range(length)]))
    return dict(
        attempts=model.attempts, turns=model.retired + model.traps, retirements=model.retired,
        traps=model.traps, trap_at=model.trap_at, work=dword(0), checksum=dword(8),
        signature_addr=signature_addr, signature=signature, uart=[], events=[], ram=ram,
    )

def retire(name, model, regs, pc, word, mem):
    rd = (word >> 7) & 31
    funct3 = (word >> 12) & 7
    rs1 = (word >> 15) & 31
    rs2 = (word >> 20) & 31
    opcode = word & 0x7F
    writes, effect = [], []
    next_pc = (pc + 4) & MASK
    if opcode == 0x13:
        imm = sext(word >> 20, 12)
        shamt = (word >> 20) & 0x3F
        value = regs[rs1]
        if funct3 == 0:
            value = (value + imm) & MASK
        elif funct3 == 1:
            value = (value << shamt) & MASK
        elif funct3 == 4:
            value = value ^ (imm & MASK)
        elif funct3 == 5:
            if ((word >> 26) & 0x3F) == 0x10:
                value = (sext(value, 64) >> shamt) & MASK
            else:
                value = value >> shamt
        elif funct3 == 6:
            value = value | (imm & MASK)
        elif funct3 == 7:
            value = value & (imm & MASK)
        else:
            raise ValueError(f'{name} OP-IMM funct3 {funct3} at {pc:#x}')
        writes = [(rd, value)]
    elif opcode == 0x1B and funct3 == 0:
        imm = sext(word >> 20, 12)
        writes = [(rd, sext((regs[rs1] + imm) & 0xFFFFFFFF, 32) & MASK)]
    elif opcode == 0x17:
        writes = [(rd, (pc + sext(word & 0xFFFFF000, 32)) & MASK)]
    elif opcode == 0x37:
        writes = [(rd, sext(word & 0xFFFFF000, 32) & MASK)]
    elif opcode == 0x03 and funct3 == 3:
        addr = (regs[rs1] + sext(word >> 20, 12)) & MASK
        value = load(model.memory, addr, 8)
        writes = [(rd, value)]
        effect = [mem(addr, 8, read=value)]
    elif opcode == 0x23 and funct3 == 3:
        imm = sext(((word >> 25) << 5) | ((word >> 7) & 31), 12)
        addr = (regs[rs1] + imm) & MASK
        value = regs[rs2]
        store(model.memory, addr, value, 8)
        effect = [mem(addr, 8, write=value)]
    elif opcode == 0x63:
        taken = {
            0: regs[rs1] == regs[rs2],
            1: regs[rs1] != regs[rs2],
            4: sext(regs[rs1], 64) < sext(regs[rs2], 64),
            5: sext(regs[rs1], 64) >= sext(regs[rs2], 64),
            6: regs[rs1] < regs[rs2],
            7: regs[rs1] >= regs[rs2],
        }.get(funct3)
        if taken is None:
            raise ValueError(f'{name} branch funct3 {funct3} at {pc:#x}')
        if taken:
            next_pc = (pc + branch_imm(word)) & MASK
    elif opcode == 0x6F:
        if rd != 0:
            writes = [(rd, (pc + 4) & MASK)]
        next_pc = (pc + jal_imm(word)) & MASK
    elif opcode == 0x73 and funct3 == 0 and word == 0x30200073:
        mstatus = model.csr[MSTATUS]
        mpie = (mstatus >> 7) & 1
        mpp = (mstatus >> 11) & 0b11
        model.privilege = {0: 0, 1: 1, 2: 3, 3: 3}[mpp]
        new_mstatus = ((mstatus & ~(1 << 3)) | (mpie << 3) | (1 << 7)) & ~MPP
        if model.privilege != 3:
            new_mstatus &= ~(1 << 17)
        model.write_csr(MSTATUS, new_mstatus)
        next_pc = model.csr[MEPC] & ~0b11
    elif opcode == 0x73 and funct3 in (1, 2, 3):
        csr = (word >> 20) & 0xFFF
        old = model.read_csr(csr)
        source = regs[rs1]
        perform = funct3 == 1 or rs1 != 0
        if perform:
            if funct3 == 1:
                written = source
            elif funct3 == 2:
                written = old | source
            else:
                written = old & ~source
            model.write_csr(csr, written)
        if rd != 0:
            writes = [(rd, old)]
    else:
        raise ValueError(f'{name} unsupported opcode {opcode:#x} word {word:#x} at {pc:#x}')
    return writes, effect, next_pc
