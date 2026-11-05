//! Direct-memory U1: grant protocol, read path, invalidation and reset.

use super::direct_mem::DIRECT_MEM_REGION_LIMIT;
use super::test_support::{DirectMemReply, DirectMemTarget};
use super::*;
use crate::physical::{
    AmoWidth, AtomicOrdering, AtomicRequest, AtomicReservationContext, ConditionalStatus,
    PhysicalAccessError, PhysicalAccessKind, PhysicalRequest, PhysicalSpan, PhysicalWidth,
    ValidatedAtomicAccess, ValidatedPhysicalAccess,
};
use crate::ram::RamSlot;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

const BASE: u64 = 0x8000_0000;
const SIZE: usize = 0x4000;
/// The declared per-transaction maximum the latency fixtures use: one tick.
const MAX_DELAY: u64 = 1_000_000;
const ORDERING: AtomicOrdering = AtomicOrdering {
    aq: false,
    rl: false,
};

fn target() -> Arc<DirectMemTarget> {
    DirectMemTarget::new(BASE, SIZE)
}

/// A TLM slot over `target`, with the window installed through debug transport
/// exactly as `Platform::build` does.
fn slot_of(target: &Arc<DirectMemTarget>, direct_mem: bool) -> RamSlot {
    let slot = RamSlot::tlm(target.clone(), BASE, SIZE, TickPeriod::DEFAULT, direct_mem).unwrap();
    slot.begin_window().unwrap();
    slot
}

fn slot(target: &Arc<DirectMemTarget>) -> RamSlot {
    slot_of(target, true)
}

fn read(
    port: &mut ValidatedPhysicalAccess<RamSlot>,
    address: u64,
    width: PhysicalWidth,
) -> Vec<u8> {
    let request = PhysicalRequest::data_read(address, width).unwrap();
    port.access(request).unwrap().read_bytes().unwrap().to_vec()
}

fn fetch(
    port: &mut ValidatedPhysicalAccess<RamSlot>,
    address: u64,
    width: PhysicalWidth,
) -> Vec<u8> {
    let request = PhysicalRequest::fetch(address, width).unwrap();
    port.access(request).unwrap().read_bytes().unwrap().to_vec()
}

fn write(port: &mut ValidatedPhysicalAccess<RamSlot>, address: u64, payload: &[u8]) {
    let width = PhysicalWidth::from_bytes(payload.len()).unwrap();
    let request = PhysicalRequest::data_write(address, width, payload).unwrap();
    port.access(request).unwrap();
}

// ---- D3/D4: the hint, the query and the cached read path ----

#[test]
fn a_cached_grant_serves_fetches_and_loads_without_b_transport() {
    let target = target();
    let slot = slot(&target);
    slot.program(0x100, &[0x5a; 8]).unwrap();
    let mut port = ValidatedPhysicalAccess::new(slot.clone());

    // The first access takes b_transport: the hint arrives with its response,
    // and only then may the adapter query.
    assert_eq!(
        read(&mut port, BASE + 0x100, PhysicalWidth::Doubleword).len(),
        8
    );
    assert_eq!(
        target.transactions(),
        1,
        "the first read must take b_transport"
    );
    assert_eq!(target.queries(), 1, "the hint did not produce one query");
    assert_eq!(slot.cached_direct_mem_regions(), 1);

    let primed = target.transactions();
    assert_eq!(
        read(&mut port, BASE + 0x104, PhysicalWidth::Word),
        vec![0x5a; 4],
        "the direct read returned different bytes than the grant holds"
    );
    assert_eq!(
        target.transactions(),
        primed,
        "a covered load took b_transport"
    );
    assert_eq!(
        fetch(&mut port, BASE + 0x100, PhysicalWidth::Word),
        vec![0x5a; 4]
    );
    assert_eq!(
        target.transactions(),
        primed,
        "a covered fetch took b_transport"
    );
    assert_eq!(
        target.queries(),
        1,
        "a cached read queried the target again"
    );

    // A second chunk is outside the grant: one transaction, one new query.
    assert_eq!(read(&mut port, BASE + 0x1100, PhysicalWidth::Word).len(), 4);
    assert_eq!(target.transactions(), primed + 1);
    assert_eq!(target.queries(), 2);
    assert_eq!(slot.cached_direct_mem_regions(), 2);
}

#[test]
fn direct_memory_off_issues_no_query_and_uses_no_grant() {
    let target = target();
    let slot = slot_of(&target, false);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    for expected in 1..=2 {
        assert_eq!(read(&mut port, BASE + 0x100, PhysicalWidth::Word).len(), 4);
        assert_eq!(target.transactions(), expected, "a read left b_transport");
        assert_eq!(target.queries(), 0, "direct memory off issued a query");
        assert_eq!(slot.cached_direct_mem_regions(), 0);
    }
}

#[test]
fn an_absent_hint_or_a_denial_caches_nothing() {
    // No hint: the adapter never queries.
    let target = target();
    target.set_hint(false);
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        target.transactions(),
        2,
        "an absent hint changed the access"
    );
    assert_eq!(target.queries(), 0, "an absent hint produced a query");
    assert_eq!(slot.cached_direct_mem_regions(), 0);

    // Denied: one query per hinted access, and nothing cached.
    let target = self::target();
    target.set_reply(DirectMemReply::Denied);
    let slot = self::slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(target.queries(), 1);
    assert_eq!(slot.cached_direct_mem_regions(), 0);
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(target.transactions(), 2, "a denial changed the next access");
    assert_eq!(target.queries(), 2, "a denial was cached as a decision");
}

#[test]
fn a_failed_query_is_an_unknown_completion() {
    let target = target();
    target.set_reply(DirectMemReply::Failed);
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    let request = PhysicalRequest::data_read(BASE, PhysicalWidth::Word).unwrap();
    let error = port.access(request).unwrap_err();
    assert!(
        matches!(error, PhysicalAccessError::UnknownCompletion(_)),
        "a contained query failure was not an unknown completion: {error:?}"
    );
    assert_eq!(slot.cached_direct_mem_regions(), 0);
    assert!(
        slot.poison_reason().is_some(),
        "a failed query did not poison"
    );
}

// ---- D3: malformed grants ----

