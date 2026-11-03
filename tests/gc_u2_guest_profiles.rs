//! Guest-suite profile declarations (RV64GC contract D4).
//!
//! Every guest in the fresh ELF suite declares its profile in
//! `scripts/run_elf_tests.sh`. Guests whose expectations depend on the
//! profile — the `s4_*` set asserting the rv64ima `misa`, FS=0 or
//! IALIGN=32 rules — run under explicit `--isa rv64ima`; the Zca, F/D and
//! Zcd/attach guests keep their profiles; every other guest declares the
//! default with no `--isa`. A profile-pinned guest silently moved to the
//! default fails this test, so the mutation runner can express the
//! guest-pinning negative control against this file.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// One `case` arm from the profile-selection block in run_elf_tests.sh.
fn declared_profiles(script: &str) -> BTreeMap<String, String> {
    let mut profiles = BTreeMap::new();
    for line in script.lines() {
        let line = line.trim();
        let Some((patterns, args)) = line.split_once(") isa_args=(") else {
            continue;
        };
        let args = args
            .trim_end_matches(" ;;")
            .trim()
            .trim_end_matches(')')
            .trim();
        for pattern in patterns.split('|') {
            profiles.insert(pattern.trim().to_string(), args.to_string());
        }
    }
    profiles
}

#[test]
fn every_guest_declares_its_profile() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = std::fs::read_to_string(repository.join("scripts/run_elf_tests.sh"))
        .expect("run_elf_tests.sh");
    let profiles = declared_profiles(&script);

    // The s4_* arm and each extension arm exist exactly once; multi-pattern
    // arms (zcd_*|fdc_*) were recorded per pattern by declared_profiles().
    for pattern in ["s4_*", "zca_*", "fd_*", "zcd_*", "fdc_*", "*"] {
        assert!(
            profiles.contains_key(pattern),
            "guest profile declaration arm missing: {pattern}"
        );
    }

    // Profile-dependent guests are bound to explicit profiles: the s4_*
    // set (misa/FS/IALIGN=32 assertions) under rv64ima, compressed guests
    // under rv64imac, F/D guests under rv64imafd, and the Zcd plus F/D/C
    // attach guests under rv64imafdc.
    assert_eq!(profiles["s4_*"], "--isa rv64ima");
    assert_eq!(profiles["zca_*"], "--isa rv64imac");
    assert_eq!(profiles["fd_*"], "--isa rv64imafd");
    assert_eq!(profiles["zcd_*"], "--isa rv64imafdc");
    assert_eq!(profiles["fdc_*"], "--isa rv64imafdc");

    // The remaining guests declare the default: no --isa in the catch-all.
    assert_eq!(profiles["*"], "");

    // Every declared profile is an approved name; the runner never passes
    // an unknown profile or a bare --isa.
    for (pattern, args) in &profiles {
        if !args.is_empty() {
            let profile = args
                .strip_prefix("--isa ")
                .expect("isa_args must be --isa NAME");
            assert!(
                matches!(profile, "rv64ima" | "rv64imac" | "rv64imafd" | "rv64imafdc"),
                "{pattern}: unapproved profile {profile}"
            );
        }
    }
}
