# Heterogeneous Platform Directions

**Status:** Research

**Authority:** Informational; no accelerator is on the active roadmap

The archived NPU and heterogeneous studies identify reusable platform concerns even though their concrete SystemBus design and schedules are obsolete.

## Retained concerns

- Accelerators may be MMIO targets and DMA-capable bus initiators.
- DMA requires an explicit master-side physical-access contract rather than a back-reference into CPU memory internals.
- Completion signaling belongs in platform interrupt wiring.
- Shared memory requires an explicit coherency model; “same host buffer” is not a coherency specification.
- Functional, loosely timed, and performance-oriented accelerator models must advertise their abstraction level.
- Task scheduling and host acceleration are implementation choices behind the device model.

## Deferred decisions

- Coherent versus non-coherent DMA.
- IOMMU and address-space ownership.
- Accelerator command ABI and register map.
- Custom instructions versus pure MMIO control.
- Multi-initiator arbitration and virtual-time behavior.

Original studies are available in [`../archive/research/`](../archive/research/).