#[test]
fn every_malformed_grant_is_a_protocol_failure_of_the_querying_access() {
    let cases = [
        (
            "a null pointer",
            DirectMemReply::Custom {
                access: BlockingDirectMemAccess::Read,
                start: BASE,
                end: BASE + 0xfff,
                read_latency: 0,
                write_latency: 0,
                null_pointer: true,
            },
            "its pointer is null",
        ),
        (
            "an inverted range",
            DirectMemReply::Custom {
                access: BlockingDirectMemAccess::Read,
                start: BASE + 8,
                end: BASE + 4,
                read_latency: 0,
                write_latency: 0,
                null_pointer: false,
            },
            "is inverted",
        ),
        (
            "an overflowing range",
            DirectMemReply::Custom {
                access: BlockingDirectMemAccess::Read,
                start: 0,
                end: u64::MAX,
                read_latency: 0,
                write_latency: 0,
                null_pointer: false,
            },
            "does not fit a representable length",
        ),
        (
            "no access",
            DirectMemReply::Custom {
                access: BlockingDirectMemAccess::NoAccess,
                start: BASE,
                end: BASE + 0xfff,
                read_latency: 0,
                write_latency: 0,
                null_pointer: false,
            },
            "it grants no access",
        ),
        (
            "a range that misses the queried address",
            DirectMemReply::Custom {
                access: BlockingDirectMemAccess::Read,
                start: BASE + 0x1000,
                end: BASE + 0x1fff,
                read_latency: 0,
                write_latency: 0,
                null_pointer: false,
            },
            "does not contain the queried address",
        ),
        (
            "a read latency above the declared maximum",
            DirectMemReply::Custom {
                access: BlockingDirectMemAccess::Read,
                start: BASE,
                end: BASE + 0xfff,
                read_latency: MAX_DELAY + 1,
                write_latency: 0,
                null_pointer: false,
            },
            "read latency of 1000001 fs is above the declared per-transaction maximum",
        ),
        (
            "a write latency above the declared maximum",
            DirectMemReply::Custom {
                access: BlockingDirectMemAccess::Read,
                start: BASE,
                end: BASE + 0xfff,
                read_latency: 0,
                write_latency: MAX_DELAY * 2,
                null_pointer: false,
            },
            "write latency of 2000000 fs is above the declared per-transaction maximum",
        ),
    ];
    for (name, reply, expected) in cases {
        let target = target();
        target.set_declared_max_delay(MAX_DELAY);
        target.set_reply(reply);
        let slot = slot(&target);
        let mut port = ValidatedPhysicalAccess::new(slot.clone());
        // The queried access is the first read of chunk 0.
        let request = PhysicalRequest::data_read(BASE + 0x100, PhysicalWidth::Word).unwrap();
        let error = match port.access(request) {
            Ok(_) => panic!("{name}: a malformed grant was accepted"),
            Err(error) => error,
        };
        assert!(
            matches!(error, PhysicalAccessError::Protocol(_)),
            "{name}: a malformed grant was not a protocol failure: {error:?}"
        );
        assert!(
            error.to_string().contains(expected),
            "{name}: the classification does not name the defect: {error}"
        );
        assert_eq!(
            slot.cached_direct_mem_regions(),
            0,
            "{name}: a malformed grant was cached"
        );
        // A malformed grant is not an unknown completion: the transport stays
        // usable and the next access is an ordinary transaction.
        assert!(
            slot.poison_reason().is_none(),
            "{name}: the transport was poisoned"
        );
        // The transport stays usable: a denial now completes normally.
        target.set_reply(DirectMemReply::Denied);
        assert_eq!(read(&mut port, BASE + 0x100, PhysicalWidth::Word).len(), 4);
    }
}

// ---- D4/D5: permission and span rules ----

#[test]
fn a_span_crossing_a_grant_end_stays_one_transaction() {
    let target = target();
    let slot = slot(&target);
    slot.program(0x100, &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88])
        .unwrap();
    target.set_reply(DirectMemReply::Custom {
        access: BlockingDirectMemAccess::Read,
        start: BASE + 0x102,
        end: BASE + 0x105,
        read_latency: 0,
        write_latency: 0,
        null_pointer: false,
    });
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    // A load inside the granted range primes the cache.
    assert_eq!(
        read(&mut port, BASE + 0x104, PhysicalWidth::Halfword),
        vec![0x55, 0x66]
    );
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    let primed = target.transactions();

    // Four bytes from the grant's last two bytes cross its end.  It must stay
    // one transaction, never a split direct-plus-blocking pair.
    assert_eq!(
        read(&mut port, BASE + 0x104, PhysicalWidth::Word),
        vec![0x55, 0x66, 0x77, 0x88]
    );
    assert_eq!(
        target.transactions(),
        primed + 1,
        "a span crossing a grant end was served by the pointer"
    );
    assert_eq!(
        target.log().last().unwrap().length,
        4,
        "a span crossing a grant end was split"
    );
}

#[test]
fn a_load_through_a_write_only_grant_takes_b_transport() {
    let target = target();
    target.set_reply(DirectMemReply::Chunk(BlockingDirectMemAccess::Write));
    let slot = slot(&target);
    slot.program(0x100, &[0xa5; 8]).unwrap();
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE + 0x100, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    let primed = target.transactions();
    assert_eq!(
        read(&mut port, BASE + 0x100, PhysicalWidth::Word),
        vec![0xa5; 4]
    );
    assert_eq!(
        target.transactions(),
        primed + 1,
        "a load through a write-only grant used the pointer"
    );
}

#[test]
fn an_inbound_debug_access_never_uses_a_grant() {
    let target = target();
    let slot = slot(&target);
    slot.program(0x100, &[0xa5; 8]).unwrap();
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE + 0x100, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    let primed = target.transactions();
    let queries = target.queries();

    let request = PhysicalRequest::data_read(BASE + 0x100, PhysicalWidth::Doubleword).unwrap();
    let response = slot
        .raw(&request, 0x100, BlockingInitiator::DebugInbound)
        .unwrap();
    assert_eq!(response.read_bytes().unwrap(), &[0xa5; 8]);
    assert_eq!(
        target.transactions(),
        primed + 1,
        "a DebugInbound access was served by the pointer"
    );
    assert_eq!(
        target.queries(),
        queries,
        "a DebugInbound access produced a direct-memory query"
    );
}

// ---- D8/D9: invalidation, reset and unknown completion ----

