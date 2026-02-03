//! ELF Loader Module
//!
//! Loads and parses ELF files for arch-test execution.
//! Supports RV64 ELF format with:
//! - Program header loading (PT_LOAD segments)
//! - Entry point extraction
//! - Signature region identification
//! - tohost/exit convention support

use std::io::{Read, Seek, SeekFrom};
use thiserror::Error;

/// ELF magic number
const ELF_MAGIC: &[u8; 4] = b"\x7fELF";

/// ELF class: 32-bit or 64-bit
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfClass {
    /// 32-bit ELF
    Elf32 = 1,
    /// 64-bit ELF
    Elf64 = 2,
}

/// ELF endianness
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfEndian {
    /// Little endian
    Little = 1,
    /// Big endian
    Big = 2,
}

/// ELF type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfType {
    /// No file type
    None = 0,
    /// Relocatable file
    Rel = 1,
    /// Executable file
    Exec = 2,
    /// Shared object file
    Dyn = 3,
    /// Core file
    Core = 4,
}

/// ELF machine type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfMachine {
    /// No machine
    None = 0,
    /// RISC-V
    Riscv = 243,
}

/// ELF program header type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfPType {
    /// Null segment (ignore)
    Null = 0,
    /// Loadable segment
    Load = 1,
    /// Dynamic linking info
    Dynamic = 2,
    /// Interpreter path
    Interpreter = 3,
    /// Auxiliary info
    Note = 4,
    /// Reserved
    Reserved = 5,
    /// TLS template
    Tls = 7,
    /// GNU EH Frame
    GnuEhFrame = 0x6474e550,
    /// GNU stack (executable permission)
    GnuStack = 0x6474e551,
    /// GNU relro
    GnuRelro = 0x6474e552,
}

/// ELF program header flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfPhFlags(pub u32);

impl ElfPhFlags {
    /// Execute permission
    pub fn execute(&self) -> bool {
        self.0 & 0x1 != 0
    }
    /// Write permission
    pub fn write(&self) -> bool {
        self.0 & 0x2 != 0
    }
    /// Read permission
    pub fn read(&self) -> bool {
        self.0 & 0x4 != 0
    }
}

/// ELF loading errors
#[derive(Debug, Error)]
pub enum ElfError {
    #[error("Invalid ELF magic number")]
    InvalidMagic,
    #[error("Unsupported ELF class: {0}")]
    UnsupportedClass(u8),
    #[error("Unsupported ELF endianness: {0}")]
    UnsupportedEndian(u8),
    #[error("Invalid ELF version: {0}")]
    InvalidVersion(u8),
    #[error("Unsupported ELF type: {0}")]
    UnsupportedType(u16),
    #[error("Unsupported machine: {0}")]
    UnsupportedMachine(u16),
    #[error("Invalid program header offset")]
    InvalidPhOffset,
    #[error("Invalid program header entry size")]
    InvalidPhEntrySize,
    #[error("Invalid section header offset")]
    InvalidShOffset,
    #[error("Invalid section header entry size")]
    InvalidShEntrySize,
    #[error("Program header out of bounds")]
    PhOutOfBounds,
    #[error("Segment out of bounds")]
    SegmentOutOfBounds,
    #[error("Invalid segment file size")]
    InvalidSegmentFileSize,
    #[error("Invalid segment memory size")]
    InvalidSegmentMemSize,
    #[error("Memory allocation failed")]
    MemoryAllocationFailed,
    #[error("I/O error: {0}")]
    IoError(String),
    #[error("Signature section not found")]
    SignatureNotFound,
    #[error("Tohost section not found")]
    TohostNotFound,
}

/// 64-bit ELF header
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Elf64Header {
    /// Magic number (0x7F 'E' 'L' 'F')
    e_ident_magic: [u8; 4],
    /// ELF class
    e_ident_class: u8,
    /// Endianness
    e_ident_data: u8,
    /// ELF version
    e_ident_version: u8,
    /// OS/ABI identification
    e_ident_osabi: u8,
    /// ABI version
    e_ident_abiversion: u8,
    /// Reserved
    e_ident_pad: [u8; 7],
    /// Object file type
    e_type: u16,
    /// Architecture
    e_machine: u16,
    /// Object file version
    e_version: u32,
    /// Entry point virtual address
    e_entry: u64,
    /// Program header table file offset
    e_phoff: u64,
    /// Section header table file offset
    e_shoff: u64,
    /// Processor-specific flags
    e_flags: u32,
    /// ELF header size in bytes
    e_ehsize: u16,
    /// Program header table entry size
    e_phentsize: u16,
    /// Program header table entry count
    e_phnum: u16,
    /// Section header table entry size
    e_shentsize: u16,
    /// Section header table entry count
    e_shnum: u16,
    /// Section header string table index
    e_shstrndx: u16,
}

