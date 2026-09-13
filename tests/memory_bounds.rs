//! Overflow regressions for storage, bus routing, and the public ELF paths.
#[allow(dead_code)]
mod common;

use common::public_elf as fixture;
use ruscv_sim::executor::{load_and_run, RiscVSimulator, SystemBus};
use ruscv_sim::memory::{MemoryError, MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use std::sync::{Arc, Mutex};

#[test]
fn extended_reads_preserve_bounds_and_values() {
    let mut storage = SimpleMemory::from_data(vec![0xff; 16]);
    let mut mapped = bus(0, 16);
    mapped.write_dword(0, u64::MAX).unwrap();
    for mem in [&mut storage as &mut dyn MemoryInterface, &mut mapped] {
        for addr in [u64::MAX, u64::MAX - 1, u64::MAX - 7, 1 << 32] {
            invalid(mem.read_byte_zext(addr), addr);
            invalid(mem.read_half_zext(addr), addr);
            invalid(mem.read_word_zext(addr), addr);
            invalid(mem.read_byte_sext(addr), addr);
            invalid(mem.read_half_sext(addr), addr);
            invalid(mem.read_word_sext(addr), addr);
        }
        assert_eq!(mem.read_byte_zext(0).unwrap(), 0xff);
        assert_eq!(mem.read_half_zext(0).unwrap(), 0xffff);
        assert_eq!(mem.read_word_zext(0).unwrap(), u64::from(u32::MAX));
        assert_eq!(mem.read_byte_sext(0).unwrap(), u64::MAX);
        assert_eq!(mem.read_half_sext(0).unwrap(), u64::MAX);
        assert_eq!(mem.read_word_sext(0).unwrap(), u64::MAX);
    }
}

#[test]
fn bus_device_fallthrough_and_ram_precedence() {
    const HTIF: u64 = 0x4000_8000;
    const UART: u64 = 0x1000_0000;
    // High RAM must not interfere with the usual low device routes.
    for (base, size) in [(u64::MAX - 7, 16), (0, 0), (HTIF, 4), (HTIF, 8)] {
        let mut mem = bus(base, size);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = calls.clone();
        mem.set_htif_write_callback(move |value| observed.lock().unwrap().push(value));
        mem.write_dword(HTIF, 123).unwrap();
        if base == HTIF && size == 8 {
            assert_eq!(mem.read_dword(HTIF).unwrap(), 123);
            assert!(calls.lock().unwrap().is_empty());
        } else {
            assert_eq!(mem.read_dword(HTIF).unwrap(), 0);
            assert_eq!(*calls.lock().unwrap(), [123]);
        }
        for width in [2, 4, 8] {
            invalid(read(&mem, UART, width), UART);
            invalid(write(&mut mem, UART, width), UART);
        }
        // UART scratch register: verify byte callbacks still reach the device.
        mem.write_byte(UART + 7, 0x5a).unwrap();
        assert_eq!(mem.read_byte(UART + 7).unwrap(), 0x5a);
    }
    let mut mem = bus(UART, 8);
    mem.write_dword(UART, 0x55).unwrap();
    assert_eq!(mem.read_dword(UART).unwrap(), 0x55);
    assert_eq!(mem.read_byte(UART).unwrap(), 0x55);
    let mut mem = bus(0, 0);
    for width in [1, 2, 4] {
        invalid(read(&mem, HTIF, width), HTIF);
        invalid(write(&mut mem, HTIF, width), HTIF);
    }
}

#[test]
fn cli_near_max_load_and_store_exit_cleanly() {
    for store in [false, true] {
        let code = [
            fixture::addi(4, 0, -1),
            if store {
                fixture::sd(5, 4, 0)
            } else {
                fixture::ld(5, 4, 0)
            },
        ];
        let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("near-max.elf");
        std::fs::write(&path, elf).unwrap();
        let output = assert_cmd::cargo::cargo_bin_cmd!()
            .arg("run")
            .arg(path)
            .args(["--max-cycles", "10"])
            .timeout(std::time::Duration::from_secs(10))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.contains("Invalid memory address"), "{text}");
        assert!(!text.contains("panicked"), "{text}");
    }
}

fn read(mem: &dyn MemoryInterface, addr: u64, width: usize) -> Result<u64, MemoryError> {
    match width {
        1 => mem.read_byte(addr).map(u64::from),
        2 => mem.read_half(addr).map(u64::from),
        4 => mem.read_word(addr).map(u64::from),
        8 => mem.read_dword(addr),
        _ => unreachable!(),
    }
}