#[test]
fn an_invalidation_inside_b_transport_takes_effect_before_the_next_access() {
    let target = target();
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE + 0x200, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    let primed = target.transactions();
    assert_eq!(read(&mut port, BASE + 0x204, PhysicalWidth::Word).len(), 4);
    assert_eq!(target.transactions(), primed, "the grant was not in use");

    // A debug-inbound access always takes b_transport and never queries, so
    // the invalidation the target performs inside it is the only thing that
    // can change the cache.  Inside `b_transport` is a legal moment for it.
    let weak = Arc::downgrade(&target);
    target.on_transport(Arc::new(move || {
        if let Some(target) = weak.upgrade() {
            target.invalidate_everything();
        }
    }));
    let request = PhysicalRequest::data_read(BASE + 0x300, PhysicalWidth::Word).unwrap();
    slot.raw(&request, 0x300, BlockingInitiator::DebugInbound)
        .unwrap();
    target.clear_transport_hook();
    assert_eq!(
        slot.cached_direct_mem_regions(),
        0,
        "an invalidated grant is still cached"
    );

    let after = target.transactions();
    assert_eq!(read(&mut port, BASE + 0x200, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        target.transactions(),
        after + 1,
        "the next access used a grant that was invalidated inside b_transport"
    );
    // That response carried the hint again, so a fresh grant exists.
    let regranted = target.transactions();
    assert_eq!(read(&mut port, BASE + 0x200, PhysicalWidth::Word).len(), 4);
    assert_eq!(target.transactions(), regranted);
}

#[test]
fn a_grant_raced_by_an_invalidation_is_discarded() {
    let target = target();
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    let once = Arc::new(AtomicBool::new(false));
    let weak = Arc::downgrade(&target);
    let fired = once.clone();
    target.on_query(Arc::new(move || {
        // Runs after the adapter sampled its epoch, so this invalidation
        // arrives while the query is in flight.
        if !fired.swap(true, Ordering::SeqCst) {
            if let Some(target) = weak.upgrade() {
                target.invalidate_everything();
            }
        }
    }));

    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        slot.cached_direct_mem_regions(),
        0,
        "a grant raced by an invalidation was installed"
    );
    let after = target.transactions();
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        target.transactions(),
        after + 1,
        "the discarded grant was somehow in use"
    );
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    target.clear_query_hook();
    let regranted = target.transactions();
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        target.transactions(),
        regranted,
        "the fresh grant was not used"
    );
}

#[test]
fn a_direct_copy_holds_the_direct_memory_lock() {
    let target = target();
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.cached_direct_mem_regions(), 1);

    // A direct copy that pauses between the grant lookup and the byte copy, so
    // the paused moment is exactly the window an invalidation could slip into.
    let (copying, copying_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    slot.set_direct_copy_hook(Some(Arc::new(move || {
        copying.send(()).unwrap();
        release_rx.lock().unwrap().recv().unwrap();
    })));
    let reader = {
        let mut reader_port = ValidatedPhysicalAccess::new(slot.clone());
        thread::spawn(move || read(&mut reader_port, BASE, PhysicalWidth::Word))
    };
    copying_rx.recv().unwrap();

    // A failed `try_lock` is the scheduler-independent observation: the copy
    // holds the direct-memory lock, so an `invalidate`, which takes that same
    // lock before it changes anything, cannot enter its protected section or
    // return while the copy is in flight (direct-memory D8).
    assert!(
        slot.direct_memory_lock_held(),
        "a direct copy did not hold the direct-memory lock"
    );
    release.send(()).unwrap();
    assert_eq!(reader.join().unwrap(), vec![0; 4]);
    slot.set_direct_copy_hook(None);
    assert!(
        !slot.direct_memory_lock_held(),
        "the direct-memory lock stayed held after the copy"
    );

    // An invalidation after the copy still takes effect.
    target.invalidate_everything();
    assert_eq!(
        slot.cached_direct_mem_regions(),
        0,
        "the invalidation after the copy did not remove the grant"
    );

    // A store copy takes the same lock, so an in-flight store is equally
    // protected from an invalidation that would free the storage behind it.
    target.set_reply(DirectMemReply::Chunk(BlockingDirectMemAccess::ReadWrite));
    write(&mut port, BASE + 0x108, &[1, 2, 3, 4]);
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    let (copying, copying_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    slot.set_direct_copy_hook(Some(Arc::new(move || {
        copying.send(()).unwrap();
        release_rx.lock().unwrap().recv().unwrap();
    })));
    let writer = {
        let mut writer_port = ValidatedPhysicalAccess::new(slot.clone());
        thread::spawn(move || write(&mut writer_port, BASE + 0x10c, &[5, 6, 7, 8]))
    };
    copying_rx.recv().unwrap();
    assert!(
        slot.direct_memory_lock_held(),
        "a direct store did not hold the direct-memory lock"
    );
    release.send(()).unwrap();
    writer.join().unwrap();
    slot.set_direct_copy_hook(None);
    assert!(
        !slot.direct_memory_lock_held(),
        "the direct-memory lock stayed held after the store copy"
    );
    assert_eq!(
        target.bytes_at(BASE + 0x10c, 4),
        [5, 6, 7, 8],
        "the store copy did not write through the grant"
    );
}

#[test]
fn the_adapter_drops_its_own_grants_before_the_target_resets() {
    let target = target();
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    // This target keeps its registrations across a reset, so only the
    // adapter's own drop can remove the grant before the target discards the
    // storage it points at.
    target.set_reset_invalidates(false);
    slot.begin_window().unwrap();
    assert_eq!(
        slot.cached_direct_mem_regions(),
        0,
        "a grant survived the adapter's transport reset"
    );
    let after = target.transactions();
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        target.transactions(),
        after + 1,
        "the adapter used a grant that outlived the reset"
    );
}

#[test]
fn an_unknown_completion_drops_every_grant() {
    let target = target();
    let slot = slot(&target);
    slot.program(0x100, &[0x11; 8]).unwrap();
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE + 0x100, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.cached_direct_mem_regions(), 1);

    // The next access is outside the grant, so it takes b_transport and fails
    // as an unknown completion.
    target.set_unknown(true);
    let request = PhysicalRequest::data_read(BASE + 0x1100, PhysicalWidth::Word).unwrap();
    let error = port.access(request).unwrap_err();
    assert!(matches!(error, PhysicalAccessError::UnknownCompletion(_)));
    assert_eq!(
        slot.cached_direct_mem_regions(),
        0,
        "a grant survived an unknown completion"
    );

    target.set_unknown(false);
    let transactions = target.transactions();
    let request = PhysicalRequest::data_read(BASE + 0x100, PhysicalWidth::Word).unwrap();
    let error = port.access(request).unwrap_err();
    assert!(
        matches!(error, PhysicalAccessError::UnknownCompletion(_)),
        "a poisoned transport served a cached grant: {error:?}"
    );
    assert_eq!(
        target.transactions(),
        transactions,
        "the poisoned transport issued a transaction"
    );
}

