//! Small hand-built ELF fixtures for public behavior tests.

use std::cmp::max;

pub const TOHOST_SEGMENT_OFFSET: u64 = 0x1000;
pub const SIGNATURE_SEGMENT_OFFSET: u64 = 0x2000;
pub const BASE: u64 = 0x8000_0000;
pub const TOHOST: u64 = BASE + TOHOST_SEGMENT_OFFSET;
pub const ALTERNATE_TOHOST: u64 = BASE + TOHOST_SEGMENT_OFFSET + 8;
pub const SIGNATURE: u64 = BASE + SIGNATURE_SEGMENT_OFFSET;
pub const SIGNATURE_BYTES: [u8; 8] = [0, 17, 34, 51, 68, 85, 102, 119];
pub const LOAD_OFFSET: usize = 0x1000;
pub const FILE_DATA_SIZE: usize = 0x2008;
pub const BSS_MEMORY_SIZE: usize = 0x18000;
pub const EXPECTED_BSS_MEMORY_LEN: usize = 0x20000;
pub const BSS_PROBE_OFFSET: usize = 0x17fff;
pub const FILE_BYTE_OFFSET: usize = 0x1800;
pub const FILE_BYTE: u8 = 0xa5;

/// Encode an RV64I ADDI instruction.
pub fn addi(rd: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20) | ((rs1 as u32) << 15) | ((rd as u32) << 7) | 0x13
}

/// Encode an RV64I AUIPC instruction.
pub fn auipc(rd: u8, immediate20: u32) -> u32 {
    assert!(immediate20 <= 0x000f_ffff);
    (immediate20 << 12) | ((rd as u32) << 7) | 0x17
}

/// Encode an RV64I LUI instruction.
pub fn lui(rd: u8, immediate20: u32) -> u32 {
    assert!(immediate20 <= 0x000f_ffff);
    (immediate20 << 12) | ((rd as u32) << 7) | 0x37
}

/// Encode an RV64I SLLI instruction.
pub fn slli(rd: u8, rs1: u8, shift: u8) -> u32 {
    assert!(shift < 64);
    ((shift as u32) << 20) | ((rs1 as u32) << 15) | (1 << 12) | ((rd as u32) << 7) | 0x13
}

/// Encode an RV64I OR-immediate instruction.
pub fn ori(rd: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | (6 << 12)
        | ((rd as u32) << 7)
        | 0x13
}

/// Encode an RV64I unsigned-byte load.
pub fn lbu(rd: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | (4 << 12)
        | ((rd as u32) << 7)
        | 0x03
}

/// Encode an RV64I byte store.
pub fn sb(rs2: u8, rs1: u8, immediate: i32) -> u32 {
    encode_store(rs2, rs1, immediate, 0)
}

/// Encode an RV64I double-word store.
pub fn sd(rs2: u8, rs1: u8, immediate: i32) -> u32 {
    encode_store(rs2, rs1, immediate, 3)
}

/// Encode an RV64I double-word load.
pub fn ld(rd: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | (3 << 12)
        | ((rd as u32) << 7)
        | 0x03
}

/// Encode an RV64I no-op.
pub fn nop() -> u32 {
    addi(0, 0, 0)
}

/// Build a standard HTIF exit payload for a guest code.
pub fn standard_exit(code: u32) -> u32 {
    let payload = code
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .expect("fixture exit code must fit in an ADDI immediate");
    addi(5, 0, payload as i32)
}

/// Build a fixture using the alternative high-bit exit encoding.
pub fn alternative_exit(code: u8) -> Vec<u32> {
    vec![
        lui(5, 0x80000),
        slli(5, 5, 32),
        addi(5, 5, i32::from(code)),
        lui(4, 0x40008),
        sd(5, 4, 0),
    ]
}

/// Construct a minimal ELF64/RISC-V executable with one PT_LOAD segment.
///
/// The segment has a file-backed region through offset `0x2008` and a larger
/// zero-filled memory extent. Code is placed at `entry_offset` within the
/// segment. Optional `.tohost` and `.signature` section metadata point into the
/// same loadable image.
pub fn elf_with_code(
    code: &[u32],
    entry_offset: usize,
    with_tohost_section: bool,
    with_signature_section: bool,
    memory_size: usize,
) -> Vec<u8> {
    build_elf(
        code,
        entry_offset,
        BASE,
        with_tohost_section.then_some(TOHOST),
        with_signature_section,
        memory_size,
    )
}

/// Construct a minimal ELF64/RISC-V executable with an explicit base and
/// optional explicit `.tohost` section address (no signature section).
///
/// Used to exercise image placement and tohost correspondence, including bases
/// and declared tohost locations that the flat library configuration cannot
/// represent.
pub fn elf_with_placement(
    code: &[u32],
    entry_offset: usize,
    base: u64,
    tohost_addr: Option<u64>,
    memory_size: usize,
) -> Vec<u8> {
    build_elf(code, entry_offset, base, tohost_addr, false, memory_size)
}