fn write(mem: &mut dyn MemoryInterface, addr: u64, width: usize) -> Result<(), MemoryError> {
    match width {
        1 => mem.write_byte(addr, 0xff),
        2 => mem.write_half(addr, 0xffff),
        4 => mem.write_word(addr, u32::MAX),
        8 => mem.write_dword(addr, u64::MAX),
        _ => unreachable!(),
    }
}

fn invalid<T: std::fmt::Debug>(result: Result<T, MemoryError>, addr: u64) {
    assert!(
        matches!(result, Err(MemoryError::InvalidAddress(a)) if a == addr),
        "{result:?}"
    );
}

#[test]
fn storage_bounds_all_widths() {
    for size in [0, 1, 2, 3, 4, 7, 8, 16] {
        for width in [1, 2, 4, 8] {
            for addr in [
                u64::MAX,
                u64::MAX - 1,
                u64::MAX - 7,
                1 << 32,
                size as u64,
                size as u64 + 1,
            ] {
                let mut mem = SimpleMemory::from_data(vec![0x5a; size]);
                invalid(read(&mem, addr, width), addr);
                invalid(write(&mut mem, addr, width), addr);
                for offset in 0..size {
                    assert_eq!(mem.read_byte(offset as u64).unwrap(), 0x5a);
                }
            }
        }
    }
}

#[test]
fn storage_edges_and_alignment() {
    for width in [1, 2, 4, 8] {
        let mut mem = SimpleMemory::new(16);
        let last = (16 - width) as u64;
        write(&mut mem, last, width).unwrap();
        assert_ne!(read(&mem, last, width).unwrap(), 0);
        invalid(read(&mem, last + 1, width), last + 1);
        invalid(write(&mut mem, last + 1, width), last + 1);
        if width > 1 {
            assert!(
                matches!(read(&mem, 1, width), Err(MemoryError::Misaligned(1, w)) if w == width as u32)
            );
            assert!(
                matches!(write(&mut mem, 1, width), Err(MemoryError::Misaligned(1, w)) if w == width as u32)
            );
        }
    }
}

fn bus(base: u64, size: usize) -> SystemBus {
    SystemBus::new(
        Arc::new(Mutex::new(SimpleMemory::new(size))),
        Arc::new(Mutex::new(Uart16550::new(0x1000_0000))),
        base,
        size,
    )
}

#[test]
fn bus_invalid_high_addresses() {
    for base in [0, 0x8000_0000] {
        for width in [1, 2, 4, 8] {
            for addr in [u64::MAX, u64::MAX - 1, u64::MAX - 7] {
                let mut mem = bus(base, 16);
                invalid(read(&mem, addr, width), addr);
                invalid(write(&mut mem, addr, width), addr);
                assert_eq!(mem.read_dword(base).unwrap(), 0);
            }
        }
    }
}

#[test]
fn bus_final_address_is_valid_but_accesses_cannot_wrap() {
    // An overflowing nominal region is clipped to the u64 address space.
    // Its accessible prefix remains usable, including byte u64::MAX.
    for size in [8, 16] {
        for width in [1, 2, 4, 8] {
            let mut mem = bus(u64::MAX - 7, size);
            let last = u64::MAX - (width as u64 - 1);
            write(&mut mem, last, width).unwrap();
            assert_ne!(read(&mem, last, width).unwrap(), 0);
            if width > 1 {
                invalid(read(&mem, last + 1, width), last + 1);
                invalid(write(&mut mem, last + 1, width), last + 1);
            }
            invalid(read(&mem, 0, width), 0);
        }
    }
}

#[test]
fn public_flat_below_base_load() {
    public_invalid_access(true, false);
}

#[test]
fn public_flat_below_base_store() {
    public_invalid_access(true, true);
}

#[test]
fn public_bus_near_max_load() {
    public_invalid_access(false, false);
}

#[test]
fn public_bus_near_max_store() {
    public_invalid_access(false, true);
}

fn public_invalid_access(flat: bool, store: bool) {
    let mut code = if flat {
        vec![fixture::auipc(4, 0), fixture::addi(4, 4, -1)]
    } else {
        vec![fixture::addi(4, 0, -1)]
    };
    code.push(if store {
        fixture::sd(5, 4, 0)
    } else {
        fixture::ld(5, 4, 0)
    });
    let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);
    let result = if flat {
        let mut sim = RiscVSimulator::new(0x1_0000);
        sim.load_elf(&elf).unwrap();
        sim.run(Some(10)).unwrap()
    } else {
        load_and_run(&elf, Some(10), None, None, false).unwrap()
    };
    assert!(!result.timed_out);
    let message = result.error.expect("invalid access must stop execution");
    assert!(message.contains("Invalid memory address"), "{message}");
    assert_eq!(result.cycles, if flat { 2 } else { 1 });
    assert_eq!(result.final_pc, fixture::BASE + result.cycles * 4);
}