#[test]
fn every_registration_is_kept_and_invalidation_reaches_all_of_them() {
    let target = target();
    let first = slot(&target);
    assert_eq!(target.registrations(), 1);
    {
        let second = slot(&target);
        assert_eq!(
            target.registrations(),
            2,
            "a later registration replaced an earlier one"
        );
        let mut port = ValidatedPhysicalAccess::new(first.clone());
        assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
        assert_eq!(first.cached_direct_mem_regions(), 1);

        // The second composition's reset must remove the first one's grant.
        let before = target.transactions();
        second.begin_window().unwrap();
        assert_eq!(
            first.cached_direct_mem_regions(),
            0,
            "another composition's reset left this grant live"
        );
        let request = PhysicalRequest::data_read(BASE, PhysicalWidth::Word).unwrap();
        assert!(port.access(request).is_ok());
        assert_eq!(
            target.transactions(),
            before + 1,
            "the first composition used a grant the second reset should have removed"
        );
    }
    // A released adapter's registration is inert and is pruned.
    target.invalidate_everything();
    assert_eq!(target.registrations(), 1);
}

// ---- D4/D6: bounded cache and latency accounting ----

#[test]
fn a_new_grant_replaces_every_overlapping_cached_grant() {
    let target = target();
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    // Two disjoint grants, each covering a small part of the first chunk.
    for start in [BASE + 0x100, BASE + 0x200] {
        target.set_reply(DirectMemReply::Custom {
            access: BlockingDirectMemAccess::Read,
            start,
            end: start + 0x7f,
            read_latency: 0,
            write_latency: 0,
            null_pointer: false,
        });
        assert_eq!(read(&mut port, start, PhysicalWidth::Word).len(), 4);
    }
    assert_eq!(
        slot.cached_direct_mem_regions(),
        2,
        "the two disjoint grants did not both stay cached"
    );

    // A wider grant overlaps both of them, so the cache keeps exactly that one.
    target.set_reply(DirectMemReply::Custom {
        access: BlockingDirectMemAccess::Read,
        start: BASE,
        end: BASE + 0xfff,
        read_latency: 0,
        write_latency: 0,
        null_pointer: false,
    });
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        slot.cached_direct_mem_regions(),
        1,
        "a new grant did not replace every overlapping cached grant"
    );
    let replaced = target.transactions();
    for start in [BASE + 0x100, BASE + 0x200, BASE] {
        assert_eq!(read(&mut port, start, PhysicalWidth::Word).len(), 4);
    }
    assert_eq!(
        target.transactions(),
        replaced,
        "the wider replacement grant is not in use"
    );
}

