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
//!
//! Negative controls behind this suite (`scripts/debug/stage6-mutations`):
//! `gc-u2-guest-default` (the `s4_*` arm moved to the default) and, through
//! the A10 oracle suite, `gc-u5-a10-machine-default` (the Machine routes
//! left on the default profile; U5).

use std::path::{Path, PathBuf};

/// One `case` arm, in source order: `(patterns, isa_args)`.
fn declared_arms(script: &str) -> Vec<(Vec<String>, String)> {
    script
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (patterns, args) = line.split_once(") isa_args=(")?;
            let args = args
                .trim_end_matches(" ;;")
                .trim()
                .trim_end_matches(')')
                .trim();
            Some((
                patterns
                    .split('|')
                    .map(str::trim)
                    .map(str::to_string)
                    .collect(),
                args.to_string(),
            ))
        })
        .collect()
}

/// The suite's patterns only ever use one `*` wildcard.
fn glob_match(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((prefix, suffix)) => {
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
    }
}

/// Bash `case` semantics: the first matching arm wins.
fn declared_profile<'a>(arms: &'a [(Vec<String>, String)], name: &str) -> &'a str {
    arms.iter()
        .find(|(patterns, _)| patterns.iter().any(|p| glob_match(p, name)))
        .map(|(_, args)| args.as_str())
        .expect("the catch-all arm matches every guest name")
}

/// The basename of every guest source, without the `.S` suffix — the
/// identity run_elf_tests.sh derives from the ELF path (`$(basename ...)`).
fn guest_names() -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("guest source directory") {
            let path = entry.expect("guest entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "S") {
                out.push(
                    path.file_stem()
                        .expect("guest file stem")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    let mut names = Vec::new();
    walk(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/bare-metal-riscv-test"),
        &mut names,
    );
    names.sort();
    names
}

#[test]
fn every_guest_declares_its_profile() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = std::fs::read_to_string(repository.join("scripts/run_elf_tests.sh"))
        .expect("run_elf_tests.sh");
    let arms = declared_arms(&script);

    // Each declaration pattern appears in exactly one arm: a later duplicate
    // would silently shadow an earlier profile under bash case semantics.
    for pattern in ["s4_*", "zca_*", "fd_*", "zcd_*", "fdc_*", "*"] {
        let hits = arms
            .iter()
            .filter(|(patterns, _)| patterns.iter().any(|p| p == pattern))
            .count();
        assert_eq!(hits, 1, "pattern {pattern} must appear in exactly one arm");
    }

    // Profile-dependent guests are bound to explicit profiles: the s4_*
    // set (misa/FS/IALIGN=32 assertions) under rv64ima, compressed guests
    // under rv64imac, F/D guests under rv64imafd, and the Zcd plus F/D/C
    // attach guests under rv64imafdc.
    for (pattern, expected) in [
        ("s4_*", "--isa rv64ima"),
        ("zca_*", "--isa rv64imac"),
        ("fd_*", "--isa rv64imafd"),
        ("zcd_*", "--isa rv64imafdc"),
        ("fdc_*", "--isa rv64imafdc"),
    ] {
        let args = &arms
            .iter()
            .find(|(patterns, _)| patterns.iter().any(|p| p == pattern))
            .expect("arm exists")
            .1;
        assert_eq!(args, expected, "arm {pattern}");
    }

    // The remaining guests declare the default: no --isa in the catch-all.
    assert_eq!(
        arms.iter()
            .find(|(p, _)| p.iter().any(|x| x == "*"))
            .expect("catch-all")
            .1,
        ""
    );

    // Every declared profile is an approved name; the runner never passes
    // an unknown profile or a bare --isa.
    for (patterns, args) in &arms {
        if !args.is_empty() {
            let profile = args
                .strip_prefix("--isa ")
                .expect("isa_args must be --isa NAME");
            assert!(
                matches!(profile, "rv64ima" | "rv64imac" | "rv64imafd" | "rv64imafdc"),
                "{patterns:?}: unapproved profile {profile}"
            );
        }
    }

    // Resolve every actual guest through the arms under bash first-match
    // order, and pin each one to the profile its source directory and name
    // require. An extra singleton arm (for example `fd_flags) isa_args=( )`)
    // shadowing any guest now fails here, not only for representative names.
    let names = guest_names();
    assert!(
        names.len() >= 100,
        "guest sources discovered: {}",
        names.len()
    );
    for name in &names {
        let actual = declared_profile(&arms, name);
        let expected = if name.starts_with("s4_") {
            "--isa rv64ima"
        } else if name.starts_with("zca_") {
            "--isa rv64imac"
        } else if name.starts_with("fd_") {
            "--isa rv64imafd"
        } else if name.starts_with("zcd_") || name.starts_with("fdc_") {
            "--isa rv64imafdc"
        } else {
            ""
        };
        assert_eq!(actual, expected, "guest {name} resolved profile");
    }
    // The resolved identities of the profile-dependent fixtures.
    let profile = |name: &str| declared_profile(&arms, name);
    assert_eq!(profile("s4_fp_absent"), "--isa rv64ima");
    assert_eq!(profile("s4_csr_policy"), "--isa rv64ima");
    assert_eq!(profile("s4_counters"), "--isa rv64ima");
    assert_eq!(profile("zca_u5_attach"), "--isa rv64imac");
    assert_eq!(profile("fd_rounding"), "--isa rv64imafd");
    assert_eq!(profile("zcd_transfer"), "--isa rv64imafdc");
    assert_eq!(profile("fdc_u6_attach"), "--isa rv64imafdc");
    for default_guest in [
        "hello",
        "add",
        "mul",
        "trap_illegal",
        "s5_wfi_none",
        "s6_ebreak",
    ] {
        assert_eq!(
            profile(default_guest),
            "",
            "{default_guest} declares the default"
        );
    }
}
