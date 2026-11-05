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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

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

    // A store always takes b_transport in this unit; inside it the target
    // invalidates the granted range, which is legal at any time.
    let weak = Arc::downgrade(&target);
    target.on_transport(Arc::new(move || {
        if let Some(target) = weak.upgrade() {
            target.invalidate_everything();
        }
    }));
    write(&mut port, BASE + 0x300, &[1, 2, 3, 4]);
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
fn an_invalidation_waits_for_a_copy_that_is_in_flight() {
    let target = target();
    let slot = slot(&target);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(read(&mut port, BASE, PhysicalWidth::Word).len(), 4);
    assert_eq!(slot.cached_direct_mem_regions(), 1);

    // A direct copy that holds the direct-memory lock until the test releases
    // it.  The pause point sits between the grant lookup and the byte copy, so
    // this proves that window: an invalidation cannot slip between them.
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

    // The copy is released only after a delay, so the assertion below reads a
    // definite ordering rather than a timing window: a lock that the copy still
    // holds cannot let this call return before that release.
    let released = Arc::new(AtomicBool::new(false));
    let releaser = {
        let released = released.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            released.store(true, Ordering::SeqCst);
            release.send(()).unwrap();
        })
    };
    target.invalidate_everything();
    assert!(
        released.load(Ordering::SeqCst),
        "an invalidation returned while a direct copy was in flight"
    );
    assert_eq!(reader.join().unwrap(), vec![0; 4]);
    releaser.join().unwrap();
    slot.set_direct_copy_hook(None);
    assert_eq!(
        slot.cached_direct_mem_regions(),
        0,
        "the invalidation that waited for the copy did not remove the grant"
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
fn zero_delay_grants_one_read_only_chunk_with_zero_latency() {
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
    assert_eq!(grant.access(), BlockingDirectMemAccess::Read);
    assert_eq!(grant.read_latency(), Femtoseconds::ZERO);
    assert_eq!(grant.write_latency(), Femtoseconds::ZERO);
    assert_eq!(
        Some((grant.start(), grant.end(), grant.pointer())),
        target.chunk_pointer(BASE + 0x123),
        "the grant does not point at the chunk b_transport serves"
    );

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