#[test]
fn the_grant_cache_is_bounded_and_eviction_only_costs_transactions() {
    let target = DirectMemTarget::new(BASE, 0xa000);
    let slot = RamSlot::tlm(target.clone(), BASE, 0xa000, TickPeriod::DEFAULT, true).unwrap();
    slot.begin_window().unwrap();
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    let chunks = DIRECT_MEM_REGION_LIMIT + 1;
    for chunk in 0..chunks {
        let address = BASE + (chunk as u64 * 0x1000);
        assert_eq!(read(&mut port, address, PhysicalWidth::Word).len(), 4);
        assert_eq!(target.queries(), chunk + 1);
    }
    assert_eq!(
        slot.cached_direct_mem_regions(),
        DIRECT_MEM_REGION_LIMIT,
        "the grant cache is not bounded"
    );
    // The oldest grant was evicted: its chunk takes one transaction again.
    let before = target.transactions();
    let address = BASE;
    assert_eq!(read(&mut port, address, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        target.transactions(),
        before + 1,
        "an evicted grant was in use"
    );
    // The newest grant is still live.
    let newest = BASE + ((chunks - 1) as u64 * 0x1000);
    let after = target.transactions();
    assert_eq!(read(&mut port, newest, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        target.transactions(),
        after,
        "the newest grant was evicted by the read"
    );
}

#[test]
fn the_in_tree_target_invalidates_before_it_discards_a_granted_chunk() {
    let target = Arc::new(ZeroDelayRam::new());
    let slot = RamSlot::tlm(target.clone(), BASE, SIZE, TickPeriod::DEFAULT, true).unwrap();
    slot.begin_window().unwrap();
    slot.program(0x100, &[0x5a; 4]).unwrap();
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(
        read(&mut port, BASE + 0x100, PhysicalWidth::Word),
        vec![0x5a; 4]
    );
    assert_eq!(slot.cached_direct_mem_regions(), 1);

    // A second composition's reset reaches every registration before the
    // in-tree target discards the chunk the grant points at.
    let other = RamSlot::tlm(target.clone(), BASE, SIZE, TickPeriod::DEFAULT, true).unwrap();
    other.begin_window().unwrap();
    assert_eq!(
        slot.cached_direct_mem_regions(),
        0,
        "an in-tree target reset left another composition's grant live"
    );

    // The next access takes b_transport and sees the zero-filled chunk, so the
    // old grant cannot have served it.
    assert_eq!(
        read(&mut port, BASE + 0x100, PhysicalWidth::Word),
        vec![0; 4],
        "the access after the reset still saw the removed grant's bytes"
    );
}

#[test]
fn a_direct_read_charges_its_grant_latency_through_the_one_accounting_path() {
    let target = target();
    target.set_declared_max_delay(MAX_DELAY);
    target.set_delay(MAX_DELAY);
    target.set_grant_latency(MAX_DELAY, MAX_DELAY);
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());

    // One annotated response and one granted latency are accounted the same
    // way: exact femtoseconds, rounded ticks, and the same boundary total.
    slot.begin_transport_turn();
    assert_eq!(read(&mut port, BASE + 0x100, PhysicalWidth::Word).len(), 4);
    let blocking = slot.take_transport_turn_delay();
    assert_eq!(blocking.femtoseconds, Femtoseconds::new(MAX_DELAY));
    assert_eq!(blocking.ticks, 1);
    assert_eq!(slot.cached_direct_mem_regions(), 1);

    let transactions = target.transactions();
    slot.begin_transport_turn();
    assert_eq!(read(&mut port, BASE + 0x104, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        target.transactions(),
        transactions,
        "the second read was not direct"
    );
    let direct = slot.take_transport_turn_delay();
    assert_eq!(
        direct, blocking,
        "a direct read charged a different delay than the annotated one"
    );
    assert_eq!(slot.consumed_delay(), Femtoseconds::new(MAX_DELAY * 2));

    // Outside a guest turn the latency is validated but not charged, exactly
    // as a debug-work annotation.
    assert_eq!(read(&mut port, BASE + 0x108, PhysicalWidth::Word).len(), 4);
    assert_eq!(
        slot.consumed_delay(),
        Femtoseconds::new(MAX_DELAY * 2),
        "a direct read outside a guest turn charged Platform time"
    );
}

// ---- D4/D5: the direct store path ----

/// A target that answers every query with a read-write chunk grant.
fn grant_read_write(target: &Arc<DirectMemTarget>) {
    target.set_reply(DirectMemReply::Chunk(BlockingDirectMemAccess::ReadWrite));
}

#[test]
fn a_cached_grant_serves_stores_without_b_transport() {
    let target = target();
    grant_read_write(&target);
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());

    // The first access takes b_transport: the hint arrives with its response,
    // and only then may the adapter query.
    assert_eq!(
        read(&mut port, BASE + 0x100, PhysicalWidth::Word),
        vec![0; 4]
    );
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    assert_eq!(
        slot.committed_write_version(),
        Some(0),
        "a load was recorded as a committed write"
    );
    let primed = target.transactions();

    write(&mut port, BASE + 0x100, &[1, 2, 3, 4]);
    assert_eq!(
        target.transactions(),
        primed,
        "a covered store took b_transport"
    );
    assert_eq!(
        target.bytes_at(BASE + 0x100, 4),
        [1, 2, 3, 4],
        "the direct store did not reach the target's storage"
    );
    assert_eq!(
        slot.committed_write_version(),
        Some(1),
        "a direct store was left out of the committed-write record"
    );

    // A later direct load reads the same bytes the direct store wrote, and a
    // fetch does too: the pointer and b_transport serve one copy.
    assert_eq!(
        read(&mut port, BASE + 0x100, PhysicalWidth::Word),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        fetch(&mut port, BASE + 0x100, PhysicalWidth::Word),
        vec![1, 2, 3, 4]
    );
    assert_eq!(target.transactions(), primed);
    // The store did not query the target again.
    assert_eq!(target.queries(), 1);
}

#[test]
fn a_store_span_crossing_a_grant_end_stays_one_transaction() {
    let target = target();
    grant_read_write(&target);
    target.set_reply(DirectMemReply::Custom {
        access: BlockingDirectMemAccess::ReadWrite,
        start: BASE + 0x102,
        end: BASE + 0x105,
        read_latency: 0,
        write_latency: 0,
        null_pointer: false,
    });
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    // A store inside the granted range primes the cache.
    write(&mut port, BASE + 0x104, &[0x11, 0x22]);
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    let primed = target.transactions();

    // Four bytes from the grant's last two bytes cross its end: one
    // transaction, never a split direct-plus-blocking pair.
    write(&mut port, BASE + 0x104, &[0x11, 0x22, 0x33, 0x44]);
    assert_eq!(
        target.transactions(),
        primed + 1,
        "a store span crossing a grant end was served by the pointer"
    );
    assert_eq!(
        target.log().last().unwrap().length,
        4,
        "a store span crossing a grant end was split"
    );
    assert_eq!(target.bytes_at(BASE + 0x104, 4), [0x11, 0x22, 0x33, 0x44]);
}

#[test]
fn a_store_to_a_read_only_region_reaches_the_target() {
    let target = target();
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    // The default answer is a read-only chunk grant.
    assert_eq!(read(&mut port, BASE + 0x100, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.cached_direct_mem_regions(), 1);

    let primed = target.transactions();
    write(&mut port, BASE + 0x100, &[9, 9, 9, 9]);
    assert_eq!(
        target.transactions(),
        primed + 1,
        "a store to a read-only region was served by the pointer"
    );
    assert_eq!(
        target.log().last().unwrap().command,
        BlockingCommand::Write,
        "the store did not reach the target as its own transaction"
    );
    assert_eq!(target.bytes_at(BASE + 0x100, 4), [9, 9, 9, 9]);
    // The hint on that completed store licensed a write query (direct-memory
    // D3), so the target now decides the direction it offers.
    assert_eq!(
        target.last_query().unwrap().access(),
        BlockingDirectMemAccess::Write,
        "the store's hint was queried for the wrong access direction"
    );

    // A read-write grant replaces the read-only region it overlaps, and the
    // next store uses it (direct-memory D5).
    grant_read_write(&target);
    let primed = target.transactions();
    write(&mut port, BASE + 0x110, &[7, 7, 7, 7]);
    assert_eq!(target.transactions(), primed + 1);
    assert_eq!(
        slot.cached_direct_mem_regions(),
        1,
        "the read-write grant did not replace the read-only region"
    );
    let after = target.transactions();
    write(&mut port, BASE + 0x114, &[8, 8, 8, 8]);
    assert_eq!(
        target.transactions(),
        after,
        "the read-write replacement grant was not used"
    );
    assert_eq!(target.bytes_at(BASE + 0x114, 4), [8, 8, 8, 8]);
}

#[test]
fn a_direct_store_charges_its_write_latency_through_the_one_accounting_path() {
    // Two ticks of annotated delay and a one-tick read latency, so a direct
    // store that charged the wrong direction would be visible.
    const WRITE_DELAY: u64 = 2_000_000;
    const READ_DELAY: u64 = 1_000_000;
    let target = target();
    grant_read_write(&target);
    target.set_declared_max_delay(WRITE_DELAY);
    target.set_delay(WRITE_DELAY);
    target.set_grant_latency(READ_DELAY, WRITE_DELAY);
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());

    // One annotated response and one granted write latency are accounted the
    // same way: exact femtoseconds, rounded ticks, the same boundary total.
    slot.begin_transport_turn();
    write(&mut port, BASE + 0x100, &[1, 2, 3, 4]);
    let blocking = slot.take_transport_turn_delay();
    assert_eq!(blocking.femtoseconds, Femtoseconds::new(WRITE_DELAY));
    assert_eq!(blocking.ticks, 2);
    assert_eq!(slot.cached_direct_mem_regions(), 1);

    let transactions = target.transactions();
    slot.begin_transport_turn();
    write(&mut port, BASE + 0x104, &[5, 6, 7, 8]);
    assert_eq!(
        target.transactions(),
        transactions,
        "the second store was not direct"
    );
    let direct = slot.take_transport_turn_delay();
    assert_eq!(
        direct, blocking,
        "a direct store charged a different delay than the annotated one"
    );
    assert_eq!(
        direct.femtoseconds,
        Femtoseconds::new(WRITE_DELAY),
        "a direct store charged the read latency instead of the write latency"
    );
    assert_eq!(slot.consumed_delay(), Femtoseconds::new(WRITE_DELAY * 2));

    // Outside a guest turn the latency is validated but not charged, exactly
    // as a debug-work annotation.
    write(&mut port, BASE + 0x108, &[9, 9, 9, 9]);
    assert_eq!(
        slot.consumed_delay(),
        Femtoseconds::new(WRITE_DELAY * 2),
        "a direct store outside a guest turn charged Platform time"
    );
}

/// D6: a direct store charges the grant's *write* latency, not its read
/// latency.  The two differ here so a wrong-direction charge cannot hide.
#[test]
fn a_direct_store_charges_the_write_latency_not_the_read_latency() {
    const WRITE_DELAY: u64 = 2_000_000;
    const READ_DELAY: u64 = 1_000_000;
    let target = target();
    grant_read_write(&target);
    target.set_declared_max_delay(WRITE_DELAY);
    target.set_grant_latency(READ_DELAY, WRITE_DELAY);
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());

    // The first store's own b_transport obtains the grant and annotates no
    // delay, so the direct store's charge is the only one in its turn.
    write(&mut port, BASE + 0x100, &[1, 2, 3, 4]);
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    let transactions = target.transactions();
    slot.begin_transport_turn();
    write(&mut port, BASE + 0x104, &[5, 6, 7, 8]);
    assert_eq!(
        target.transactions(),
        transactions,
        "the second store was not direct"
    );
    let direct = slot.take_transport_turn_delay();
    assert_eq!(
        direct.femtoseconds,
        Femtoseconds::new(WRITE_DELAY),
        "a direct store charged {} fs, not its granted {WRITE_DELAY} fs write latency",
        direct.femtoseconds.as_u64()
    );
    assert_eq!(direct.ticks, 2);
}

