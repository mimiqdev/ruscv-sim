use ruscv_sim::core::commits::{CommitLogger, MemoryAccess};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test: Register value changes from non-zero to zero (first register change)
/// This tests the edge case at commits.rs L89 where a register value becomes 0
#[test]
fn test_register_change_to_zero() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("reg_to_zero.log");

    {
        let mut logger = CommitLogger::new_file(&test_file).unwrap();

        // regs_before: x5 = 0xABCD
        // regs_after: x5 = 0x0000 (changed to zero)
        let mut regs_before = [0u64; 32];
        let mut regs_after = [0u64; 32];
        regs_before[5] = 0xABCD_EF00_1234_5678;
        regs_after[5] = 0x0000_0000_0000_0000; // Value changed to zero

        logger
            .log_commit(
                0,           // hartid
                3,           // privilege (machine mode)
                0x8000_0000, // pc
                0x00500193,  // opcode (addi x3, x0, 5)
                &regs_before,
                &regs_after,
                None,
            )
            .unwrap();
    }

    // Verify that the change to zero is logged
    let mut content = String::new();
    File::open(&test_file)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    // Should contain x5 with zero value
    assert!(
        content.contains(" x5  0x0000000000000000"),
        "Expected register change to zero to be logged, got: {}",
        content
    );
    assert!(content.starts_with("core   0: 3 0x0000000080000000"));
}

/// Test: Memory store with value = None (commits.rs L112 edge case)
/// Tests the branch where store has no value
#[test]
fn test_memory_store_with_none_value() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("store_none.log");

    {
        let mut logger = CommitLogger::new_file(&test_file).unwrap();

        let regs = [0u64; 32];

        // MemoryAccess for store with None value
        let mem_access = MemoryAccess {
            addr: 0x8000_0100,
            value: None, // None value for store
            is_store: true,
        };

        logger
            .log_commit(
                0,
                3,
                0x8000_0010,
                0x00152023, // store instruction
                &regs,
                &regs,
                Some(&mem_access),
            )
            .unwrap();
    }

    let mut content = String::new();
    File::open(&test_file)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    // Should output: mem <addr> (without value)
    assert!(
        content.contains("mem 0x80000100"),
        "Expected store with no value to be logged, got: {}",
        content
    );
    // Should NOT contain a value after the address
    assert!(
        !content.contains("mem 0x80000100 0x"),
        "Store with None value should not have value printed"
    );
}

/// Test: Multiple register changes including to-zero values
#[test]
fn test_multiple_reg_changes_including_zero() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("multi_reg_zero.log");

    {
        let mut logger = CommitLogger::new_file(&test_file).unwrap();

        let mut regs_before = [0u64; 32];
        let mut regs_after = [0u64; 32];

        // x10: 0 -> non-zero
        // x11: non-zero -> 0
        // x12: non-zero -> non-zero
        regs_before[10] = 0;
        regs_after[10] = 0x100;
        regs_before[11] = 0x200;
        regs_after[11] = 0; // Changed to zero
        regs_before[12] = 0x300;
        regs_after[12] = 0x400;

        logger
            .log_commit(
                0,
                3,
                0x8000_0000,
                0x00000000,
                &regs_before,
                &regs_after,
                None,
            )
            .unwrap();
    }

    let mut content = String::new();
    File::open(&test_file)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    assert!(content.contains(" x10  0x0000000000000100"));
    assert!(content.contains(" x11  0x0000000000000000"));
    assert!(content.contains(" x12  0x0000000000000400"));
}

/// Test: Commit logging with stdout output
#[test]
fn test_commit_logger_stdout() {
    // Create logger that writes to stdout
    let logger = CommitLogger::new_stdout();
    // This should not panic
    let _ = logger;
}

/// Test: First register change detection (always logged)
#[test]
fn test_first_register_change_is_logged() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("first_change.log");

    {
        let mut logger = CommitLogger::new_file(&test_file).unwrap();

        // x1 changes from 0 to value
        let mut regs_before = [0u64; 32];
        let mut regs_after = [0u64; 32];
        regs_after[1] = 0x8000_0000; // Return address

        logger
            .log_commit(
                0,
                3,
                0x8000_0000,
                0x00000093, // auipc x1, 0
                &regs_before,
                &regs_after,
                None,
            )
            .unwrap();
    }

    let mut content = String::new();
    File::open(&test_file)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    assert!(content.contains(" x1  0x0000000080000000"));
}