fn build_elf(
    code: &[u32],
    entry_offset: usize,
    base: u64,
    tohost_addr: Option<u64>,
    with_signature_section: bool,
    memory_size: usize,
) -> Vec<u8> {
    let code_size = code
        .len()
        .checked_mul(4)
        .expect("fixture code size overflow");
    let code_end = entry_offset
        .checked_add(code_size)
        .expect("fixture code end overflow");
    let file_data_size = max(FILE_DATA_SIZE, code_end);
    let memory_size = max(memory_size, file_data_size);
    let mut segment = vec![0u8; file_data_size];

    for (index, instruction) in code.iter().enumerate() {
        let offset = entry_offset + index * 4;
        segment[offset..offset + 4].copy_from_slice(&instruction.to_le_bytes());
    }
    segment[FILE_BYTE_OFFSET] = FILE_BYTE;

    if with_signature_section {
        segment[0x2000..0x2008].copy_from_slice(&SIGNATURE_BYTES);
    }

    let mut section_names = vec![0u8];
    let tohost_name = append_string(&mut section_names, ".tohost");
    let signature_name = append_string(&mut section_names, ".signature");
    let shstrtab_name = append_string(&mut section_names, ".shstrtab");

    let mut sections = Vec::new();
    if let Some(address) = tohost_addr {
        sections.push(SectionDefinition {
            name: tohost_name,
            section_type: 1,
            flags: 0x3,
            address,
            offset: LOAD_OFFSET as u64 + TOHOST_SEGMENT_OFFSET,
            size: 8,
            alignment: 8,
        });
    }
    if with_signature_section {
        sections.push(SectionDefinition {
            name: signature_name,
            section_type: 1,
            flags: 0x3,
            address: base + SIGNATURE_SEGMENT_OFFSET,
            offset: LOAD_OFFSET as u64 + SIGNATURE_SEGMENT_OFFSET,
            size: SIGNATURE_BYTES.len() as u64,
            alignment: 8,
        });
    }

    let section_count = 1 + sections.len() + 1;
    let section_table_offset = align_up(LOAD_OFFSET + file_data_size, 8);
    let string_table_offset = section_table_offset + section_count * 64;
    let elf_size = string_table_offset + section_names.len();
    let mut elf = vec![0u8; elf_size];

    elf[0..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2;
    elf[5] = 1;
    elf[6] = 1;
    write_u16(&mut elf, 16, 2);
    write_u16(&mut elf, 18, 243);
    write_u32(&mut elf, 20, 1);
    write_u64(&mut elf, 24, base + entry_offset as u64);
    write_u64(&mut elf, 32, 64);
    write_u64(&mut elf, 40, section_table_offset as u64);
    write_u16(&mut elf, 52, 64);
    write_u16(&mut elf, 54, 56);
    write_u16(&mut elf, 56, 1);
    write_u16(&mut elf, 58, 64);
    write_u16(&mut elf, 60, section_count as u16);
    write_u16(&mut elf, 62, (section_count - 1) as u16);

    let program_header = 64;
    write_u32(&mut elf, program_header, 1);
    write_u32(&mut elf, program_header + 4, 7);
    write_u64(&mut elf, program_header + 8, LOAD_OFFSET as u64);
    write_u64(&mut elf, program_header + 16, base);
    write_u64(&mut elf, program_header + 24, base);
    write_u64(&mut elf, program_header + 32, file_data_size as u64);
    write_u64(&mut elf, program_header + 40, memory_size as u64);
    write_u64(&mut elf, program_header + 48, 0x1000);

    elf[LOAD_OFFSET..LOAD_OFFSET + segment.len()].copy_from_slice(&segment);

    for (index, section) in sections.iter().enumerate() {
        write_section_header(&mut elf, section_table_offset + (index + 1) * 64, section);
    }
    let string_section = SectionDefinition {
        name: shstrtab_name,
        section_type: 3,
        flags: 0,
        address: 0,
        offset: string_table_offset as u64,
        size: section_names.len() as u64,
        alignment: 1,
    };
    write_section_header(
        &mut elf,
        section_table_offset + (section_count - 1) * 64,
        &string_section,
    );
    elf[string_table_offset..].copy_from_slice(&section_names);

    elf
}

fn encode_store(rs2: u8, rs1: u8, immediate: i32, funct3: u32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    let immediate = immediate as u32 & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | ((immediate & 0x1f) << 7)
        | 0x23
}

fn append_string(table: &mut Vec<u8>, value: &str) -> u32 {
    let offset = table.len() as u32;
    table.extend_from_slice(value.as_bytes());
    table.push(0);
    offset
}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn write_section_header(elf: &mut [u8], offset: usize, section: &SectionDefinition) {
    write_u32(elf, offset, section.name);
    write_u32(elf, offset + 4, section.section_type);
    write_u64(elf, offset + 8, section.flags);
    write_u64(elf, offset + 16, section.address);
    write_u64(elf, offset + 24, section.offset);
    write_u64(elf, offset + 32, section.size);
    write_u64(elf, offset + 48, section.alignment);
}

struct SectionDefinition {
    name: u32,
    section_type: u32,
    flags: u64,
    address: u64,
    offset: u64,
    size: u64,
    alignment: u64,
}

fn write_u16(buffer: &mut [u8], offset: usize, value: u16) {
    buffer[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(buffer: &mut [u8], offset: usize, value: u32) {
    buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(buffer: &mut [u8], offset: usize, value: u64) {
    buffer[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