/// D7: a reported store-conditional success that the validated boundary must
/// reject did not complete, so it leaves the write-suspension gate open.
#[test]
fn a_rejected_store_conditional_success_leaves_the_gate_open() {
    let target = target();
    let slot = prime(&target);
    let mut phys = ValidatedPhysicalAccess::new(slot.clone());
    let mut atomics = ValidatedAtomicAccess::new(slot.clone());

    // A guest load-reserved at 0x100 opens the gate and defines the
    // reservation the Hart would carry into a store-conditional.
    let (_, context) = load_reserved(&mut atomics, BASE + 0x100);

    // The target claims success for a span outside that reservation.  The
    // validated boundary rejects it as a protocol failure of the envelope.
    target.misreport_next_store_conditional_success();
    let request = AtomicRequest::store_conditional(
        BASE + 0x180,
        PhysicalWidth::Doubleword,
        ORDERING,
        &[0u8; 8],
        Some(context),
    )
    .unwrap();
    let error = atomics.access_atomic(request).unwrap_err();
    assert!(
        matches!(
            error,
            crate::physical::AtomicAccessError::Protocol(
                crate::physical::AtomicProtocolError::ConditionalSuccessOutsideReservation { .. }
            )
        ),
        "an uncovered conditional success was not a protocol failure: {error:?}"
    );
    let suspended = target.transactions();
    write(&mut phys, BASE + 0x100, &[1, 2, 3, 4]);
    assert_eq!(
        target.transactions(),
        suspended + 1,
        "a protocol-failed store-conditional closed the write-suspension gate"
    );

    // The same holds for a success the boundary rejects for want of any
    // reservation context.
    target.misreport_next_store_conditional_success();
    let request = AtomicRequest::store_conditional(
        BASE + 0x100,
        PhysicalWidth::Doubleword,
        ORDERING,
        &[0u8; 8],
        Option::<AtomicReservationContext>::None,
    )
    .unwrap();
    let error = atomics.access_atomic(request).unwrap_err();
    assert!(
        matches!(
            error,
            crate::physical::AtomicAccessError::Protocol(
                crate::physical::AtomicProtocolError::ConditionalSuccessWithoutReservation
            )
        ),
        "a conditional success without reservation context was not a protocol failure: {error:?}"
    );
    let suspended = target.transactions();
    write(&mut phys, BASE + 0x108, &[5, 6, 7, 8]);
    assert_eq!(
        target.transactions(),
        suspended + 1,
        "a success without reservation context closed the write-suspension gate"
    );
}

#[test]
fn a_direct_store_is_recorded_as_a_committed_write() {
    let target = target();
    grant_read_write(&target);
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE + 0x100, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.committed_write_version(), Some(0));

    let primed = target.transactions();
    write(&mut port, BASE + 0x100, &[1, 2, 3, 4]);
    assert_eq!(target.transactions(), primed, "the store was not direct");
    assert_eq!(
        slot.committed_write_version(),
        Some(1),
        "a direct store was left out of the committed-write record"
    );
    write(&mut port, BASE + 0x104, &[5, 6, 7, 8]);
    assert_eq!(
        slot.committed_write_version(),
        Some(2),
        "a second direct store was left out of the committed-write record"
    );
}

// ---- D7: the write-suspension gate ----

fn prime(target: &Arc<DirectMemTarget>) -> RamSlot {
    grant_read_write(target);
    let slot = slot(target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE + 0x100, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.cached_direct_mem_regions(), 1);
    slot
}

fn load_reserved(
    port: &mut ValidatedAtomicAccess<RamSlot>,
    address: u64,
) -> (Vec<u8>, AtomicReservationContext) {
    let request =
        AtomicRequest::load_reserved(address, PhysicalWidth::Doubleword, ORDERING).unwrap();
    let response = port.access_atomic(request).unwrap();
    (
        response.old_bytes().unwrap().to_vec(),
        AtomicReservationContext {
            reserved: PhysicalSpan {
                paddr: address,
                width: PhysicalWidth::Doubleword,
            },
            snapshot: response.snapshot().expect("load-reserved snapshot"),
        },
    )
}

#[test]
fn a_guest_load_reserved_suspends_direct_stores_until_a_conclusive_store_conditional() {
    let target = target();
    let slot = prime(&target);
    let mut phys = ValidatedPhysicalAccess::new(slot.clone());
    let mut atomics = ValidatedAtomicAccess::new(slot.clone());

    // A guest load-reserved opens the gate: its reservation's store-conditional
    // must observe every later store through the target.
    let (_, context) = load_reserved(&mut atomics, BASE + 0x100);
    let suspended = target.transactions();
    write(&mut phys, BASE + 0x100, &[0x11, 0x22, 0x33, 0x44]);
    assert_eq!(
        target.transactions(),
        suspended + 1,
        "a store after a guest load-reserved was served by the pointer"
    );
    // The gate is deliberately coarse: every store is suspended, not just an
    // overlapping one.
    write(&mut phys, BASE + 0x300, &[0x55, 0x66, 0x77, 0x88]);
    assert_eq!(
        target.transactions(),
        suspended + 2,
        "the gate let a non-overlapping store through the pointer"
    );

    // The store committed a version to the reserved span, so the reservation
    // fails; that conclusive completion closes the gate.
    let request = AtomicRequest::store_conditional(
        BASE + 0x100,
        PhysicalWidth::Doubleword,
        ORDERING,
        &[0u8; 8],
        Some(context),
    )
    .unwrap();
    let response = atomics.access_atomic(request).unwrap();
    assert_eq!(
        response.conditional_status(),
        Some(ConditionalStatus::Failure),
        "a suspended store did not reach the reservation check"
    );

    let reopened = target.transactions();
    write(&mut phys, BASE + 0x108, &[1, 1, 1, 1]);
    assert_eq!(
        target.transactions(),
        reopened,
        "a conclusive store-conditional did not reopen the direct store path"
    );
    assert_eq!(target.bytes_at(BASE + 0x108, 4), [1, 1, 1, 1]);
}