/// Test: No register changes results in only header
#[test]
fn test_no_register_changes() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("no_changes.log");

    {
        let mut logger = CommitLogger::new_file(&test_file).unwrap();

        let regs = [0u64; 32];

        logger
            .log_commit(0, 3, 0x8000_0000, 0x00000000, &regs, &regs, None)
            .unwrap();
    }

    let mut content = String::new();
    File::open(&test_file)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    // Should only contain header, no register changes
    let parts: Vec<&str> = content.split(" x").collect();
    assert_eq!(
        parts.len(),
        1,
        "Expected no register changes, got: {}",
        content
    );
}

/// Test: Memory load with None value
#[test]
fn test_memory_load_with_none_value() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("load_none.log");

    {
        let mut logger = CommitLogger::new_file(&test_file).unwrap();

        let regs = [0u64; 32];

        let mem_access = MemoryAccess {
            addr: 0x8000_1000,
            value: None,
            is_store: false, // Load
        };

        logger
            .log_commit(
                0,
                3,
                0x8000_0008,
                0x00052083,
                &regs,
                &regs,
                Some(&mem_access),
            )
            .unwrap();
    }

    let mut content = String::new();
    File::open(&test_file)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    assert!(content.contains("mem 0x80001000"));
}

/// Test: Memory store with Some value
#[test]
fn test_memory_store_with_value() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("store_value.log");

    {
        let mut logger = CommitLogger::new_file(&test_file).unwrap();

        let regs = [0u64; 32];

        let mem_access = MemoryAccess {
            addr: 0x8000_0180,
            value: Some(0x12345678),
            is_store: true,
        };

        logger
            .log_commit(
                0,
                3,
                0x8000_0010,
                0x00152023,
                &regs,
                &regs,
                Some(&mem_access),
            )
            .unwrap();
    }

    let mut content = String::new();
    File::open(&test_file)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    assert!(content.contains("mem 0x80000180 0x12345678"));
}

/// Test: Log comment functionality
#[test]
fn test_log_comment() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("comment.log");

    {
        let mut logger = CommitLogger::new_file(&test_file).unwrap();

        logger.log_comment("test comment").unwrap();
        logger.log_comment("another comment").unwrap();
    }

    let mut content = String::new();
    File::open(&test_file)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    assert!(content.contains(">>>>  test comment"));
    assert!(content.contains(">>>>  another comment"));
}

/// Test: MemoryAccess creation helpers
#[test]
fn test_memory_access_helpers() {
    let load = MemoryAccess::load(0x8000_1000);
    assert_eq!(load.addr, 0x8000_1000);
    assert!(!load.is_store);
    assert!(load.value.is_none());

    let store = MemoryAccess::store(0x8000_2000, 0xDEAD_BEEF);
    assert_eq!(store.addr, 0x8000_2000);
    assert!(store.is_store);
    assert_eq!(store.value, Some(0xDEAD_BEEF));
}

/// Test: Log file to directory that is not writable returns error
#[test]
fn test_log_file_to_unwritable_directory() {
    // Create a temporary directory
    let temp_dir = TempDir::new().unwrap();

    // Create a subdirectory that is read-only
    let readonly_dir = temp_dir.path().join("readonly");
    std::fs::create_dir(&readonly_dir).unwrap();

    // Make the directory read-only (remove write permission)
    let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&readonly_dir, perms).unwrap();

    // Try to create a log file in the read-only directory
    let test_file = readonly_dir.join("test.log");

    // This should fail with a permission denied error
    let result = CommitLogger::new_file(&test_file);

    assert!(
        result.is_err(),
        "Expected error when creating file in read-only directory"
    );
    // On Unix-like systems, this should be a permission denied error
    #[cfg(unix)]
    {
        if let Err(error) = result {
            assert_eq!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied,
                "Expected PermissionDenied error, got: {:?}",
                error.kind()
            );
        }
    }
}

/// Test: Log file to non-existent directory returns error
#[test]
fn test_log_file_to_nonexistent_directory() {
    let nonexistent_path = PathBuf::from("/nonexistent/path/that/does/not/exist/test.log");

    let result = CommitLogger::new_file(&nonexistent_path);

    assert!(
        result.is_err(),
        "Expected error when creating file in non-existent directory"
    );
    // This should be a "not found" error
    #[cfg(unix)]
    {
        if let Err(error) = result {
            assert_eq!(
                error.kind(),
                std::io::ErrorKind::NotFound,
                "Expected NotFound error, got: {:?}",
                error.kind()
            );
        }
    }
}

/// Test: Log file to a file path that is actually a directory returns error
#[test]
fn test_log_file_to_directory_returns_error() {
    let temp_dir = TempDir::new().unwrap();

    // Try to create a log file using a directory path
    let result = CommitLogger::new_file(temp_dir.path());

    assert!(result.is_err(), "Expected error when path is a directory");
    if let Err(error) = result {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::IsADirectory,
            "Expected IsADirectory error, got: {:?}",
            error.kind()
        );
    }
}