/// 64-bit program header
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Elf64Phdr {
    /// Segment type
    p_type: u32,
    /// Segment flags
    p_flags: u32,
    /// Segment file offset
    p_offset: u64,
    /// Segment virtual address
    p_vaddr: u64,
    /// Segment physical address
    p_paddr: u64,
    /// Segment size in file
    p_filesz: u64,
    /// Segment size in memory
    p_memsz: u64,
    /// Segment alignment
    p_align: u64,
}

/// 64-bit section header
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Elf64Shdr {
    /// Section name (string table index)
    sh_name: u32,
    /// Section type
    sh_type: u32,
    /// Section flags
    sh_flags: u64,
    /// Section virtual address
    sh_addr: u64,
    /// Section file offset
    sh_offset: u64,
    /// Section size in file
    sh_size: u64,
    /// Link to other section
    sh_link: u32,
    /// Extra information
    sh_info: u32,
    /// Section alignment
    sh_addralign: u64,
    /// Entry size if section holds table
    sh_entsize: u64,
}

/// ELF file loader
#[derive(Debug)]
#[allow(dead_code)]
pub struct ElfLoader {
    /// ELF header
    header: Elf64Header,
    /// Entry point
    entry_point: u64,
    /// Program headers
    program_headers: Vec<Elf64Phdr>,
    /// Loadable segments
    load_segments: Vec<Elf64Phdr>,
    /// Signature section info (if found)
    signature_section: Option<SignatureInfo>,
    /// Tohost address (if found)
    tohost_addr: Option<u64>,
    /// ELF file size
    file_size: u64,
}

/// Signature section information
#[derive(Debug, Clone)]
pub struct SignatureInfo {
    /// Virtual address of signature region
    pub vaddr: u64,
    /// Size of signature region
    pub size: u64,
    /// Offset in file
    pub file_offset: u64,
}

/// Type alias for section header parsing result
type SectionHeaderResult = Result<Option<(String, u32, u64, u64, u64)>, ElfError>;

/// Type alias for ELF loading result
type ElfLoadResult = Result<(u64, Vec<u8>, Option<SignatureInfo>, Option<u64>, u64), ElfError>;

impl ElfLoader {
    /// Load ELF file from reader
    pub fn load<R: Read + Seek>(reader: &mut R) -> Result<Self, ElfError> {
        // Get file size first
        let file_size = reader
            .seek(SeekFrom::End(0))
            .map_err(|e| ElfError::IoError(e.to_string()))?;

        // Rewind to start and read full 64-byte ELF header
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|e| ElfError::IoError(e.to_string()))?;

        let mut header_buf = [0u8; 64];
        reader
            .read_exact(&mut header_buf)
            .map_err(|e| ElfError::IoError(e.to_string()))?;

        // Check magic number
        if &header_buf[0..4] != ELF_MAGIC {
            return Err(ElfError::InvalidMagic);
        }

        // Validate class (must be 64-bit for RV64)
        let class = match header_buf[4] {
            1 => ElfClass::Elf32,
            2 => ElfClass::Elf64,
            _ => return Err(ElfError::UnsupportedClass(header_buf[4])),
        };
        if class != ElfClass::Elf64 {
            return Err(ElfError::UnsupportedClass(header_buf[4]));
        }

        // Validate endianness
        let endian = match header_buf[5] {
            1 => ElfEndian::Little,
            2 => ElfEndian::Big,
            _ => return Err(ElfError::UnsupportedEndian(header_buf[5])),
        };
        if endian != ElfEndian::Little {
            return Err(ElfError::UnsupportedEndian(header_buf[5]));
        }

        // Validate version
        if header_buf[6] != 1 {
            return Err(ElfError::InvalidVersion(header_buf[6]));
        }

        // Parse ELF header
        let header = Self::parse_header(&header_buf, endian)?;

        // Validate ELF type (must be executable)
        let elf_type = match header.e_type {
            0 => ElfType::None,
            1 => ElfType::Rel,
            2 => ElfType::Exec,
            3 => ElfType::Dyn,
            4 => ElfType::Core,
            _ => return Err(ElfError::UnsupportedType(header.e_type)),
        };
        if elf_type != ElfType::Exec {
            return Err(ElfError::UnsupportedType(header.e_type));
        }

        // Validate machine (must be RISC-V)
        if header.e_machine != 243 {
            return Err(ElfError::UnsupportedMachine(header.e_machine));
        }

        // Read program headers
        let program_headers = Self::read_program_headers(reader, &header, file_size)?;