#[test]
fn a_faulted_store_conditional_leaves_the_gate_open() {
    let target = target();
    let slot = prime(&target);
    let mut phys = ValidatedPhysicalAccess::new(slot.clone());
    let mut atomics = ValidatedAtomicAccess::new(slot.clone());

    let (_, context) = load_reserved(&mut atomics, BASE + 0x100);
    // The target rejects the store-conditional: the Hart keeps its reservation.
    target.fault_next_store_conditional();
    let request = AtomicRequest::store_conditional(
        BASE + 0x100,
        PhysicalWidth::Doubleword,
        ORDERING,
        &[0u8; 8],
        Some(context),
    )
    .unwrap();
    let error = atomics.access_atomic(request).unwrap_err();
    assert!(
        matches!(error, crate::physical::AtomicAccessError::TargetRejected(_)),
        "a rejected store-conditional was not a target fault: {error:?}"
    );

    let suspended = target.transactions();
    write(&mut phys, BASE + 0x100, &[4, 4, 4, 4]);
    assert_eq!(
        target.transactions(),
        suspended + 1,
        "a store after a faulted store-conditional was served by the pointer"
    );
    assert_eq!(target.bytes_at(BASE + 0x100, 4), [4, 4, 4, 4]);
}

#[test]
fn a_page_table_update_load_reserved_does_not_open_the_gate() {
    let target = target();
    let slot = prime(&target);
    let mut phys = ValidatedPhysicalAccess::new(slot.clone());
    let mut atomics = ValidatedAtomicAccess::new(slot.clone());

    let request = AtomicRequest::load_reserved(BASE + 0x140, PhysicalWidth::Word, ORDERING)
        .unwrap()
        .with_page_table_update();
    atomics.access_atomic(request).unwrap();
    assert_eq!(target.atomics(), 1, "the A/D leg did not reach the target");

    let after = target.transactions();
    write(&mut phys, BASE + 0x140, &[5, 6, 7, 8]);
    assert_eq!(
        target.transactions(),
        after,
        "an A/D load-reserved opened the write-suspension gate"
    );
    assert_eq!(target.bytes_at(BASE + 0x140, 4), [5, 6, 7, 8]);
}

#[test]
fn a_page_table_update_store_conditional_does_not_close_the_gate() {
    let target = target();
    let slot = prime(&target);
    let mut phys = ValidatedPhysicalAccess::new(slot.clone());
    let mut atomics = ValidatedAtomicAccess::new(slot.clone());

    // A guest load-reserved opens the gate.
    load_reserved(&mut atomics, BASE + 0x160);
    // A walk-private A/D pair concludes successfully over a covered grant; it
    // is not the Hart's reservation, so it must leave the gate open.
    let lr = AtomicRequest::load_reserved(BASE + 0x168, PhysicalWidth::Doubleword, ORDERING)
        .unwrap()
        .with_page_table_update();
    let loaded = atomics.access_atomic(lr).unwrap();
    let context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: BASE + 0x168,
            width: PhysicalWidth::Doubleword,
        },
        snapshot: loaded.snapshot().expect("A/D load-reserved snapshot"),
    };
    let sc = AtomicRequest::store_conditional(
        BASE + 0x168,
        PhysicalWidth::Doubleword,
        ORDERING,
        &[0x5a; 8],
        Some(context),
    )
    .unwrap()
    .with_page_table_update();
    let stored = atomics.access_atomic(sc).unwrap();
    assert_eq!(
        stored.conditional_status(),
        Some(ConditionalStatus::Success),
        "the A/D store-conditional did not conclude"
    );

    let after = target.transactions();
    write(&mut phys, BASE + 0x160, &[1, 2, 3, 4]);
    assert_eq!(
        target.transactions(),
        after + 1,
        "an A/D store-conditional closed the write-suspension gate"
    );
}

// ---- D7: every atomic envelope is one transaction over a covered grant ----

#[test]
fn every_atomic_envelope_is_one_transaction_over_a_covered_grant() {
    let target = target();
    let slot = prime(&target);
    let mut atomics = ValidatedAtomicAccess::new(slot.clone());
    let queries = target.queries();

    // A guest load-reserved.
    let mut transactions = target.transactions();
    let (_, context) = load_reserved(&mut atomics, BASE + 0x100);
    assert_eq!(
        target.transactions(),
        transactions + 1,
        "a load-reserved over a covered grant was served by the pointer"
    );

    // A guest store-conditional: one transaction whatever its conditional
    // status, because the target decides it inside that same call.
    transactions = target.transactions();
    let request = AtomicRequest::store_conditional(
        BASE + 0x100,
        PhysicalWidth::Doubleword,
        ORDERING,
        &[1u8; 8],
        Some(context),
    )
    .unwrap();
    atomics.access_atomic(request).unwrap();
    assert_eq!(
        target.transactions(),
        transactions + 1,
        "a store-conditional over a covered grant was served by the pointer"
    );

    // An AMO.
    transactions = target.transactions();
    let operand = 1u32.to_le_bytes();
    let request = AtomicRequest::rmw(
        BASE + 0x120,
        PhysicalWidth::Word,
        ORDERING,
        &operand,
        crate::hart_amods::transform(crate::hart_amods::AmoOperation::Add, AmoWidth::Word),
    )
    .unwrap();
    atomics.access_atomic(request).unwrap();
    assert_eq!(
        target.transactions(),
        transactions + 1,
        "an AMO over a covered grant was served by the pointer"
    );

    // Each leg of an A/D update.
    let payload = [0u8; 8];
    transactions = target.transactions();
    let lr = AtomicRequest::load_reserved(BASE + 0x130, PhysicalWidth::Doubleword, ORDERING)
        .unwrap()
        .with_page_table_update();
    let loaded = atomics.access_atomic(lr).unwrap();
    assert_eq!(
        target.transactions(),
        transactions + 1,
        "an A/D load-reserved over a covered grant was served by the pointer"
    );
    let context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: BASE + 0x130,
            width: PhysicalWidth::Doubleword,
        },
        snapshot: loaded.snapshot().expect("A/D load-reserved snapshot"),
    };
    transactions = target.transactions();
    let sc = AtomicRequest::store_conditional(
        BASE + 0x130,
        PhysicalWidth::Doubleword,
        ORDERING,
        &payload,
        Some(context),
    )
    .unwrap()
    .with_page_table_update();
    atomics.access_atomic(sc).unwrap();
    assert_eq!(
        target.transactions(),
        transactions + 1,
        "an A/D store-conditional over a covered grant was served by the pointer"
    );

    assert_eq!(
        target.atomics(),
        5,
        "an atomic envelope did not reach the target as its own transaction"
    );
    // An atomic completion is never asked for a grant.
    assert_eq!(
        target.queries(),
        queries,
        "an atomic envelope produced a direct-memory query"
    );
    assert_eq!(slot.cached_direct_mem_regions(), 1);
}

