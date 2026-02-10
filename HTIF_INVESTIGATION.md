# HTIF Support in Bare-Metal Spike Execution - Investigation Report

## Summary

After investigating HTIF (Host-Target Interface) support in bare-metal Spike execution, here are the findings:

### 1. Does Spike provide HTIF device for bare-metal programs?

**NO** - Spike does not provide HTIF device by default for bare-metal programs. When a bare-metal program tries to write to the HTIF tohost address (0x40008000), it receives a store segfault:

```
User store segfault @ 0x00000000400008000
```

### 2. What addresses does HTIF use in Spike?

The HTIF device in Spike uses:
- **tohost**: `0x40008000`
- **fromhost**: `0x40008008`

These addresses are defined by the proxy kernel (pk) and the HTIF implementation in Spike's fesvr library.

### 3. Can bare-metal programs write to tohost to exit?

**NO** - Bare-metal programs cannot directly use HTIF to exit in Spike. The HTIF device is only available when running programs through the proxy kernel (pk) or Berkeley Boot Loader (bbl).

When running bare-metal programs directly:
- Spike shows: `warning: tohost and fromhost symbols not in ELF; can't communicate with target`
- Store operations to 0x40008000 result in segfault

### 4. If HTIF is available, what format should bare-metal programs use?

When HTIF is available (via pk), the TOHOST_CMD format is:

```c
// RV64:
#define TOHOST_CMD(dev, cmd, payload) \
  (((uint64_t)(dev) << 56) | ((uint64_t)(cmd) << 48) | (uint64_t)(payload))

// RV32 (limited):
#define TOHOST_CMD(dev, cmd, payload) ({ \
  if ((dev) || (cmd)) __builtin_trap(); \
  (payload); })
```

**Exit command format:**
- dev = 0 (standard device)
- cmd = 1 (exit command)
- payload = exit_code

Example: Exit code 0 = `0x1` (dev=0, cmd=1, payload=0)

**Finisher interface (alternative for RISC-V compliance tests):**
```c
#define FINISHER_PASS  0x5555
#define FINISHER_FAIL  0x3333
```

Write to address `0x40008000` to signal test completion.

## Recommendations for Bare-Metal Programs in Spike

1. **Use pk (proxy kernel)** for HTIF support:
   ```bash
   spike pk <program>
   ```

2. **Define tohost/fromhost symbols** in your ELF:
   ```assembly
   .section .data
   tohost: .dword 0
   fromhost: .dword 0
   ```

3. **For RISC-V compliance tests**, use the finisher interface:
   ```c
   volatile unsigned int *finisher = (unsigned int *)0x40008000;
   *finisher = 0x5555;  // PASS
   ```

4. **Alternative: Use EBREAK** for simulation exit in debug mode:
   ```assembly
   ebreak
   ```

## Spike Memory Map

```
0x00000000 - 0x1000       : Debug region
0x00001000                 : Default reset vector (DEFAULT_RSTVEC)
0x02000000 - 0x02c00000    : CLINT (CLINT_BASE)
0x0c000000 - 0x0d000000    : PLIC (PLIC_BASE)
0x10000000 - 0x10000100    : NS16550 UART (NS16550_BASE)
0x40000000                 : EXT_IO_BASE
0x40008000                : HTIF tohost (when available via pk)
0x80000000                : DRAM_BASE (default memory)
```

## References

- Spike source: https://github.com/riscv-software-src/riscv-isa-sim
- HTIF headers: fesvr/htif.h (in Spike)
- Proxy kernel: riscv-pk (`pk`)
- HTIF spec: RISC-V proxy kernel and Berkeley Bootloader documentation