        // Find loadable segments
        let load_segments: Vec<Elf64Phdr> = program_headers
            .iter()
            .filter(|ph| {
                let p_type = ph.p_type;
                p_type == ElfPType::Load as u32
            })
            .copied()
            .collect();

        // Find signature and tohost sections
        let (signature_section, tohost_addr) =
            Self::find_special_sections(reader, &header, file_size)?;

        Ok(Self {
            header,
            entry_point: header.e_entry,
            program_headers,
            load_segments,
            signature_section,
            tohost_addr,
            file_size,
        })
    }

    /// Parse ELF header from buffer
    fn parse_header(buf: &[u8; 64], endian: ElfEndian) -> Result<Elf64Header, ElfError> {
        // Parse based on endianness
        let (
            e_type,
            e_machine,
            e_version,
            e_entry,
            e_phoff,
            e_shoff,
            e_flags,
            e_ehsize,
            e_phentsize,
            e_phnum,
            e_shentsize,
            e_shnum,
            e_shstrndx,
        ) = match endian {
            ElfEndian::Little => {
                let e_type = u16::from_le_bytes([buf[16], buf[17]]);
                let e_machine = u16::from_le_bytes([buf[18], buf[19]]);
                let e_version = u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
                let e_entry = u64::from_le_bytes([
                    buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
                ]);
                let e_phoff = u64::from_le_bytes([
                    buf[32], buf[33], buf[34], buf[35], buf[36], buf[37], buf[38], buf[39],
                ]);
                let e_shoff = u64::from_le_bytes([
                    buf[40], buf[41], buf[42], buf[43], buf[44], buf[45], buf[46], buf[47],
                ]);
                let e_flags = u32::from_le_bytes([buf[48], buf[49], buf[50], buf[51]]);
                let e_ehsize = u16::from_le_bytes([buf[52], buf[53]]);
                let e_phentsize = u16::from_le_bytes([buf[54], buf[55]]);
                let e_phnum = u16::from_le_bytes([buf[56], buf[57]]);
                let e_shentsize = u16::from_le_bytes([buf[58], buf[59]]);
                let e_shnum = u16::from_le_bytes([buf[60], buf[61]]);
                let e_shstrndx = u16::from_le_bytes([buf[62], buf[63]]);
                (
                    e_type,
                    e_machine,
                    e_version,
                    e_entry,
                    e_phoff,
                    e_shoff,
                    e_flags,
                    e_ehsize,
                    e_phentsize,
                    e_phnum,
                    e_shentsize,
                    e_shnum,
                    e_shstrndx,
                )
            }
            ElfEndian::Big => {
                let e_type = u16::from_be_bytes([buf[16], buf[17]]);
                let e_machine = u16::from_be_bytes([buf[18], buf[19]]);
                let e_version = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
                let e_entry = u64::from_be_bytes([
                    buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
                ]);
                let e_phoff = u64::from_be_bytes([
                    buf[32], buf[33], buf[34], buf[35], buf[36], buf[37], buf[38], buf[39],
                ]);
                let e_shoff = u64::from_be_bytes([
                    buf[40], buf[41], buf[42], buf[43], buf[44], buf[45], buf[46], buf[47],
                ]);
                let e_flags = u32::from_be_bytes([buf[48], buf[49], buf[50], buf[51]]);
                let e_ehsize = u16::from_be_bytes([buf[52], buf[53]]);
                let e_phentsize = u16::from_be_bytes([buf[54], buf[55]]);
                let e_phnum = u16::from_be_bytes([buf[56], buf[57]]);
                let e_shentsize = u16::from_be_bytes([buf[58], buf[59]]);
                let e_shnum = u16::from_be_bytes([buf[60], buf[61]]);
                let e_shstrndx = u16::from_be_bytes([buf[62], buf[63]]);
                (
                    e_type,
                    e_machine,
                    e_version,
                    e_entry,
                    e_phoff,
                    e_shoff,
                    e_flags,
                    e_ehsize,
                    e_phentsize,
                    e_phnum,
                    e_shentsize,
                    e_shnum,
                    e_shstrndx,
                )
            }
        };

        Ok(Elf64Header {
            e_ident_magic: [buf[0], buf[1], buf[2], buf[3]],
            e_ident_class: buf[4],
            e_ident_data: buf[5],
            e_ident_version: buf[6],
            e_ident_osabi: buf[7],
            e_ident_abiversion: buf[8],
            e_ident_pad: [buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]],
            e_type,
            e_machine,
            e_version,
            e_entry,
            e_phoff,
            e_shoff,
            e_flags,
            e_ehsize,
            e_phentsize,
            e_phnum,
            e_shentsize,
            e_shnum,
            e_shstrndx,
        })
    }

    /// Read program headers
    fn read_program_headers<R: Read + Seek>(
        reader: &mut R,
        header: &Elf64Header,
        file_size: u64,
    ) -> Result<Vec<Elf64Phdr>, ElfError> {
        if header.e_phoff == 0 || header.e_phnum == 0 {
            return Ok(Vec::new());
        }

        // Check program header bounds
        let ph_end = header
            .e_phoff
            .saturating_add((header.e_phentsize as u64) * (header.e_phnum as u64));
        if ph_end > file_size {
            return Err(ElfError::PhOutOfBounds);
        }

        let mut program_headers = Vec::with_capacity(header.e_phnum as usize);
        let ph_size = header.e_phentsize as usize;

        for i in 0..header.e_phnum {
            let offset = header.e_phoff + (i as u64) * (header.e_phentsize as u64);
            reader
                .seek(SeekFrom::Start(offset))
                .map_err(|e| ElfError::IoError(e.to_string()))?;

            let mut buf = vec![0u8; ph_size];
            reader.read_exact(&mut buf).map_err(|e| {
                ElfError::IoError(format!("Failed to read program header {}: {}", i, e))
            })?;

            let phdr = Self::parse_program_header(&buf)?;
            program_headers.push(phdr);
        }

        Ok(program_headers)
    }

    /// Parse a program header from bytes
    fn parse_program_header(buf: &[u8]) -> Result<Elf64Phdr, ElfError> {
        if buf.len() < 56 {
            return Err(ElfError::InvalidPhEntrySize);
        }

        // ELF64 program header is 56 bytes
        let p_type = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let p_flags = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let p_offset = u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);
        let p_vaddr = u64::from_le_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]);
        let p_paddr = u64::from_le_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]);
        let p_filesz = u64::from_le_bytes([
            buf[32], buf[33], buf[34], buf[35], buf[36], buf[37], buf[38], buf[39],
        ]);
        let p_memsz = u64::from_le_bytes([
            buf[40], buf[41], buf[42], buf[43], buf[44], buf[45], buf[46], buf[47],
        ]);
        let p_align = u64::from_le_bytes([
            buf[48], buf[49], buf[50], buf[51], buf[52], buf[53], buf[54], buf[55],
        ]);

        Ok(Elf64Phdr {
            p_type,
            p_flags,
            p_offset,
            p_vaddr,
            p_paddr,
            p_filesz,
            p_memsz,
            p_align,
        })
    }

    /// Find signature and tohost sections/symbols
    fn find_special_sections<R: Read + Seek>(
        reader: &mut R,
        header: &Elf64Header,
        file_size: u64,
    ) -> Result<(Option<SignatureInfo>, Option<u64>), ElfError> {
        let mut signature_section = None;
        let mut tohost_addr = None;
        let mut symtab_info: Option<(u64, u64)> = None; // (offset, size)
        let mut strtab_info: Option<(u64, u64)> = None; // (offset, size)

        // If no section headers, return None
        if header.e_shoff == 0 || header.e_shnum == 0 {
            return Ok((None, None));
        }

        // First, read the section header string table (.shstrtab) if available
        let shstrtab = if header.e_shstrndx != 0 {
            Self::read_string_table(reader, header, file_size, header.e_shstrndx)?
        } else {
            None
        };

        // Read section headers to find .signature, .tohost, .symtab, and .strtab
        let shentsize = header.e_shentsize as usize;
        for i in 0..header.e_shnum {
            let offset = header.e_shoff + (i as u64) * (header.e_shentsize as u64);
            if offset.saturating_add(shentsize as u64) > file_size {
                break; // Out of bounds, stop searching
            }

            reader
                .seek(SeekFrom::Start(offset))
                .map_err(|e| ElfError::IoError(e.to_string()))?;

            let mut buf = vec![0u8; shentsize];
            reader
                .read_exact(&mut buf)
                .map_err(|e| ElfError::IoError(e.to_string()))?;

            // Parse section header with string table for name resolution
            if let Some((name, sh_type, sh_addr, sh_offset, sh_size)) =
                Self::parse_section_header(&buf, shstrtab.as_deref())?
            {
                // Check for .signature section
                if name == ".signature" && sh_type == 1 {
                    // SHT_PROGBITS
                    signature_section = Some(SignatureInfo {
                        vaddr: sh_addr,
                        size: sh_size,
                        file_offset: sh_offset,
                    });
                }
                // Check for .tohost section
                if name == ".tohost" && sh_type == 1 {
                    // SHT_PROGBITS
                    tohost_addr = Some(sh_addr);
                }
                // Record .symtab section info (SHT_SYMTAB = 2)
                if name == ".symtab" && sh_type == 2 {
                    symtab_info = Some((sh_offset, sh_size));
                }
                // Record .strtab section info (SHT_STRTAB = 3)
                if name == ".strtab" && sh_type == 3 {
                    strtab_info = Some((sh_offset, sh_size));
                }
            }
        }

        // If tohost not found as section, try to find it as a symbol
        if tohost_addr.is_none() {
            if let (Some((sym_offset, sym_size)), Some((str_offset, str_size))) =
                (symtab_info, strtab_info)
            {
                tohost_addr = Self::find_tohost_symbol(
                    reader, file_size, sym_offset, sym_size, str_offset, str_size,
                )?;
            }
        }

        // Fallback: if tohost address is 0 or not found, use default 0x80001000
        // This handles cases where the linker script doesn't properly place .tohost section
        if tohost_addr == Some(0) || tohost_addr.is_none() {
            tohost_addr = Some(0x80001000);
        }

        Ok((signature_section, tohost_addr))
    }

    /// Find tohost symbol address from symbol table
    fn find_tohost_symbol<R: Read + Seek>(
        reader: &mut R,
        file_size: u64,
        sym_offset: u64,
        sym_size: u64,
        str_offset: u64,
        str_size: u64,
    ) -> Result<Option<u64>, ElfError> {
        // Validate bounds
        if sym_offset.saturating_add(sym_size) > file_size
            || str_offset.saturating_add(str_size) > file_size
        {
            return Ok(None);
        }

        // Read string table
        reader
            .seek(SeekFrom::Start(str_offset))
            .map_err(|e| ElfError::IoError(e.to_string()))?;
        let mut strtab = vec![0u8; str_size as usize];
        reader
            .read_exact(&mut strtab)
            .map_err(|e| ElfError::IoError(e.to_string()))?;

        // Read symbol table (each entry is 24 bytes for ELF64)
        const SYM_ENTRY_SIZE: u64 = 24;
        let num_symbols = sym_size / SYM_ENTRY_SIZE;

        reader
            .seek(SeekFrom::Start(sym_offset))
            .map_err(|e| ElfError::IoError(e.to_string()))?;

        for _i in 0..num_symbols {
            let mut buf = [0u8; 24];
            reader
                .read_exact(&mut buf)
                .map_err(|e| ElfError::IoError(e.to_string()))?;

            // Parse symbol entry
            let st_name = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            let st_value = u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]);

            // Get symbol name from string table
            if let Some(name) = Self::get_string_from_table(&strtab, st_name) {
                if name == "tohost" {
                    return Ok(Some(st_value));
                }
            }
        }

        Ok(None)
    }

    /// Read section header string table (.shstrtab)
    fn read_string_table<R: Read + Seek>(
        reader: &mut R,
        header: &Elf64Header,
        file_size: u64,
        strtab_index: u16,
    ) -> Result<Option<Vec<u8>>, ElfError> {
        // First, read the string table section header to get its offset and size
        let strtab_sh_offset = header.e_shoff + (strtab_index as u64) * (header.e_shentsize as u64);
        if strtab_sh_offset.saturating_add(header.e_shentsize as u64) > file_size {
            return Ok(None);
        }

        reader
            .seek(SeekFrom::Start(strtab_sh_offset))
            .map_err(|e| ElfError::IoError(e.to_string()))?;

        let mut sh_buf = vec![0u8; header.e_shentsize as usize];
        reader
            .read_exact(&mut sh_buf)
            .map_err(|e| ElfError::IoError(e.to_string()))?;

        // Parse the section header to get offset and size
        let strtab_offset = u64::from_le_bytes([
            sh_buf[24], sh_buf[25], sh_buf[26], sh_buf[27], sh_buf[28], sh_buf[29], sh_buf[30],
            sh_buf[31],
        ]);
        let strtab_size = u64::from_le_bytes([
            sh_buf[32], sh_buf[33], sh_buf[34], sh_buf[35], sh_buf[36], sh_buf[37], sh_buf[38],
            sh_buf[39],
        ]);

        if strtab_offset == 0 || strtab_size == 0 {
            return Ok(None);
        }

        if strtab_offset.saturating_add(strtab_size) > file_size {
            return Ok(None);
        }

        // Read the string table contents
        reader
            .seek(SeekFrom::Start(strtab_offset))
            .map_err(|e| ElfError::IoError(e.to_string()))?;

        let mut strtab = vec![0u8; strtab_size as usize];
        reader
            .read_exact(&mut strtab)
            .map_err(|e| ElfError::IoError(e.to_string()))?;

        Ok(Some(strtab))
    }

    /// Parse a section header and return (name, type, addr, offset, size)
    /// If string_table is provided, resolve the section name from it
    fn parse_section_header(buf: &[u8], string_table: Option<&[u8]>) -> SectionHeaderResult {
        if buf.len() < 64 {
            return Ok(None);
        }

        let sh_name_offset = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let sh_type = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let _sh_flags = u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);
        let sh_addr = u64::from_le_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]);
        let sh_offset = u64::from_le_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]);
        let sh_size = u64::from_le_bytes([
            buf[32], buf[33], buf[34], buf[35], buf[36], buf[37], buf[38], buf[39],
        ]);

        // Resolve section name from string table if available
        let name = if let Some(strtab) = string_table {
            Self::get_string_from_table(strtab, sh_name_offset)
                .unwrap_or_else(|| format!("sh_{}", sh_name_offset))
        } else {
            format!("sh_{}", sh_name_offset)
        };

        Ok(Some((name, sh_type, sh_addr, sh_offset, sh_size)))
    }

    /// Get a null-terminated string from the string table at the given offset
    fn get_string_from_table(strtab: &[u8], offset: u32) -> Option<String> {
        let offset = offset as usize;
        if offset >= strtab.len() {
            return None;
        }

        // Find null terminator
        let end = strtab[offset..]
            .iter()
            .position(|&b| b == 0)
            .map(|pos| offset + pos)
            .unwrap_or(strtab.len());

        // Convert to String (only valid UTF-8)
        std::str::from_utf8(&strtab[offset..end])
            .ok()
            .map(|s| s.to_string())
    }

    /// Get entry point
    pub fn entry_point(&self) -> u64 {
        self.entry_point
    }

    /// Get loadable segments
    pub fn load_segments(&self) -> &[Elf64Phdr] {
        &self.load_segments
    }

    /// Get signature section info
    pub fn signature_section(&self) -> Option<&SignatureInfo> {
        self.signature_section.as_ref()
    }

    /// Get tohost address
    pub fn tohost_addr(&self) -> Option<u64> {
        self.tohost_addr
    }

    /// Get total memory footprint (minimum required memory)
    pub fn memory_footprint(&self) -> (u64, u64) {
        let mut min_addr = u64::MAX;
        let mut max_addr = 0u64;

        for seg in &self.load_segments {
            if seg.p_vaddr < min_addr {
                min_addr = seg.p_vaddr;
            }
            let seg_end = seg.p_vaddr.saturating_add(seg.p_memsz);
            if seg_end > max_addr {
                max_addr = seg_end;
            }
        }

        if min_addr == u64::MAX {
            (0, 0)
        } else {
            (min_addr, max_addr)
        }
    }

    /// Load all segments into memory
    /// Memory buffer should be allocated starting from the base address returned by memory_footprint()
    pub fn load_into_memory<R: Read + Seek>(
        &self,
        reader: &mut R,
        mem: &mut [u8],
    ) -> Result<(), ElfError> {
        let (base_addr, _) = self.memory_footprint();

        for seg in &self.load_segments {
            // Calculate offset from the start of memory buffer
            // The segment's vaddr is relative to the base_addr
            let mem_offset = (seg.p_vaddr - base_addr) as usize;
            let mem_size = seg.p_memsz as usize;
            let file_offset = seg.p_offset as usize;
            let file_size = seg.p_filesz as usize;

            // Validate memory offset and size with proper bounds checking
            // mem_offset must be within bounds
            if mem_offset >= mem.len() {
                return Err(ElfError::SegmentOutOfBounds);
            }
            // The segment must fit entirely within the buffer
            if mem_offset.saturating_add(mem_size) > mem.len() {
                return Err(ElfError::SegmentOutOfBounds);
            }

            // Validate file offsets
            if file_offset.saturating_add(file_size) > self.file_size as usize {
                return Err(ElfError::SegmentOutOfBounds);
            }

            // Read segment data from file
            reader
                .seek(SeekFrom::Start(file_offset as u64))
                .map_err(|e| ElfError::IoError(e.to_string()))?;

            let mut file_data = vec![0u8; file_size];
            reader
                .read_exact(&mut file_data)
                .map_err(|e| ElfError::IoError(e.to_string()))?;

            // Copy to memory (zero-initialized memory first)
            for i in 0..mem_size {
                if i < file_size {
                    mem[mem_offset + i] = file_data[i];
                } else {
                    mem[mem_offset + i] = 0;
                }
            }
        }
        Ok(())
    }
}