// ---- D13: the in-tree target ----

fn data_write(address: u64, payload: &mut [u8]) -> BlockingTransaction<'_> {
    BlockingTransaction::new(
        address,
        BlockingCommand::Write,
        payload,
        BlockingExtension::new(PhysicalAccessKind::DataWrite, BlockingInitiator::Hart),
    )
}

#[test]
fn zero_delay_grants_one_read_write_chunk_with_zero_latency() {
    let target = ZeroDelayRam::new();
    let query = BlockingDirectMemQuery::new(BASE + 0x123, 4, BlockingDirectMemAccess::Read);
    assert_eq!(
        target.get_direct_mem_ptr(query),
        BlockingDirectMemResponse::Denied,
        "an unwritten chunk was granted"
    );

    let mut payload = [0u8; 8];
    let mut transaction = data_write(BASE + 0x100, &mut payload);
    target.b_transport(&mut transaction);
    assert_eq!(transaction.status(), BlockingResponseStatus::Ok);
    assert!(
        transaction.is_dmi_allowed(),
        "a completed non-atomic transaction carried no hint"
    );

    let BlockingDirectMemResponse::Granted(grant) = target.get_direct_mem_ptr(query) else {
        panic!("an allocated chunk was not granted");
    };
    assert_eq!(grant.start(), BASE);
    assert_eq!(grant.end(), BASE + 0xfff);
    assert_eq!(grant.access(), BlockingDirectMemAccess::ReadWrite);
    assert_eq!(grant.read_latency(), Femtoseconds::ZERO);
    assert_eq!(grant.write_latency(), Femtoseconds::ZERO);
    assert_eq!(
        Some((grant.start(), grant.end(), grant.pointer())),
        target.chunk_pointer(BASE + 0x123),
        "the grant does not point at the chunk b_transport serves"
    );

    // A store through the read-write grant lands in the same atomics
    // `b_transport` and debug transport serve (direct-memory D10/D13): the
    // very byte a later blocking read returns.
    // SAFETY: the target issued this pointer over its own live chunk and has
    // not invalidated it.
    unsafe {
        AtomicU8::from_ptr(grant.pointer().add(0x104)).store(0x7e, Ordering::Relaxed);
    }
    let mut read = [0u8; 1];
    let mut transaction = BlockingTransaction::new(
        BASE + 0x104,
        BlockingCommand::Read,
        &mut read,
        BlockingExtension::new(PhysicalAccessKind::DataRead, BlockingInitiator::Hart),
    );
    target.b_transport(&mut transaction);
    assert_eq!(
        transaction.data(),
        [0x7e],
        "a store through the pointer is not visible to b_transport"
    );
    assert_eq!(target.bytes_at(BASE + 0x104, 1), [0x7e]);

    // An atomic transaction is never hinted.
    let mut bytes = [0u8; 8];
    let mut transaction = BlockingTransaction::new(
        BASE + 0x100,
        BlockingCommand::Read,
        &mut bytes,
        BlockingExtension::atomic(
            PhysicalAccessKind::DataRead,
            BlockingInitiator::Hart,
            BlockingAtomicExtension::load_reserved(
                AmoWidth::Doubleword,
                AtomicOrdering {
                    aq: false,
                    rl: false,
                },
            ),
        ),
    );
    target.b_transport(&mut transaction);
    assert!(
        !transaction.is_dmi_allowed(),
        "an atomic transaction was hinted"
    );
}

#[test]
fn the_committed_write_clock_survives_a_transport_reset() {
    let target = Arc::new(ZeroDelayRam::new());
    let slot = RamSlot::tlm(target.clone(), BASE, SIZE, TickPeriod::DEFAULT, false).unwrap();
    slot.begin_window().unwrap();
    slot.program(0x40, &0x1111_2222_3333_4444u64.to_le_bytes())
        .unwrap();
    let mut port = ValidatedAtomicAccess::new(slot.clone());
    let (old, context) = lr_context(&mut port, BASE + 0x40);
    assert_eq!(old, 0x1111_2222_3333_4444u64.to_le_bytes());

    // The same reinstall sequence a reset performs: reset, zero-fill,
    // placement.  A version must not repeat across it.
    slot.begin_window().unwrap();
    slot.program(0x40, &0x1111_2222_3333_4444u64.to_le_bytes())
        .unwrap();
    assert_eq!(
        store_conditional(
            &mut port,
            BASE + 0x40,
            PhysicalWidth::Doubleword,
            &0xaaaa_bbbb_cccc_ddddu64.to_le_bytes(),
            context
        ),
        ConditionalStatus::Failure,
        "a committed-write version repeated across a transport reset"
    );
    assert_eq!(
        slot.host_read(0x40, 8).unwrap(),
        0x1111_2222_3333_4444u64.to_le_bytes(),
        "a failing store-conditional wrote the bytes"
    );
}

fn lr_context(
    port: &mut ValidatedAtomicAccess<RamSlot>,
    address: u64,
) -> (Vec<u8>, AtomicReservationContext) {
    let request =
        AtomicRequest::load_reserved(address, PhysicalWidth::Doubleword, ORDERING).unwrap();
    let response = port.access_atomic(request).unwrap();
    (
        response.old_bytes().unwrap().to_vec(),
        AtomicReservationContext {
            reserved: PhysicalSpan {
                paddr: address,
                width: PhysicalWidth::Doubleword,
            },
            snapshot: response.snapshot().expect("load-reserved snapshot"),
        },
    )
}

fn store_conditional(
    port: &mut ValidatedAtomicAccess<RamSlot>,
    address: u64,
    width: PhysicalWidth,
    payload: &[u8],
    reservation: AtomicReservationContext,
) -> ConditionalStatus {
    let request =
        AtomicRequest::store_conditional(address, width, ORDERING, payload, Some(reservation))
            .unwrap();
    port.access_atomic(request)
        .unwrap()
        .conditional_status()
        .expect("store-conditional status")
}