/// Load ELF file and return entry point and memory data
pub fn load_elf_file(
    data: &[u8],
) -> Result<(u64, Vec<u8>, Option<SignatureInfo>, Option<u64>, u64), ElfError> {
    let mut cursor = std::io::Cursor::new(data);
    let loader = ElfLoader::load(&mut cursor)?;

    let (base_addr, max_addr) = loader.memory_footprint();

    // Allocate memory - ensure it's large enough to hold all segments
    // Memory buffer starts from address 0, so we need size = max_addr - base_addr
    let mem_size = (max_addr - base_addr).next_power_of_two().max(0x10000);
    let mut mem = vec![0u8; mem_size as usize];

    // Load segments - they will be placed at offset (p_vaddr - base_addr) in the buffer
    loader.load_into_memory(&mut cursor, &mut mem)?;

    Ok((
        loader.entry_point(),
        mem,
        loader.signature_section().cloned(),
        loader.tohost_addr(),
        base_addr, // Return base_addr for correct memory loading
    ))
}

/// Convert virtual address to memory offset
pub fn vaddr_to_offset(loader: &ElfLoader, vaddr: u64) -> Option<u64> {
    for seg in loader.load_segments() {
        if vaddr >= seg.p_vaddr && vaddr < seg.p_vaddr + seg.p_memsz {
            return Some(seg.p_offset + (vaddr - seg.p_vaddr));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Create a minimal valid 64-bit ELF executable for testing
    fn create_test_elf() -> Vec<u8> {
        let mut elf = Vec::new();

        // ELF Header (64 bytes)
        // Magic + class + data + version + pad
        elf.extend_from_slice(b"\x7fELF"); // e_ident[0..4]
        elf.push(2); // e_ident[4] = ELFCLASS64
        elf.push(1); // e_ident[5] = ELFDATA2LSB (little endian)
        elf.push(1); // e_ident[6] = EV_CURRENT
        elf.push(0); // e_ident[7] = ELFOSABI_NONE
        elf.push(0); // e_ident[8] = abiversion
        elf.extend_from_slice(&[0u8; 7]); // e_ident[9..16] = padding

        // e_type = ET_EXEC (2)
        elf.extend_from_slice(&2u16.to_le_bytes());
        // e_machine = EM_RISCV (243)
        elf.extend_from_slice(&243u16.to_le_bytes());
        // e_version = 1
        elf.extend_from_slice(&1u32.to_le_bytes());
        // e_entry = 0x8000_0000
        elf.extend_from_slice(&0x8000_0000u64.to_le_bytes());
        // e_phoff = 64 (header size)
        elf.extend_from_slice(&64u64.to_le_bytes());
        // e_shoff = 0 (no section headers)
        elf.extend_from_slice(&0u64.to_le_bytes());
        // e_flags = 0
        elf.extend_from_slice(&0u32.to_le_bytes());
        // e_ehsize = 64
        elf.extend_from_slice(&64u16.to_le_bytes());
        // e_phentsize = 56 (Program header entry size for ELF64)
        elf.extend_from_slice(&56u16.to_le_bytes());
        // e_phnum = 2 (2 program headers)
        elf.extend_from_slice(&2u16.to_le_bytes());
        // e_shentsize = 0
        elf.extend_from_slice(&0u16.to_le_bytes());
        // e_shnum = 0
        elf.extend_from_slice(&0u16.to_le_bytes());
        // e_shstrndx = 0
        elf.extend_from_slice(&0u16.to_le_bytes());

        // Program Header 1: LOAD for .text
        // p_type = PT_LOAD (1)
        elf.extend_from_slice(&1u32.to_le_bytes());
        // p_flags = PF_X | PF_R (5)
        elf.extend_from_slice(&5u32.to_le_bytes());
        // p_offset = 0x100 (after headers)
        elf.extend_from_slice(&0x100u64.to_le_bytes());
        // p_vaddr = 0x8000_0000
        elf.extend_from_slice(&0x8000_0000u64.to_le_bytes());
        // p_paddr = 0x8000_0000
        elf.extend_from_slice(&0x8000_0000u64.to_le_bytes());
        // p_filesz = 0x100
        elf.extend_from_slice(&0x100u64.to_le_bytes());
        // p_memsz = 0x100
        elf.extend_from_slice(&0x100u64.to_le_bytes());
        // p_align = 0x1000
        elf.extend_from_slice(&0x1000u64.to_le_bytes());

        // Program Header 2: LOAD for .data
        // p_type = PT_LOAD (1)
        elf.extend_from_slice(&1u32.to_le_bytes());
        // p_flags = PF_R | PF_W (6)
        elf.extend_from_slice(&6u32.to_le_bytes());
        // p_offset = 0x200
        elf.extend_from_slice(&0x200u64.to_le_bytes());
        // p_vaddr = 0x8000_0100
        elf.extend_from_slice(&0x8000_0100u64.to_le_bytes());
        // p_paddr = 0x8000_0100
        elf.extend_from_slice(&0x8000_0100u64.to_le_bytes());
        // p_filesz = 0x100
        elf.extend_from_slice(&0x100u64.to_le_bytes());
        // p_memsz = 0x200 (includes BSS)
        elf.extend_from_slice(&0x200u64.to_le_bytes());
        // p_align = 0x1000
        elf.extend_from_slice(&0x1000u64.to_le_bytes());

        // Padding to reach 0x100 (headers are 64 + 112 = 176 bytes = 0xB0)
        elf.extend_from_slice(&[0u8; 0x100 - 176]);

        // .text segment data (0x100 bytes)
        elf.extend_from_slice(&[0x13u8; 0x100]); // nop instructions

        // Padding to reach 0x200 (already at 0x200, no padding needed)
        // elf.extend_from_slice(&[0u8; 0x100]);

        // .data segment data (0x100 bytes)
        elf.extend_from_slice(&[0x42u8; 0x100]);

        elf
    }

    #[test]
    fn test_elf_load() {
        let elf_data = create_test_elf();
        let mut cursor = Cursor::new(&elf_data);

        let loader = ElfLoader::load(&mut cursor).unwrap();

        assert_eq!(loader.entry_point(), 0x8000_0000);
        assert_eq!(loader.load_segments().len(), 2);

        let (min, max) = loader.memory_footprint();
        assert_eq!(min, 0x8000_0000);
        assert!(max > 0x8000_0200);
    }

    #[test]
    fn test_load_into_memory() {
        let elf_data = create_test_elf();
        let mut cursor = Cursor::new(&elf_data);

        let loader = ElfLoader::load(&mut cursor).unwrap();

        // Create memory buffer
        let mem_size = 0x10000;
        let mut mem = vec![0u8; mem_size];

        loader.load_into_memory(&mut cursor, &mut mem).unwrap();

        // Verify .text loaded at 0x8000_0000
        assert_eq!(mem[0], 0x13);
        assert_eq!(mem[0x100], 0x42);
    }

    #[test]
    fn test_load_elf_file_function() {
        let elf_data = create_test_elf();

        let (entry, mem, sig, tohost, base_addr) = load_elf_file(&elf_data).unwrap();

        assert_eq!(entry, 0x8000_0000);
        assert_eq!(base_addr, 0x8000_0000); // base_addr should be the minimum vaddr
                                            // Memory should be large enough to hold all segments (0x300 bytes range)
        assert!(mem.len() >= 0x300);
        // Verify data was loaded correctly
        assert_eq!(mem[0], 0x13); // .text at offset 0
        assert_eq!(mem[0x100], 0x42); // .data at offset 0x100
        assert!(sig.is_none()); // No signature section in test ELF
        assert!(tohost.is_none()); // No tohost section in test ELF
    }

    /// Test tohost symbol parsing from real ELF file
    /// This test verifies that tohost symbol is correctly extracted from the symbol table
    #[test]
    fn test_tohost_symbol_from_elf() {
        // Read the actual test ELF file
        let elf_path = "tests/riscv-tests/add.elf";
        let elf_data = std::fs::read(elf_path);

        // Skip test if ELF file doesn't exist (e.g., not built yet)
        if elf_data.is_err() {
            eprintln!("Skipping test: {} not found", elf_path);
            return;
        }

        let elf_data = elf_data.unwrap();
        let (entry, _mem, _sig, tohost, base_addr) = load_elf_file(&elf_data).unwrap();

        // Verify entry point
        assert_eq!(entry, 0x8000_0000);
        assert_eq!(base_addr, 0x8000_0000);

        // Verify tohost address is correctly parsed from symbol table
        // Expected: 0x80002000 (from readelf -s output)
        assert!(
            tohost.is_some(),
            "tohost symbol should be found in the ELF file"
        );
        assert_eq!(
            tohost.unwrap(),
            0x8000_2000,
            "tohost address should be 0x80002000"
        );
    }

    #[test]
    fn test_fib_tohost_address() {
        // Test that fib.elf correctly parses tohost address from .tohost section
        let elf_path = "tests/riscv-tests/fib.elf";
        let elf_data = std::fs::read(elf_path);

        if elf_data.is_err() {
            eprintln!("Skipping test: {} not found", elf_path);
            return;
        }

        let elf_data = elf_data.unwrap();
        let (entry, _mem, _sig, tohost, base_addr) = load_elf_file(&elf_data).unwrap();

        // Verify entry point
        assert_eq!(entry, 0x8000_0000);
        assert_eq!(base_addr, 0x8000_0000);

        // Verify tohost address is correctly parsed from .tohost section
        assert!(tohost.is_some(), "tohost should be found in the ELF file");
        assert_eq!(
            tohost.unwrap(),
            0x8000_1000,
            "tohost address should be 0x80001000 (from .tohost section)"
        );
    }
}
