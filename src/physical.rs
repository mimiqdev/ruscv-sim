//! Transport-neutral physical-access vocabulary: the A7 non-atomic seam and
//! the A8 atomic operation envelope.
//!
//! This module describes one complete fetch, data read, or data write using a
//! physical address, an explicit byte width, and raw bytes.  Since A8 T1 it
//! also carries the atomic operation envelope vocabulary of dev-plan §5.2 M1:
//! one indivisible target-visible event per AMO (RMW), load-reserved, or
//! store-conditional, consistent with ADR-0002 §§4–6.  It still does not
//! provide a native address map, adapt [`crate::memory::MemoryInterface`],
//! interpret integer or floating-point values, or map faults to Hart traps.
//!
//! The atomic envelope is a separate request/response family, structurally
//! distinct from [`PhysicalRequest`]/[`PhysicalResponse`]: a fetch, read, or
//! write pair can never represent an AMO or store-conditional, because the
//! Hart-supplied pure transform, the operand bytes, the optional reservation
//! context, the old-value result, and the conditional status exist only on
//! [`AtomicRequest`]/[`AtomicResponse`].  The transform is produced only by
//! the single Hart-owned AMO arithmetic module ([`crate::hart_amods`]); the
//! physical domain applies it opaquely and implements no ISA semantics.
//!
//! [`ValidatedPhysicalAccess`] is the small validation boundary the native
//! target adapters sit behind, and [`ValidatedAtomicAccess`] is its atomic
//! envelope counterpart used since A8 T2.  A backend reports typed target,
//! host, protocol, or unknown-completion results; the boundary validates the
//! request before the backend is called and validates every response before
//! returning success.
//! Valid requests and responses use fixed-size storage (at most eight bytes),
//! so the boundary itself does not add a heap allocation to an ordinary step.
//! Error context is owned only on failure paths.

use crate::memory::{MemoryError, SimpleMemory};
use std::fmt;
use std::sync::{Arc, Mutex};

use thiserror::Error;

/// The largest non-atomic physical transfer width in the A7 contract.
pub const MAX_PHYSICAL_ACCESS_BYTES: usize = 8;

/// Explicit byte widths accepted by the non-atomic physical contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PhysicalWidth {
    /// One byte.
    Byte = 1,
    /// Two bytes.
    Halfword = 2,
    /// Four bytes.
    Word = 4,
    /// Eight bytes.
    Doubleword = 8,
}

impl PhysicalWidth {
    /// Returns the number of bytes in this width.
    pub const fn bytes(self) -> usize {
        self as usize
    }

    /// Converts an external byte count into a contract width.
    ///
    /// Zero and every width other than 1, 2, 4, or 8 are malformed request
    /// input.  A backend's refusal of one of these valid widths is instead a
    /// [`PhysicalTargetRejectionReason::UnsupportedWidth`].
    pub fn from_bytes(bytes: usize) -> Result<Self, PhysicalProtocolError> {
        match bytes {
            0 => Err(PhysicalProtocolError::ZeroWidth),
            1 => Ok(Self::Byte),
            2 => Ok(Self::Halfword),
            4 => Ok(Self::Word),
            8 => Ok(Self::Doubleword),
            requested => Err(PhysicalProtocolError::InvalidWidth { requested }),
        }
    }
}

impl TryFrom<usize> for PhysicalWidth {
    type Error = PhysicalProtocolError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<u8> for PhysicalWidth {
    type Error = PhysicalProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::from_bytes(value as usize)
    }
}

/// Access categories carried by a non-atomic physical request.
///
/// AMO/LR/SC operation envelopes are never represented by these ordinary
/// categories: the atomic category lives on the separate
/// [`AtomicRequest`] family, and an ordinary read/write pair must not be
/// used to emulate an atomic operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicalAccessKind {
    /// Instruction bytes requested by the Hart.
    Fetch,
    /// Raw bytes loaded by an ordinary integer or floating-point operation.
    DataRead,
    /// Raw bytes stored by an ordinary integer or floating-point operation.
    DataWrite,
}

/// A valid request identity used to bind a response to its request.
///
/// This descriptor intentionally excludes the write payload.  The payload is
/// part of request validation and is observed by the backend, while address,
/// width, and category are the response-binding identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicalRequestDescriptor {
    /// 64-bit physical start address.
    pub paddr: u64,
    /// Explicit transfer width.
    pub width: PhysicalWidth,
    /// Fetch/read/write category.
    pub category: PhysicalAccessKind,
}

impl PhysicalRequestDescriptor {
    /// Creates a request descriptor from already-typed request fields.
    pub const fn new(paddr: u64, width: PhysicalWidth, category: PhysicalAccessKind) -> Self {
        Self {
            paddr,
            width,
            category,
        }
    }

    /// Returns the contiguous physical span represented by this descriptor.
    pub const fn span(self) -> PhysicalSpan {
        PhysicalSpan {
            paddr: self.paddr,
            width: self.width,
        }
    }
}

/// One contiguous physical byte span.
///
/// The span is retained even when its end would wrap.  Calling
/// [`Self::checked_end_inclusive`] distinguishes that target rejection from a
/// malformed request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicalSpan {
    /// 64-bit physical start address.
    pub paddr: u64,
    /// Number of bytes in the span.
    pub width: PhysicalWidth,
}

impl PhysicalSpan {
    /// Returns the last physical address when the span does not wrap.
    pub const fn checked_end_inclusive(self) -> Option<u64> {
        self.paddr.checked_add((self.width.bytes() - 1) as u64)
    }

    /// Returns whether this span is representable without wrapping.
    pub const fn is_non_wrapping(self) -> bool {
        self.checked_end_inclusive().is_some()
    }
}

/// Typed protocol/invariant failures at the request or response boundary.
///
/// These errors are never inferred from an error string.  A malformed request
/// is rejected before a backend call; a malformed backend response is rejected
/// before an `Ok` result can escape the validated port.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PhysicalProtocolError {
    /// A zero-byte request is not a valid physical transaction.
    #[error("physical access width must be nonzero")]
    ZeroWidth,
    /// A request width other than 1, 2, 4, or 8 bytes was supplied.
    #[error("unsupported physical access width {requested}; expected 1, 2, 4, or 8")]
    InvalidWidth {
        /// The malformed external width.
        requested: usize,
    },
    /// A write did not carry exactly the selected width of bytes.
    #[error("{category:?} write payload has length {actual}, expected {expected} bytes")]
    PayloadLength {
        /// The request category carrying the malformed payload.
        category: PhysicalAccessKind,
        /// Required payload length.
        expected: usize,
        /// Supplied payload length.
        actual: usize,
    },
    /// A fetch or read carried a write payload.
    #[error("{category:?} request unexpectedly carried {actual} payload bytes")]
    UnexpectedPayload {
        /// The category that cannot carry a payload.
        category: PhysicalAccessKind,
        /// Supplied payload length.
        actual: usize,
    },
    /// A backend response was bound to a different address, width, or category.
    #[error("physical response binding does not match its request")]
    ResponseBindingMismatch {
        /// Identity requested by the caller.
        expected: PhysicalRequestDescriptor,
        /// Identity returned by the backend.
        actual: PhysicalRequestDescriptor,
    },
    /// The backend's supplied response bytes do not match its reported length.
    #[error("physical response supplied {supplied} bytes but reported {reported} bytes")]
    ResponsePayloadLengthMismatch {
        /// Length reported by the backend response metadata.
        reported: usize,
        /// Length actually supplied to the response constructor.
        supplied: usize,
    },
    /// A read/fetch response did not contain exactly the requested bytes.
    #[error("physical response has {actual} bytes, expected {expected}")]
    ResponseLengthMismatch {
        /// Requested width.
        expected: usize,
        /// Reported response length.
        actual: usize,
    },
    /// The response completion kind contradicts the request category.
    #[error("physical response completion {actual:?} contradicts {expected:?} request")]
    ResponseCompletionMismatch {
        /// Completion kind required by the request.
        expected: PhysicalCompletionKind,
        /// Completion kind returned by the backend.
        actual: PhysicalCompletionKind,
    },
    /// The backend explicitly reported a protocol violation.
    #[error("backend reported a physical protocol failure: {context}")]
    BackendProtocol {
        /// Typed backend diagnostic context.
        context: String,
    },
    /// A validated-port invariant failed while accepting a response.
    #[error("physical access invariant failure: {context}")]
    Invariant {
        /// Diagnostic context for the invariant failure.
        context: String,
    },
}

/// A validated, borrowed non-atomic physical request.
///
/// The constructors enforce category/payload rules and retain the write bytes
/// by reference.  In particular, constructing a write does not copy its
/// payload and does not allocate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRequest<'a> {
    descriptor: PhysicalRequestDescriptor,
    write_payload: Option<&'a [u8]>,
}

impl<'a> PhysicalRequest<'a> {
    /// Creates a request with an explicit category and typed width.
    pub fn new(
        category: PhysicalAccessKind,
        paddr: u64,
        width: PhysicalWidth,
        write_payload: Option<&'a [u8]>,
    ) -> Result<Self, PhysicalProtocolError> {
        let request = Self {
            descriptor: PhysicalRequestDescriptor::new(paddr, width, category),
            write_payload,
        };
        request.validate()?;
        Ok(request)
    }

    /// Creates a request from an external byte count.
    ///
    /// This is the checked boundary for raw width values.  Invalid widths are
    /// protocol failures, while a valid-width target refusal is represented by
    /// [`PhysicalAccessError::TargetRejected`].
    pub fn from_parts(
        category: PhysicalAccessKind,
        paddr: u64,
        width: usize,
        write_payload: Option<&'a [u8]>,
    ) -> Result<Self, PhysicalProtocolError> {
        Self::new(
            category,
            paddr,
            PhysicalWidth::from_bytes(width)?,
            write_payload,
        )
    }

    /// Creates an instruction-fetch request.
    pub fn fetch(paddr: u64, width: PhysicalWidth) -> Result<Self, PhysicalProtocolError> {
        Self::new(PhysicalAccessKind::Fetch, paddr, width, None)
    }

    /// Creates an ordinary data-read request.
    pub fn data_read(paddr: u64, width: PhysicalWidth) -> Result<Self, PhysicalProtocolError> {
        Self::new(PhysicalAccessKind::DataRead, paddr, width, None)
    }

    /// Alias for [`Self::data_read`].
    pub fn read(paddr: u64, width: PhysicalWidth) -> Result<Self, PhysicalProtocolError> {
        Self::data_read(paddr, width)
    }

    /// Creates an ordinary data-write request with an exact byte payload.
    pub fn data_write(
        paddr: u64,
        width: PhysicalWidth,
        payload: &'a [u8],
    ) -> Result<Self, PhysicalProtocolError> {
        Self::new(PhysicalAccessKind::DataWrite, paddr, width, Some(payload))
    }

    /// Alias for [`Self::data_write`].
    pub fn write(
        paddr: u64,
        width: PhysicalWidth,
        payload: &'a [u8],
    ) -> Result<Self, PhysicalProtocolError> {
        Self::data_write(paddr, width, payload)
    }

    /// Returns the physical start address.
    pub const fn paddr(&self) -> u64 {
        self.descriptor.paddr
    }

    /// Returns the explicit transfer width.
    pub const fn width(&self) -> PhysicalWidth {
        self.descriptor.width
    }

    /// Returns the request category.
    pub const fn category(&self) -> PhysicalAccessKind {
        self.descriptor.category
    }

    /// Returns the address/width/category identity used for response binding.
    pub const fn descriptor(&self) -> PhysicalRequestDescriptor {
        self.descriptor
    }

    /// Returns the request's contiguous physical span.
    pub const fn span(&self) -> PhysicalSpan {
        self.descriptor.span()
    }

    /// Returns the exact write payload, or `None` for fetch/read requests.
    pub const fn write_payload(&self) -> Option<&'a [u8]> {
        self.write_payload
    }

    /// Alias for [`Self::write_payload`].
    pub const fn payload(&self) -> Option<&'a [u8]> {
        self.write_payload()
    }

    /// Rechecks request invariants without checking target address routing.
    ///
    /// A non-wrapping physical span is deliberately not part of this method:
    /// range overflow is a target rejection and is checked by the validated
    /// port immediately before backend dispatch.
    pub fn validate(&self) -> Result<(), PhysicalProtocolError> {
        match (self.category(), self.write_payload) {
            (PhysicalAccessKind::DataWrite, Some(payload)) => {
                let expected = self.width().bytes();
                if payload.len() != expected {
                    return Err(PhysicalProtocolError::PayloadLength {
                        category: self.category(),
                        expected,
                        actual: payload.len(),
                    });
                }
            }
            (PhysicalAccessKind::DataWrite, None) => {
                return Err(PhysicalProtocolError::PayloadLength {
                    category: self.category(),
                    expected: self.width().bytes(),
                    actual: 0,
                });
            }
            (category, Some(payload)) => {
                return Err(PhysicalProtocolError::UnexpectedPayload {
                    category,
                    actual: payload.len(),
                });
            }
            (_, None) => {}
        }
        Ok(())
    }
}

/// Fixed-size raw response bytes.
///
/// A conforming response reports exactly 1, 2, 4, or 8 bytes.  The reported
/// length is kept separately from the eight-byte inline prefix so a fake or
/// transport adapter can be tested with a short or long response without a
/// heap-backed buffer.  A long response is rejected by the validated port
/// before its prefix can be observed as success.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PhysicalResponseBytes {
    bytes: [u8; MAX_PHYSICAL_ACCESS_BYTES],
    /// Number of bytes supplied to the constructor, retained separately from
    /// the untrusted backend-reported length.
    supplied_len: usize,
    /// Length claimed by the backend response metadata.
    reported_len: usize,
}

impl PhysicalResponseBytes {
    /// Copies response bytes into the inline response representation.
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self::with_reported_len(bytes, bytes.len())
    }

    /// Creates response bytes with an explicitly reported length.
    ///
    /// Only the first eight bytes are stored.  Both the supplied slice length
    /// and `reported_len` are retained; the validated port rejects a response
    /// that claims a padded or truncated length.  `reported_len` may be greater
    /// than eight solely to model a malformed long response; such a response
    /// can never be accepted by [`ValidatedPhysicalAccess`].
    pub fn with_reported_len(bytes: &[u8], reported_len: usize) -> Self {
        let mut stored = [0; MAX_PHYSICAL_ACCESS_BYTES];
        let copy_len = bytes.len().min(MAX_PHYSICAL_ACCESS_BYTES);
        stored[..copy_len].copy_from_slice(&bytes[..copy_len]);
        Self {
            bytes: stored,
            supplied_len: bytes.len(),
            reported_len,
        }
    }

    /// Returns the number of bytes supplied to the constructor.
    pub const fn supplied_len(self) -> usize {
        self.supplied_len
    }

    /// Returns the backend-reported byte length.
    pub const fn reported_len(self) -> usize {
        self.reported_len
    }

    /// Returns the exact bytes when supplied and reported lengths agree and
    /// fit inline storage.
    ///
    /// The validated success path always has a `Some` result.  `None` marks a
    /// deliberately malformed or over-sized response.
    pub fn as_slice(&self) -> Option<&[u8]> {
        (self.supplied_len == self.reported_len && self.reported_len <= MAX_PHYSICAL_ACCESS_BYTES)
            .then(|| &self.bytes[..self.reported_len])
    }

    /// Returns the stored prefix for diagnostics of a malformed response.
    pub fn stored_prefix(&self) -> &[u8] {
        &self.bytes[..self.reported_len.min(MAX_PHYSICAL_ACCESS_BYTES)]
    }
}

impl fmt::Debug for PhysicalResponseBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PhysicalResponseBytes")
            .field("bytes", &self.stored_prefix())
            .field("supplied_len", &self.supplied_len)
            .field("reported_len", &self.reported_len)
            .finish()
    }
}

/// Completion kind carried by a physical response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicalCompletionKind {
    /// A raw-byte read/fetch completion.
    Read,
    /// A write completion with no invented read value.
    WriteAcknowledgement,
}

/// Untrusted backend completion body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalResponseCompletion {
    /// Raw bytes returned by a fetch or data read.
    Read(PhysicalResponseBytes),
    /// Complete data-write acknowledgement.
    WriteAcknowledgement,
}

impl PhysicalResponseCompletion {
    /// Returns the completion kind without interpreting any raw bytes.
    pub const fn kind(self) -> PhysicalCompletionKind {
        match self {
            Self::Read(_) => PhysicalCompletionKind::Read,
            Self::WriteAcknowledgement => PhysicalCompletionKind::WriteAcknowledgement,
        }
    }
}

/// A backend response carrying an explicit request binding.
///
/// Backends may construct this with [`Self::new`], but the value is untrusted
/// until [`ValidatedPhysicalAccess`] checks its binding, completion kind, and
/// exact read length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalResponse {
    binding: PhysicalRequestDescriptor,
    completion: PhysicalResponseCompletion,
}

impl PhysicalResponse {
    /// Creates an untrusted response for a descriptor and completion body.
    pub const fn new(
        binding: PhysicalRequestDescriptor,
        completion: PhysicalResponseCompletion,
    ) -> Self {
        Self {
            binding,
            completion,
        }
    }

    /// Creates a read/fetch response bound to a request.
    ///
    /// Length is intentionally not checked here: this constructor is also the
    /// test/backend seam for proving that the validated port rejects short and
    /// long responses.
    pub fn read_for(request: &PhysicalRequest<'_>, bytes: &[u8]) -> Self {
        Self::new(
            request.descriptor(),
            PhysicalResponseCompletion::Read(PhysicalResponseBytes::from_slice(bytes)),
        )
    }

    /// Creates a write acknowledgement bound to a request.
    pub const fn write_ack_for(request: &PhysicalRequest<'_>) -> Self {
        Self::new(
            request.descriptor(),
            PhysicalResponseCompletion::WriteAcknowledgement,
        )
    }

    /// Returns the response's request binding.
    pub const fn binding(&self) -> PhysicalRequestDescriptor {
        self.binding
    }

    /// Returns the untrusted completion body.
    pub const fn completion(&self) -> PhysicalResponseCompletion {
        self.completion
    }

    /// Returns validated raw bytes when this response body is a read.
    pub fn read_bytes(&self) -> Option<&[u8]> {
        match &self.completion {
            PhysicalResponseCompletion::Read(bytes) => bytes.as_slice(),
            PhysicalResponseCompletion::WriteAcknowledgement => None,
        }
    }

    /// Returns whether this response body is a write acknowledgement.
    pub const fn is_write_acknowledgement(&self) -> bool {
        matches!(
            self.completion,
            PhysicalResponseCompletion::WriteAcknowledgement
        )
    }
}

/// Reasons a valid request may be refused by a physical target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicalTargetRejectionReason {
    /// The complete address span wraps past `u64::MAX`.
    RangeOverflow,
    /// No target accepts the complete physical span.
    Unmapped,
    /// The target denies this otherwise valid request.
    PermissionDenied,
    /// The target does not support this valid width.
    UnsupportedWidth,
    /// The target does not support this valid category.
    UnsupportedCategory,
    /// The target reported a bus or device access failure.
    TargetError,
}

/// A guest-visible physical target rejection with preserved request context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalTargetRejection {
    /// Address/width/category identity of the rejected request.
    pub request: PhysicalRequestDescriptor,
    /// Complete span, retained even when its end overflows.
    pub span: PhysicalSpan,
    /// Typed target rejection reason.
    pub reason: PhysicalTargetRejectionReason,
    /// Target-specific diagnostic context.
    pub context: String,
}

impl PhysicalTargetRejection {
    /// Creates a target rejection while retaining the request identity and span.
    pub fn new(
        request: PhysicalRequestDescriptor,
        reason: PhysicalTargetRejectionReason,
        context: impl Into<String>,
    ) -> Self {
        Self {
            span: request.span(),
            request,
            reason,
            context: context.into(),
        }
    }
}

impl fmt::Display for PhysicalTargetRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "target rejected {:?} at {:#018x} ({} bytes): {}",
            self.request.category,
            self.request.paddr,
            self.request.width.bytes(),
            self.context
        )
    }
}

impl std::error::Error for PhysicalTargetRejection {}

/// A host/backend failure for a valid physical request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalBackendFailure {
    /// Request being serviced when the host/backend failed.
    pub request: PhysicalRequestDescriptor,
    /// Typed diagnostic context.
    pub context: String,
}

impl fmt::Display for PhysicalBackendFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "backend failure: {}", self.context)
    }
}

impl std::error::Error for PhysicalBackendFailure {}

/// An unresolved backend completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalUnknownCompletion {
    /// Request whose effects cannot be established.
    pub request: PhysicalRequestDescriptor,
    /// Typed diagnostic context.
    pub context: String,
}

impl fmt::Display for PhysicalUnknownCompletion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown completion: {}", self.context)
    }
}

impl std::error::Error for PhysicalUnknownCompletion {}

/// Result categories a backend may return before response validation.
///
/// `Target` is the only guest-visible failure category.  `Host`, `Protocol`,
/// and `Unknown` remain simulator-side failures and are never reclassified by
/// the validated port from diagnostic text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicalBackendError {
    /// The target rejected a valid request.
    Target {
        /// Typed rejection reason.
        reason: PhysicalTargetRejectionReason,
        /// Target-specific context.
        context: String,
    },
    /// A host/backend resource prevented completion.
    Host {
        /// Host/backend diagnostic context.
        context: String,
    },
    /// The backend explicitly detected a protocol violation.
    Protocol {
        /// Protocol diagnostic context.
        context: String,
    },
    /// The backend cannot establish whether effects occurred.
    Unknown {
        /// Unknown-completion diagnostic context.
        context: String,
    },
}

impl PhysicalBackendError {
    /// Creates a typed target rejection result.
    pub fn target(reason: PhysicalTargetRejectionReason, context: impl Into<String>) -> Self {
        Self::Target {
            reason,
            context: context.into(),
        }
    }

    /// Creates a typed host/backend failure result.
    pub fn host(context: impl Into<String>) -> Self {
        Self::Host {
            context: context.into(),
        }
    }

    /// Creates a typed backend protocol failure result.
    pub fn protocol(context: impl Into<String>) -> Self {
        Self::Protocol {
            context: context.into(),
        }
    }

    /// Creates a typed unknown-completion result.
    pub fn unknown(context: impl Into<String>) -> Self {
        Self::Unknown {
            context: context.into(),
        }
    }
}

/// Final error taxonomy returned by a validated physical access.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PhysicalAccessError {
    /// A valid request was rejected by its physical target.
    #[error("{0}")]
    TargetRejected(PhysicalTargetRejection),
    /// The host/backend could not complete a valid request.
    #[error("{0}")]
    BackendFailure(PhysicalBackendFailure),
    /// Request or response protocol/invariant validation failed.
    #[error("{0}")]
    Protocol(PhysicalProtocolError),
    /// Completion effects cannot be established; no retry is implied.
    #[error("{0}")]
    UnknownCompletion(PhysicalUnknownCompletion),
}

/// Result returned by a physical-access boundary.
pub type PhysicalAccessResult = Result<PhysicalResponse, PhysicalAccessError>;

/// Result a raw backend supplies to the validated boundary.
pub type PhysicalBackendResult = Result<PhysicalResponse, PhysicalBackendError>;

/// The untrusted transport/backend seam for a non-atomic request.
///
/// Implementations report raw bytes and typed failure categories only.  They do
/// not sign/zero extend loads, interpret FP values, enter traps, or implement
/// atomic operations.  A backend is not certified merely by implementing this
/// trait; callers must place it behind [`ValidatedPhysicalAccess`] to obtain
/// request/response validation.
pub trait PhysicalBackend {
    /// Services one complete request without retaining the borrowed request.
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult;
}

impl<B: PhysicalBackend + ?Sized> PhysicalBackend for &mut B {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        (**self).transact(request)
    }
}

/// A native target implementation used by a shared-lock adapter.
///
/// The target owns routing and raw-byte effects; the shared adapter below owns
/// the outer synchronization boundary.  A target implementation is still only
/// a backend and must be placed behind [`ValidatedPhysicalAccess`] before it is
/// advertised as a validated physical port.
pub trait NativePhysicalTarget {
    /// Services one complete native request without retaining the borrow.
    fn transact_native(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult;
}

/// A native target that can execute one atomic operation envelope
/// (dev-plan §5.2 M1).
///
/// The target owns routing and the single locked critical section per
/// envelope; the shared adapter owns the outer synchronization boundary
/// exactly as for [`NativePhysicalTarget`].  A target that cannot provide
/// the envelope rejects it before any mutation, and poisoned
/// synchronization is a host/backend failure, never a target rejection.
/// Targets implement no ISA semantics: an RMW applies the request's
/// Hart-supplied [`AmoTransform`] opaquely.
pub trait NativeAtomicTarget {
    /// Services one complete atomic envelope without retaining the borrow.
    fn transact_native_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult;
}

/// A native backend view over one shared target instance.
///
/// This wrapper stores an `Arc<Mutex<T>>`, not a snapshot.  A legacy typed view
/// and a raw native view can therefore be constructed from the same target and
/// observe each other's committed bytes/device effects immediately.  Poisoned
/// synchronization is a host/backend failure and never a target rejection.
#[derive(Debug)]
pub struct SharedNativeBackend<T> {
    target: Arc<Mutex<T>>,
}

impl<T> SharedNativeBackend<T> {
    /// Creates a native backend view over an existing target instance.
    pub const fn new(target: Arc<Mutex<T>>) -> Self {
        Self { target }
    }

    /// Borrows the exact shared target handle.
    pub const fn target(&self) -> &Arc<Mutex<T>> {
        &self.target
    }

    /// Consumes the view and returns the shared target handle.
    pub fn into_target(self) -> Arc<Mutex<T>> {
        self.target
    }
}

impl<T: NativePhysicalTarget> PhysicalBackend for SharedNativeBackend<T> {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        let mut target = self
            .target
            .lock()
            .map_err(|_| PhysicalBackendError::host("native target lock poisoned"))?;
        target.transact_native(request)
    }
}

impl<T: NativeAtomicTarget> AtomicBackend for SharedNativeBackend<T> {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        let mut target = self
            .target
            .lock()
            .map_err(|_| PhysicalBackendError::host("native target lock poisoned"))?;
        target.transact_native_atomic(request)
    }
}

/// Shared-lock raw backend for the native [`crate::executor::SystemBus`].
///
/// The alias is intentionally a view over the caller-supplied bus.  It does
/// not copy RAM, UART state, or the HTIF callback.
pub type NativeSystemBusBackend = SharedNativeBackend<crate::executor::SystemBus>;

/// Descriptive compatibility alias for [`NativeSystemBusBackend`].
pub type SystemBusPhysicalBackend = NativeSystemBusBackend;

/// Native raw backend for one shared [`SimpleMemory`] RAM object.
///
/// Unlike the legacy typed `MemoryInterface`, this target transfers one
/// contiguous raw span and does not apply architectural alignment or integer
/// extension.  The physical port remains responsible for request validation;
/// this backend repeats the request/span checks so a direct backend call cannot
/// wrap or partially write.
pub struct NativeRamBackend {
    memory: Arc<Mutex<SimpleMemory>>,
    ram_base: u64,
    ram_size: usize,
}

impl fmt::Debug for NativeRamBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeRamBackend")
            .field("ram_base", &self.ram_base)
            .field("ram_size", &self.ram_size)
            .finish_non_exhaustive()
    }
}

impl NativeRamBackend {
    /// Creates a raw RAM backend over an existing RAM/locking domain.
    pub const fn new(memory: Arc<Mutex<SimpleMemory>>, ram_base: u64, ram_size: usize) -> Self {
        Self {
            memory,
            ram_base,
            ram_size,
        }
    }

    /// Borrows the exact shared RAM handle.
    pub const fn memory(&self) -> &Arc<Mutex<SimpleMemory>> {
        &self.memory
    }

    /// Returns the physical RAM base used by this adapter.
    pub const fn ram_base(&self) -> u64 {
        self.ram_base
    }

    /// Returns the configured physical RAM span.
    pub const fn ram_size(&self) -> usize {
        self.ram_size
    }

    /// Consumes the adapter and returns the exact shared RAM handle.
    pub fn into_memory(self) -> Arc<Mutex<SimpleMemory>> {
        self.memory
    }
}

impl NativePhysicalTarget for NativeRamBackend {
    fn transact_native(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        request
            .validate()
            .map_err(|error| PhysicalBackendError::protocol(error.to_string()))?;

        let descriptor = request.descriptor();
        if !request.span().is_non_wrapping() {
            return Err(PhysicalBackendError::Target {
                reason: PhysicalTargetRejectionReason::RangeOverflow,
                context: format!(
                    "physical span starting at {:#018x} with width {} wraps the address space",
                    descriptor.paddr,
                    descriptor.width.bytes()
                ),
            });
        }
        if !crate::memory::contains_range(
            self.ram_base,
            self.ram_size,
            descriptor.paddr,
            descriptor.width.bytes(),
        ) {
            return Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::Unmapped,
                format!(
                    "RAM does not contain the complete span at {:#018x} ({} bytes)",
                    descriptor.paddr,
                    descriptor.width.bytes()
                ),
            ));
        }

        let offset = descriptor.paddr.checked_sub(self.ram_base).ok_or_else(|| {
            PhysicalBackendError::target(
                PhysicalTargetRejectionReason::Unmapped,
                "RAM address is below its configured base",
            )
        })?;
        let mut memory = self
            .memory
            .lock()
            .map_err(|_| PhysicalBackendError::host("RAM lock poisoned"))?;

        match descriptor.category {
            PhysicalAccessKind::Fetch | PhysicalAccessKind::DataRead => {
                let mut bytes = [0; MAX_PHYSICAL_ACCESS_BYTES];
                memory
                    .read_bytes_into(offset, &mut bytes[..descriptor.width.bytes()])
                    .map_err(map_native_memory_error)?;
                Ok(PhysicalResponse::new(
                    descriptor,
                    PhysicalResponseCompletion::Read(PhysicalResponseBytes::from_slice(
                        &bytes[..descriptor.width.bytes()],
                    )),
                ))
            }
            PhysicalAccessKind::DataWrite => {
                let payload = request.write_payload().ok_or_else(|| {
                    PhysicalBackendError::protocol(
                        "validated native RAM write was missing its payload",
                    )
                })?;
                memory
                    .write_bytes(offset, payload)
                    .map_err(map_native_memory_error)?;
                Ok(PhysicalResponse::new(
                    descriptor,
                    PhysicalResponseCompletion::WriteAcknowledgement,
                ))
            }
        }
    }
}

impl PhysicalBackend for NativeRamBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.transact_native(request)
    }
}

/// Executes one atomic envelope against shared RAM (dev-plan §5.2 M1).
///
/// This is the single native-RAM atomic critical section: the complete span
/// and width are validated before any mutation, one lock hold covers the
/// whole section, and the Hart-supplied transform (RMW) or echoed reservation
/// context (SC) is applied opaquely by the storage object.  RAM implements
/// no ISA semantics of its own.
///
/// `memory` is the shared RAM handle both the flat and bus configurations
/// wrap; `ram_base`/`ram_size` are the physical window this target claims.
/// Called by [`NativeRamBackend`] and by the [`crate::executor::SystemBus`]
/// native atomic path so both facades execute the identical section.
pub(crate) fn native_ram_atomic_transact(
    memory: &Arc<Mutex<SimpleMemory>>,
    ram_base: u64,
    ram_size: usize,
    request: &AtomicRequest<'_>,
) -> AtomicBackendResult {
    request
        .validate()
        .map_err(|error| PhysicalBackendError::protocol(error.to_string()))?;

    let descriptor = request.descriptor();
    if !request.span().is_non_wrapping() {
        return Err(PhysicalBackendError::target(
            PhysicalTargetRejectionReason::RangeOverflow,
            format!(
                "atomic span starting at {:#018x} with width {} wraps the address space",
                descriptor.paddr,
                descriptor.width.bytes()
            ),
        ));
    }
    if !crate::memory::contains_range(
        ram_base,
        ram_size,
        descriptor.paddr,
        descriptor.width.bytes(),
    ) {
        return Err(PhysicalBackendError::target(
            PhysicalTargetRejectionReason::Unmapped,
            format!(
                "RAM does not contain the complete atomic span at {:#018x} ({} bytes)",
                descriptor.paddr,
                descriptor.width.bytes()
            ),
        ));
    }

    // One lock hold for the whole critical section: no competing reader can
    // observe the internal read and write as separate events, and the
    // conditional check cannot be staled by any granted writer.
    let mut ram = memory
        .lock()
        .map_err(|_| PhysicalBackendError::host("RAM lock poisoned"))?;
    let offset = descriptor.paddr - ram_base;
    let width = descriptor.width.bytes();
    match descriptor.kind {
        AtomicAccessKind::Rmw => {
            let transform = request.transform().ok_or_else(|| {
                PhysicalBackendError::protocol(
                    "validated RMW envelope was missing its Hart transform",
                )
            })?;
            let operand = request.operand_bytes().ok_or_else(|| {
                PhysicalBackendError::protocol("validated RMW envelope was missing its operand")
            })?;
            let old = ram
                .atomic_rmw(offset, width, operand, |old, operand| {
                    transform.apply(old, operand)
                })
                .map_err(map_native_memory_error)?;
            Ok(AtomicResponse::rmw_for(request, &old[..width]))
        }
        AtomicAccessKind::LoadReserved => {
            let outcome = ram
                .atomic_load_reserved(offset, width)
                .map_err(map_native_memory_error)?;
            let snapshot =
                CommittedWriteSnapshot::from_bytes(&outcome.snapshot).map_err(|error| {
                    PhysicalBackendError::protocol(format!(
                        "RAM committed-write snapshot does not fit the envelope: {error}"
                    ))
                })?;
            Ok(AtomicResponse::load_reserved_for(
                request,
                &outcome.old_bytes[..width],
                snapshot,
            ))
        }
        AtomicAccessKind::StoreConditional => {
            let payload = request.store_payload().ok_or_else(|| {
                PhysicalBackendError::protocol(
                    "validated store-conditional envelope was missing its payload",
                )
            })?;
            let committed = if let Some(context) = request.reservation() {
                let covered = context.reserved.paddr <= descriptor.paddr
                    && descriptor
                        .span()
                        .checked_end_inclusive()
                        .zip(context.reserved.checked_end_inclusive())
                        .is_some_and(|(requested_end, reserved_end)| requested_end <= reserved_end);
                if !covered {
                    false
                } else {
                    // A covered reservation must describe this RAM's
                    // bookkeeping domain. An uncovered one is already a
                    // conditional failure and touches no guest bytes.
                    if !crate::memory::contains_range(
                        ram_base,
                        ram_size,
                        context.reserved.paddr,
                        context.reserved.width.bytes(),
                    ) {
                        return Err(PhysicalBackendError::protocol(format!(
                            "reservation context span {:?} is outside this RAM's bookkeeping domain",
                            context.reserved
                        )));
                    }
                    ram.atomic_store_conditional(
                        offset,
                        width,
                        payload,
                        context.reserved.paddr - ram_base,
                        context.reserved.width.bytes(),
                        context.snapshot.as_bytes(),
                    )
                    .map_err(map_native_memory_error)?
                }
            } else {
                false
            };
            Ok(AtomicResponse::store_conditional_for(
                request,
                if committed {
                    ConditionalStatus::Success
                } else {
                    ConditionalStatus::Failure
                },
            ))
        }
    }
}

impl AtomicBackend for NativeRamBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        native_ram_atomic_transact(&self.memory, self.ram_base, self.ram_size, request)
    }
}

/// Maps a concrete native-memory error by its typed variant.
///
/// This helper deliberately does not inspect diagnostic strings.  Range and
/// target-local failures are target rejections; injected host/protocol failures
/// retain their simulator-side categories.
pub(crate) fn map_native_memory_error(error: MemoryError) -> PhysicalBackendError {
    match error {
        MemoryError::InvalidAddress(address) => PhysicalBackendError::Target {
            reason: PhysicalTargetRejectionReason::Unmapped,
            context: format!("native target rejected address {address:#018x}"),
        },
        MemoryError::OutOfBounds => PhysicalBackendError::Target {
            reason: PhysicalTargetRejectionReason::Unmapped,
            context: "native target rejected an out-of-bounds span".into(),
        },
        MemoryError::Misaligned(address, width) => PhysicalBackendError::Target {
            reason: PhysicalTargetRejectionReason::TargetError,
            context: format!(
                "native target rejected address {address:#018x} for {width}-byte access"
            ),
        },
        MemoryError::Backend(context) => PhysicalBackendError::Host { context },
        MemoryError::Protocol(context) => PhysicalBackendError::Protocol { context },
        MemoryError::Unknown(context) => PhysicalBackendError::Unknown { context },
    }
}

/// The validated transport-neutral physical-access interface.
pub trait PhysicalAccess {
    /// Validates and submits one complete non-atomic request.
    fn access(&mut self, request: PhysicalRequest<'_>) -> PhysicalAccessResult;
}

/// Validation boundary around an untrusted physical backend.
///
/// The wrapper checks malformed request payloads and the complete non-wrapping
/// span before invoking the backend.  It then checks response identity,
/// completion kind, and exact read length.  It performs exactly one backend
/// call and has no retry path, including for unknown completion.
#[derive(Debug)]
pub struct ValidatedPhysicalAccess<B> {
    backend: B,
}

impl<B> ValidatedPhysicalAccess<B> {
    /// Wraps a backend in the T1 validation boundary.
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Borrows the wrapped backend for inspection or adapter-specific setup.
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutably borrows the wrapped backend.
    pub const fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Unwraps the backend.
    pub fn into_backend(self) -> B {
        self.backend
    }
}

impl<B: PhysicalBackend> ValidatedPhysicalAccess<B> {
    /// Convenience method equivalent to the [`PhysicalAccess`] trait method.
    pub fn access(&mut self, request: PhysicalRequest<'_>) -> PhysicalAccessResult {
        <Self as PhysicalAccess>::access(self, request)
    }

    /// Submits one request through the validated boundary.
    pub fn transact(&mut self, request: PhysicalRequest<'_>) -> PhysicalAccessResult {
        self.access(request)
    }
}

/// Atomic envelopes reuse the one validated port object.
///
/// When the wrapped backend is also an [`AtomicBackend`], the port validates
/// and submits atomic envelopes through the same validation boundary the
/// standalone [`ValidatedAtomicAccess`] applies — borrowed here so both
/// categories share one port instance (dev-plan §7.2).  A backend without
/// atomic capability simply never satisfies this impl and cannot be connected
/// as a Hart data port.
impl<B: AtomicBackend> AtomicAccess for ValidatedPhysicalAccess<B> {
    fn access_atomic(&mut self, request: AtomicRequest<'_>) -> AtomicAccessResult {
        ValidatedAtomicAccess::new(&mut self.backend).access_atomic(request)
    }
}

impl<B: PhysicalBackend> PhysicalAccess for ValidatedPhysicalAccess<B> {
    fn access(&mut self, request: PhysicalRequest<'_>) -> PhysicalAccessResult {
        request.validate().map_err(PhysicalAccessError::Protocol)?;

        let descriptor = request.descriptor();
        if !request.span().is_non_wrapping() {
            return Err(PhysicalAccessError::TargetRejected(
                PhysicalTargetRejection::new(
                    descriptor,
                    PhysicalTargetRejectionReason::RangeOverflow,
                    format!(
                        "physical span starting at {:#018x} with width {} wraps the address space",
                        descriptor.paddr,
                        descriptor.width.bytes()
                    ),
                ),
            ));
        }

        let response = match self.backend.transact(&request) {
            Ok(response) => response,
            Err(PhysicalBackendError::Target { reason, context }) => {
                return Err(PhysicalAccessError::TargetRejected(
                    PhysicalTargetRejection::new(descriptor, reason, context),
                ));
            }
            Err(PhysicalBackendError::Host { context }) => {
                return Err(PhysicalAccessError::BackendFailure(
                    PhysicalBackendFailure {
                        request: descriptor,
                        context,
                    },
                ));
            }
            Err(PhysicalBackendError::Protocol { context }) => {
                return Err(PhysicalAccessError::Protocol(
                    PhysicalProtocolError::BackendProtocol { context },
                ));
            }
            Err(PhysicalBackendError::Unknown { context }) => {
                return Err(PhysicalAccessError::UnknownCompletion(
                    PhysicalUnknownCompletion {
                        request: descriptor,
                        context,
                    },
                ));
            }
        };

        validate_response(&request, response)
    }
}

fn validate_response(
    request: &PhysicalRequest<'_>,
    response: PhysicalResponse,
) -> PhysicalAccessResult {
    let expected_binding = request.descriptor();
    if response.binding() != expected_binding {
        return Err(PhysicalAccessError::Protocol(
            PhysicalProtocolError::ResponseBindingMismatch {
                expected: expected_binding,
                actual: response.binding(),
            },
        ));
    }

    let completion = response.completion();
    let expected_completion = match request.category() {
        PhysicalAccessKind::Fetch | PhysicalAccessKind::DataRead => PhysicalCompletionKind::Read,
        PhysicalAccessKind::DataWrite => PhysicalCompletionKind::WriteAcknowledgement,
    };
    if completion.kind() != expected_completion {
        return Err(PhysicalAccessError::Protocol(
            PhysicalProtocolError::ResponseCompletionMismatch {
                expected: expected_completion,
                actual: completion.kind(),
            },
        ));
    }

    if let PhysicalResponseCompletion::Read(bytes) = completion {
        if bytes.supplied_len() != bytes.reported_len() {
            return Err(PhysicalAccessError::Protocol(
                PhysicalProtocolError::ResponsePayloadLengthMismatch {
                    reported: bytes.reported_len(),
                    supplied: bytes.supplied_len(),
                },
            ));
        }

        let expected_len = request.width().bytes();
        if bytes.reported_len() != expected_len {
            return Err(PhysicalAccessError::Protocol(
                PhysicalProtocolError::ResponseLengthMismatch {
                    expected: expected_len,
                    actual: bytes.reported_len(),
                },
            ));
        }
        // Widths are limited to eight bytes, so an exact length always has an
        // inline slice.  Retain an explicit invariant check rather than
        // allowing a malformed future representation to become success.
        if bytes.as_slice().is_none() {
            return Err(PhysicalAccessError::Protocol(
                PhysicalProtocolError::Invariant {
                    context: "exact response length has no inline byte representation".into(),
                },
            ));
        }
    }

    Ok(response)
}

// Compatibility-oriented aliases keep the semantic vocabulary easy to find
// without creating a second interface or changing any existing public API.
/// Short alias for [`PhysicalWidth`].
pub type AccessWidth = PhysicalWidth;
/// Short alias for [`PhysicalAccessKind`].
pub type AccessCategory = PhysicalAccessKind;
/// Short alias for [`PhysicalAccessKind`].
pub type PhysicalRequestKind = PhysicalAccessKind;
/// Short alias for [`PhysicalResponseBytes`].
pub type RawPhysicalBytes = PhysicalResponseBytes;
/// Short alias for [`PhysicalResponseCompletion`].
pub type PhysicalCompletion = PhysicalResponseCompletion;
/// Short alias for [`ValidatedPhysicalAccess`].
pub type PhysicalAccessPort<B> = ValidatedPhysicalAccess<B>;

// -------------------------------------------------------------------------
// A8 T1: the atomic operation envelope vocabulary (dev-plan §5.2 M1).
//
// One indivisible target-visible event per AMO/LR/SC, extending this port
// with the atomic category required by ADR-0002 §§4–6.  The vocabulary is
// target-free: it defines the request kinds, the Hart-supplied pure
// transform representation, operand/result bytes, conditional status,
// validation, and response binding.  Backends execute the critical section
// and apply the Hart transform opaquely; they implement no ISA semantics.
// -------------------------------------------------------------------------

/// Maximum inline bytes of a committed-write bookkeeping snapshot.
///
/// A snapshot describes the covered blocks of at most an eight-byte span, so
/// a bounded inline representation keeps the envelope allocation-free.  The
/// bytes are opaque outside the backend that produced them; every bookkeeping
/// form must preserve exact reserved-span overlap semantics (dev-plan §5.2
/// M1), so a standalone coarse counter is not a valid snapshot.
pub const MAX_COMMITTED_WRITE_SNAPSHOT_BYTES: usize = 64;

/// Envelope result widths accepted by the atomic vocabulary.
///
/// Atomic widths are 4 or 8 bytes only (dev-plan §5.2 M1).  Byte and halfword
/// envelopes are malformed request input rejected before any backend call,
/// never target refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AmoWidth {
    /// Four-byte (W) envelope.
    Word = 4,
    /// Eight-byte (D) envelope.
    Doubleword = 8,
}

impl AmoWidth {
    /// Returns the number of bytes in this width.
    pub const fn bytes(self) -> usize {
        self as usize
    }

    /// Returns the equivalent physical transfer width.
    pub const fn physical_width(self) -> PhysicalWidth {
        match self {
            Self::Word => PhysicalWidth::Word,
            Self::Doubleword => PhysicalWidth::Doubleword,
        }
    }

    /// Returns the envelope width for a physical width, or `None` when the
    /// physical width is not an atomic width (1, 2, or anything but 4/8).
    pub const fn from_physical_width(width: PhysicalWidth) -> Option<Self> {
        match width {
            PhysicalWidth::Word => Some(Self::Word),
            PhysicalWidth::Doubleword => Some(Self::Doubleword),
            PhysicalWidth::Byte | PhysicalWidth::Halfword => None,
        }
    }
}

/// The Hart-supplied pure RMW transform carried by an envelope (dev-plan
/// §5.2 M1).
///
/// A plain function pointer over raw span bytes: the arguments are the old
/// span bytes and the operand bytes, both exactly the envelope width and in
/// physical/guest memory byte order; the returned buffer's first
/// `width.bytes()` bytes are the new span content, and the trailing bytes are
/// zero and ignored for narrower spans.  The produced functions are total and
/// never panic, so a backend can call one inside its critical section.
pub type PureAmoTransform =
    fn(old_bytes: &[u8], operand_bytes: &[u8]) -> [u8; MAX_PHYSICAL_ACCESS_BYTES];

/// One pure RMW transform produced by the Hart-owned AMO arithmetic module.
///
/// Instances are constructed only inside this crate by
/// [`crate::hart_amods::transform`]; callers elsewhere obtain transforms from
/// that module and apply them opaquely.  This keeps the AMO arithmetic in
/// exactly one Hart-owned implementation, identically for every backend.
#[derive(Debug, Clone, Copy)]
pub struct AmoTransform {
    width: AmoWidth,
    apply: PureAmoTransform,
}

impl AmoTransform {
    /// In-crate constructor: only the Hart-owned arithmetic module produces
    /// transforms.
    pub(crate) const fn new(width: AmoWidth, apply: PureAmoTransform) -> Self {
        Self { width, apply }
    }

    /// Returns the encoded W/D result width of this transform.
    pub const fn width(self) -> AmoWidth {
        self.width
    }

    /// Applies the pure transform to old and operand span bytes.
    pub fn apply(self, old_bytes: &[u8], operand_bytes: &[u8]) -> [u8; MAX_PHYSICAL_ACCESS_BYTES] {
        (self.apply)(old_bytes, operand_bytes)
    }
}

/// A backend-owned committed-write bookkeeping snapshot (dev-plan §5.2 M1,
/// §5.4).
///
/// A load-reserved envelope returns the domain's committed-write version
/// snapshot of the covered blocks; a store-conditional envelope echoes it so
/// the backend can re-check exact reserved-span overlap inside its one
/// critical section.  The bytes are opaque to the Hart and to the vocabulary:
/// the producing backend defines their interpretation.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommittedWriteSnapshot {
    bytes: [u8; MAX_COMMITTED_WRITE_SNAPSHOT_BYTES],
    len: usize,
}

impl CommittedWriteSnapshot {
    /// Copies snapshot bytes into the inline snapshot representation.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AtomicProtocolError> {
        if bytes.len() > MAX_COMMITTED_WRITE_SNAPSHOT_BYTES {
            return Err(AtomicProtocolError::SnapshotTooLong {
                requested: bytes.len(),
                max: MAX_COMMITTED_WRITE_SNAPSHOT_BYTES,
            });
        }
        let mut stored = [0; MAX_COMMITTED_WRITE_SNAPSHOT_BYTES];
        stored[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: stored,
            len: bytes.len(),
        })
    }

    /// Returns the number of snapshot bytes.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the snapshot carries no bytes.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the opaque snapshot bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl fmt::Debug for CommittedWriteSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommittedWriteSnapshot")
            .field("bytes", &self.as_bytes())
            .finish()
    }
}

/// Atomic request kinds carried by an atomic operation envelope.
///
/// Each kind is one indivisible target-visible event; none of them can be
/// composed from the ordinary [`PhysicalAccessKind`] categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomicAccessKind {
    /// One read-transform-write (AMO) critical-section event returning the
    /// exact old bytes.
    Rmw,
    /// One load-reserved read that also returns the committed-write snapshot
    /// of the covered blocks.
    LoadReserved,
    /// One conditional store whose single write happens only on conditional
    /// success inside the backend's critical section.
    StoreConditional,
}

/// Informational `aq`/`rl` ordering bits carried by an atomic envelope.
///
/// All four combinations are legal encodings (dev-plan §5.3); in this
/// single-Hart in-order profile they impose no additional ordering effect and
/// are excluded from the response-binding identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct AtomicOrdering {
    /// Acquire bit.
    pub aq: bool,
    /// Release bit.
    pub rl: bool,
}

/// A valid atomic request identity used to bind an atomic response to its
/// request.
///
/// Like [`PhysicalRequestDescriptor`], the binding identity excludes
/// payloads, the transform, the reservation context, and the informational
/// ordering bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtomicRequestDescriptor {
    /// 64-bit physical start address.
    pub paddr: u64,
    /// Explicit transfer width (4 or 8 bytes only).
    pub width: PhysicalWidth,
    /// RMW/load-reserved/store-conditional kind.
    pub kind: AtomicAccessKind,
}

impl AtomicRequestDescriptor {
    /// Creates an atomic request descriptor from already-typed fields.
    pub const fn new(paddr: u64, width: PhysicalWidth, kind: AtomicAccessKind) -> Self {
        Self { paddr, width, kind }
    }

    /// Returns the contiguous physical span represented by this descriptor.
    pub const fn span(self) -> PhysicalSpan {
        PhysicalSpan {
            paddr: self.paddr,
            width: self.width,
        }
    }
}

/// Conditional outcome of a store-conditional envelope (dev-plan §5.2 M1).
///
/// [`ConditionalStatus::Success`] means the backend performed the single
/// write inside its critical section; [`ConditionalStatus::Failure`] means no
/// write occurred.  Both are complete, successful envelope transactions: a
/// conditional failure is never a target rejection or an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConditionalStatus {
    /// The single conditional write committed.
    Success,
    /// No write occurred; the Hart writes its own `rd = 1` failure value.
    Failure,
}

/// The Hart reservation context carried by a store-conditional envelope
/// (dev-plan §5.2 M1, §5.4).
///
/// A store-conditional envelope may carry no reservation context. The target
/// must still validate the requested store span and its atomic capability
/// before returning conditional failure for absent or uncovered reservation
/// context. When a context is present and covers the request, the snapshot
/// re-check and write are the backend's, inside its one critical section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtomicReservationContext {
    /// Exact reserved byte span (port-issued paddr + width) recorded at the
    /// load-reserved.
    pub reserved: PhysicalSpan,
    /// The LR-time committed-write snapshot, echoed verbatim.
    pub snapshot: CommittedWriteSnapshot,
}

/// Typed protocol/invariant failures at the atomic request or response
/// boundary.
///
/// These errors are never inferred from an error string.  A malformed atomic
/// envelope is rejected before a backend call; a malformed atomic response is
/// rejected before an `Ok` result can escape the validated boundary.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AtomicProtocolError {
    /// An atomic envelope width other than 4 or 8 bytes was supplied.
    #[error("atomic envelope width must be 4 or 8 bytes, got {requested}")]
    UnsupportedAtomicWidth {
        /// The malformed requested width in bytes.
        requested: usize,
    },
    /// An RMW operand or store-conditional payload had the wrong length.
    #[error("{kind:?} envelope payload has length {actual}, expected {expected} bytes")]
    PayloadLength {
        /// The envelope kind carrying the malformed payload.
        kind: AtomicAccessKind,
        /// Required payload length.
        expected: usize,
        /// Supplied payload length.
        actual: usize,
    },
    /// An RMW envelope was missing its Hart-supplied transform.
    #[error("RMW envelope is missing its Hart-supplied transform")]
    MissingTransform,
    /// The transform's encoded width did not match the envelope width.
    #[error("Hart transform width is {actual} bytes but the envelope width is {expected} bytes")]
    TransformWidthMismatch {
        /// Required transform width in bytes.
        expected: usize,
        /// Supplied transform width in bytes.
        actual: usize,
    },
    /// The backend reported conditional success for an SC without Hart
    /// reservation context.
    #[error("store-conditional succeeded without Hart reservation context")]
    ConditionalSuccessWithoutReservation,
    /// The backend reported conditional success for an SC outside the Hart's
    /// reserved byte span.
    #[error("store-conditional succeeded for span {requested:?} outside reservation {reserved:?}")]
    ConditionalSuccessOutsideReservation {
        /// Hart's reserved span.
        reserved: PhysicalSpan,
        /// Requested SC span.
        requested: PhysicalSpan,
    },
    /// A reservation context or LR response carried an empty snapshot.
    #[error("committed-write snapshot is empty; it cannot describe any covered block")]
    EmptySnapshot,
    /// The reserved byte span of a reservation context wraps the address space.
    #[error("reserved span {reserved:?} wraps the address space")]
    ReservationSpanOverflow {
        /// The wrapping reserved span.
        reserved: PhysicalSpan,
    },
    /// Legacy protocol diagnosis for a caller that requires reservation-span
    /// containment. The standard atomic envelope accepts this as a valid SC
    /// request and asks the target to return conditional failure.
    #[error("envelope span {requested:?} is not contained in the reserved span {reserved:?}")]
    ReservationSpanMismatch {
        /// The reserved span carried by the context.
        reserved: PhysicalSpan,
        /// The requested envelope span.
        requested: PhysicalSpan,
    },
    /// Snapshot bytes exceeded the inline representation.
    #[error("committed-write snapshot needs {requested} bytes, at most {max} are carried inline")]
    SnapshotTooLong {
        /// Requested snapshot length.
        requested: usize,
        /// Inline capacity.
        max: usize,
    },
    /// A backend response was bound to a different address, width, or kind.
    #[error("atomic response binding does not match its request")]
    ResponseBindingMismatch {
        /// Identity requested by the caller.
        expected: AtomicRequestDescriptor,
        /// Identity returned by the backend.
        actual: AtomicRequestDescriptor,
    },
    /// The response completion kind contradicts the request kind.
    #[error("atomic response completion {actual:?} contradicts {expected:?} request")]
    ResponseCompletionMismatch {
        /// Completion kind required by the request.
        expected: AtomicAccessKind,
        /// Completion kind returned by the backend.
        actual: AtomicAccessKind,
    },
    /// The backend's supplied response bytes do not match its reported length.
    #[error("atomic response supplied {supplied} bytes but reported {reported} bytes")]
    ResponsePayloadLengthMismatch {
        /// Length reported by the backend response metadata.
        reported: usize,
        /// Length actually supplied to the response constructor.
        supplied: usize,
    },
    /// An RMW/LR response did not contain exactly the requested bytes.
    #[error("atomic response has {actual} bytes, expected {expected}")]
    ResponseLengthMismatch {
        /// Requested width.
        expected: usize,
        /// Reported response length.
        actual: usize,
    },
    /// The backend explicitly reported a protocol violation.
    #[error("backend reported an atomic protocol failure: {context}")]
    BackendProtocol {
        /// Typed backend diagnostic context.
        context: String,
    },
    /// A validated-boundary invariant failed while accepting a response.
    #[error("atomic envelope invariant failure: {context}")]
    Invariant {
        /// Diagnostic context for the invariant failure.
        context: String,
    },
}

/// A valid, borrowed atomic operation envelope request (dev-plan §5.2 M1).
///
/// The constructors enforce the kind/payload rules and retain borrowed bytes
/// by reference, so constructing an envelope does not allocate. A conforming
/// request is one indivisible event: an RMW carries the operand and the
/// Hart-supplied transform; a load-reserved carries nothing extra; a
/// store-conditional carries the write payload and optional Hart reservation
/// context. No field or constructor can express an AMO/SC through the
/// ordinary [`PhysicalRequest`] categories.
#[derive(Debug, Clone, Copy)]
pub struct AtomicRequest<'a> {
    descriptor: AtomicRequestDescriptor,
    ordering: AtomicOrdering,
    operand: Option<&'a [u8]>,
    transform: Option<AmoTransform>,
    store_payload: Option<&'a [u8]>,
    reservation: Option<AtomicReservationContext>,
}

impl<'a> AtomicRequest<'a> {
    /// Creates an RMW envelope carrying the operand bytes and the
    /// Hart-supplied pure transform.
    pub fn rmw(
        paddr: u64,
        width: PhysicalWidth,
        ordering: AtomicOrdering,
        operand: &'a [u8],
        transform: AmoTransform,
    ) -> Result<Self, AtomicProtocolError> {
        let request = Self {
            descriptor: AtomicRequestDescriptor::new(paddr, width, AtomicAccessKind::Rmw),
            ordering,
            operand: Some(operand),
            transform: Some(transform),
            store_payload: None,
            reservation: None,
        };
        request.validate()?;
        Ok(request)
    }

    /// Creates a load-reserved envelope: one atomic-category read that
    /// returns the old bytes and the committed-write snapshot.
    pub fn load_reserved(
        paddr: u64,
        width: PhysicalWidth,
        ordering: AtomicOrdering,
    ) -> Result<Self, AtomicProtocolError> {
        let request = Self {
            descriptor: AtomicRequestDescriptor::new(paddr, width, AtomicAccessKind::LoadReserved),
            ordering,
            operand: None,
            transform: None,
            store_payload: None,
            reservation: None,
        };
        request.validate()?;
        Ok(request)
    }

    /// Creates the existing store-conditional envelope carrying the write
    /// payload and optional Hart reservation context. `None` represents no
    /// reservation; an uncovered context is also permitted so the target can
    /// validate access and capability before reporting conditional failure.
    pub fn store_conditional(
        paddr: u64,
        width: PhysicalWidth,
        ordering: AtomicOrdering,
        payload: &'a [u8],
        reservation: impl Into<Option<AtomicReservationContext>>,
    ) -> Result<Self, AtomicProtocolError> {
        let request = Self {
            descriptor: AtomicRequestDescriptor::new(
                paddr,
                width,
                AtomicAccessKind::StoreConditional,
            ),
            ordering,
            operand: None,
            transform: None,
            store_payload: Some(payload),
            reservation: reservation.into(),
        };
        request.validate()?;
        Ok(request)
    }

    /// Returns the physical start address.
    pub const fn paddr(&self) -> u64 {
        self.descriptor.paddr
    }

    /// Returns the explicit transfer width (4 or 8 bytes only).
    pub const fn width(&self) -> PhysicalWidth {
        self.descriptor.width
    }

    /// Returns the envelope kind.
    pub const fn kind(&self) -> AtomicAccessKind {
        self.descriptor.kind
    }

    /// Returns the informational `aq`/`rl` ordering bits.
    pub const fn ordering(&self) -> AtomicOrdering {
        self.ordering
    }

    /// Returns the address/width/kind identity used for response binding.
    pub const fn descriptor(&self) -> AtomicRequestDescriptor {
        self.descriptor
    }

    /// Returns the request's contiguous physical span.
    pub const fn span(&self) -> PhysicalSpan {
        self.descriptor.span()
    }

    /// Returns the exact RMW operand bytes, or `None` for other kinds.
    pub const fn operand_bytes(&self) -> Option<&'a [u8]> {
        self.operand
    }

    /// Returns the Hart-supplied pure transform, or `None` for other kinds.
    pub const fn transform(&self) -> Option<AmoTransform> {
        self.transform
    }

    /// Returns the exact store-conditional write payload, or `None` for other
    /// kinds.
    pub const fn store_payload(&self) -> Option<&'a [u8]> {
        self.store_payload
    }

    /// Returns the optional Hart reservation context (`None` for an
    /// unreserved SC and for other envelope kinds).
    pub const fn reservation(&self) -> Option<AtomicReservationContext> {
        self.reservation
    }

    /// Rechecks envelope invariants without checking target address routing.
    ///
    /// A non-wrapping envelope span is deliberately not part of this method:
    /// range overflow of the requested span is a target rejection and is
    /// checked by the validated boundary immediately before backend dispatch.
    pub fn validate(&self) -> Result<(), AtomicProtocolError> {
        let width_bytes = self.descriptor.width.bytes();
        let expected_width = AmoWidth::from_physical_width(self.descriptor.width).ok_or(
            AtomicProtocolError::UnsupportedAtomicWidth {
                requested: width_bytes,
            },
        )?;
        match self.descriptor.kind {
            AtomicAccessKind::Rmw => {
                let operand = self.operand.ok_or(AtomicProtocolError::PayloadLength {
                    kind: self.descriptor.kind,
                    expected: width_bytes,
                    actual: 0,
                })?;
                if operand.len() != width_bytes {
                    return Err(AtomicProtocolError::PayloadLength {
                        kind: self.descriptor.kind,
                        expected: width_bytes,
                        actual: operand.len(),
                    });
                }
                let transform = self
                    .transform
                    .ok_or(AtomicProtocolError::MissingTransform)?;
                if transform.width() != expected_width {
                    return Err(AtomicProtocolError::TransformWidthMismatch {
                        expected: width_bytes,
                        actual: transform.width().bytes(),
                    });
                }
            }
            AtomicAccessKind::LoadReserved => {}
            AtomicAccessKind::StoreConditional => {
                let payload = self
                    .store_payload
                    .ok_or(AtomicProtocolError::PayloadLength {
                        kind: self.descriptor.kind,
                        expected: width_bytes,
                        actual: 0,
                    })?;
                if payload.len() != width_bytes {
                    return Err(AtomicProtocolError::PayloadLength {
                        kind: self.descriptor.kind,
                        expected: width_bytes,
                        actual: payload.len(),
                    });
                }
                if let Some(reservation) = self.reservation {
                    if reservation.snapshot.is_empty() {
                        return Err(AtomicProtocolError::EmptySnapshot);
                    }
                    if AmoWidth::from_physical_width(reservation.reserved.width).is_none() {
                        return Err(AtomicProtocolError::UnsupportedAtomicWidth {
                            requested: reservation.reserved.width.bytes(),
                        });
                    }
                    if reservation.reserved.checked_end_inclusive().is_none() {
                        return Err(AtomicProtocolError::ReservationSpanOverflow {
                            reserved: reservation.reserved,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

/// Untrusted backend completion body for an atomic envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicResponseCompletion {
    /// RMW completion: the exact pre-transform span bytes; the transformed
    /// bytes were written in the same critical section.
    Rmw(PhysicalResponseBytes),
    /// Load-reserved completion: the old span bytes plus the committed-write
    /// snapshot of the covered blocks.
    LoadReserved {
        /// The exact old span bytes.
        old_bytes: PhysicalResponseBytes,
        /// The committed-write snapshot at LR time.
        snapshot: CommittedWriteSnapshot,
    },
    /// Conditional store-conditional completion.
    StoreConditional(ConditionalStatus),
}

impl AtomicResponseCompletion {
    /// Returns the completion kind without interpreting any raw bytes.
    pub const fn kind(self) -> AtomicAccessKind {
        match self {
            Self::Rmw(_) => AtomicAccessKind::Rmw,
            Self::LoadReserved { .. } => AtomicAccessKind::LoadReserved,
            Self::StoreConditional(_) => AtomicAccessKind::StoreConditional,
        }
    }
}

/// A backend response carrying an explicit atomic request binding.
///
/// Backends may construct this with [`Self::new`], but the value is untrusted
/// until [`ValidatedAtomicAccess`] checks its binding, completion kind, exact
/// old-byte length, and snapshot presence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtomicResponse {
    binding: AtomicRequestDescriptor,
    completion: AtomicResponseCompletion,
}

impl AtomicResponse {
    /// Creates an untrusted response for an atomic descriptor and completion
    /// body.
    pub const fn new(
        binding: AtomicRequestDescriptor,
        completion: AtomicResponseCompletion,
    ) -> Self {
        Self {
            binding,
            completion,
        }
    }

    /// Creates an RMW response bound to a request, carrying the exact old
    /// span bytes.
    pub fn rmw_for(request: &AtomicRequest<'_>, old_bytes: &[u8]) -> Self {
        Self::new(
            request.descriptor(),
            AtomicResponseCompletion::Rmw(PhysicalResponseBytes::from_slice(old_bytes)),
        )
    }

    /// Creates a load-reserved response bound to a request.
    pub fn load_reserved_for(
        request: &AtomicRequest<'_>,
        old_bytes: &[u8],
        snapshot: CommittedWriteSnapshot,
    ) -> Self {
        Self::new(
            request.descriptor(),
            AtomicResponseCompletion::LoadReserved {
                old_bytes: PhysicalResponseBytes::from_slice(old_bytes),
                snapshot,
            },
        )
    }

    /// Creates a conditional store-conditional response bound to a request.
    pub const fn store_conditional_for(
        request: &AtomicRequest<'_>,
        status: ConditionalStatus,
    ) -> Self {
        Self::new(
            request.descriptor(),
            AtomicResponseCompletion::StoreConditional(status),
        )
    }

    /// Returns the response's request binding.
    pub const fn binding(&self) -> AtomicRequestDescriptor {
        self.binding
    }

    /// Returns the untrusted completion body.
    pub const fn completion(&self) -> AtomicResponseCompletion {
        self.completion
    }

    /// Returns validated old span bytes for RMW and load-reserved
    /// completions, or `None` for a conditional completion.
    pub fn old_bytes(&self) -> Option<&[u8]> {
        match &self.completion {
            AtomicResponseCompletion::Rmw(bytes) => bytes.as_slice(),
            AtomicResponseCompletion::LoadReserved { old_bytes, .. } => old_bytes.as_slice(),
            AtomicResponseCompletion::StoreConditional(_) => None,
        }
    }

    /// Returns the committed-write snapshot of a load-reserved completion.
    pub const fn snapshot(&self) -> Option<CommittedWriteSnapshot> {
        match self.completion {
            AtomicResponseCompletion::LoadReserved { snapshot, .. } => Some(snapshot),
            _ => None,
        }
    }

    /// Returns the conditional status of a store-conditional completion.
    pub const fn conditional_status(&self) -> Option<ConditionalStatus> {
        match self.completion {
            AtomicResponseCompletion::StoreConditional(status) => Some(status),
            _ => None,
        }
    }
}

/// A guest-visible physical target rejection of an atomic envelope with
/// preserved request context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicTargetRejection {
    /// Address/width/kind identity of the rejected envelope.
    pub request: AtomicRequestDescriptor,
    /// Complete span, retained even when its end overflows.
    pub span: PhysicalSpan,
    /// Typed target rejection reason.
    pub reason: PhysicalTargetRejectionReason,
    /// Target-specific diagnostic context.
    pub context: String,
}

impl AtomicTargetRejection {
    /// Creates an atomic target rejection while retaining the request
    /// identity and span.
    pub fn new(
        request: AtomicRequestDescriptor,
        reason: PhysicalTargetRejectionReason,
        context: impl Into<String>,
    ) -> Self {
        Self {
            span: request.span(),
            request,
            reason,
            context: context.into(),
        }
    }
}

impl fmt::Display for AtomicTargetRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "target rejected atomic {:?} at {:#018x} ({} bytes): {}",
            self.request.kind,
            self.request.paddr,
            self.request.width.bytes(),
            self.context
        )
    }
}

impl std::error::Error for AtomicTargetRejection {}

/// A host/backend failure while servicing a valid atomic envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicBackendFailure {
    /// Envelope being serviced when the host/backend failed.
    pub request: AtomicRequestDescriptor,
    /// Typed diagnostic context.
    pub context: String,
}

impl fmt::Display for AtomicBackendFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "atomic backend failure: {}", self.context)
    }
}

impl std::error::Error for AtomicBackendFailure {}

/// An unresolved atomic envelope completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicUnknownCompletion {
    /// Envelope whose effects cannot be established.
    pub request: AtomicRequestDescriptor,
    /// Typed diagnostic context.
    pub context: String,
}

impl fmt::Display for AtomicUnknownCompletion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown atomic completion: {}", self.context)
    }
}

impl std::error::Error for AtomicUnknownCompletion {}

/// Final error taxonomy returned by a validated atomic envelope access.
///
/// The categories deliberately reuse the A7 physical taxonomy (dev-plan §5.7)
/// without weakening it: only [`AtomicAccessError::TargetRejected`] is
/// guest-visible; host, protocol, and unknown completions remain
/// simulator-side failures and are never reclassified from diagnostic text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AtomicAccessError {
    /// A valid envelope was rejected by its physical target.
    #[error("{0}")]
    TargetRejected(AtomicTargetRejection),
    /// The host/backend could not complete a valid envelope.
    #[error("{0}")]
    BackendFailure(AtomicBackendFailure),
    /// Envelope or response protocol/invariant validation failed.
    #[error("{0}")]
    Protocol(AtomicProtocolError),
    /// Completion effects cannot be established; no retry is implied.
    #[error("{0}")]
    UnknownCompletion(AtomicUnknownCompletion),
}

/// Result returned by a validated atomic envelope boundary.
pub type AtomicAccessResult = Result<AtomicResponse, AtomicAccessError>;

/// Result an atomic backend supplies to the validated boundary.
pub type AtomicBackendResult = Result<AtomicResponse, PhysicalBackendError>;

/// The untrusted transport/backend seam for an atomic envelope.
///
/// Implementations execute the single target-visible transaction and report
/// raw old bytes, snapshots, conditional status, and typed failure categories
/// only. For StoreConditional, they must validate the complete store span and
/// target atomic capability before returning conditional Failure for absent
/// or uncovered Hart reservation context. A valid supported target returns
/// Failure without reading/writing guest bytes or committing bookkeeping; an
/// unsupported/denied target reports a target rejection instead. They must
/// never report Success without reservation context. These target checks are
/// not Hart/MMU/PMP checks. Backends do not implement ISA arithmetic,
/// interpret the transform, enter traps, or retry. A backend is not certified
/// merely by implementing this trait; callers must place it behind
/// [`ValidatedAtomicAccess`] to obtain envelope/response validation.
pub trait AtomicBackend {
    /// Services one complete envelope without retaining the borrowed request.
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult;
}

/// The validated transport-neutral atomic envelope interface.
pub trait AtomicAccess {
    /// Validates and submits one complete atomic envelope.
    fn access_atomic(&mut self, request: AtomicRequest<'_>) -> AtomicAccessResult;
}

/// The validated Hart data port: ordinary accesses and atomic envelopes over
/// one shared port object (dev-plan §7.2).
///
/// The Hart's single data port serves ordinary fetches' data sibling — loads
/// and stores — through [`PhysicalAccess`] and AMO/LR/SC through
/// [`AtomicAccess`].  Both categories travel one port handle, so the
/// raw/atomic path locks exactly one port for the complete operation; there
/// is no second port, RAM, or lock domain for atomics.
///
/// This trait is implemented automatically for every port that implements
/// both supertraits, including [`ValidatedPhysicalAccess`] over an
/// [`AtomicBackend`].
pub trait PhysicalDataAccess: PhysicalAccess + AtomicAccess {}

impl<T: PhysicalAccess + AtomicAccess> PhysicalDataAccess for T {}

impl<B: AtomicBackend + ?Sized> AtomicBackend for &mut B {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        (**self).transact_atomic(request)
    }
}

/// Validation boundary around an untrusted atomic backend.
///
/// The wrapper checks envelope widths (4/8 only), kind/payload rules, and the
/// complete non-wrapping span before invoking the backend exactly once.  It
/// then checks response identity, completion kind, exact old-byte length, and
/// snapshot presence.  It has no retry path, including for unknown
/// completion: an unknown atomic completion is terminal.
#[derive(Debug)]
pub struct ValidatedAtomicAccess<B> {
    backend: B,
}

impl<B> ValidatedAtomicAccess<B> {
    /// Wraps an atomic backend in the T1 validation boundary.
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Borrows the wrapped backend for inspection or adapter-specific setup.
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutably borrows the wrapped backend.
    pub const fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Unwraps the backend.
    pub fn into_backend(self) -> B {
        self.backend
    }
}

impl<B: AtomicBackend> ValidatedAtomicAccess<B> {
    /// Convenience method equivalent to the [`AtomicAccess`] trait method.
    pub fn access_atomic(&mut self, request: AtomicRequest<'_>) -> AtomicAccessResult {
        <Self as AtomicAccess>::access_atomic(self, request)
    }

    /// Submits one envelope through the validated boundary.
    pub fn transact_atomic(&mut self, request: AtomicRequest<'_>) -> AtomicAccessResult {
        self.access_atomic(request)
    }
}

impl<B: AtomicBackend> AtomicAccess for ValidatedAtomicAccess<B> {
    fn access_atomic(&mut self, request: AtomicRequest<'_>) -> AtomicAccessResult {
        request.validate().map_err(AtomicAccessError::Protocol)?;

        let descriptor = request.descriptor();
        if !request.span().is_non_wrapping() {
            return Err(AtomicAccessError::TargetRejected(
                AtomicTargetRejection::new(
                    descriptor,
                    PhysicalTargetRejectionReason::RangeOverflow,
                    format!(
                        "atomic span starting at {:#018x} with width {} wraps the address space",
                        descriptor.paddr,
                        descriptor.width.bytes()
                    ),
                ),
            ));
        }

        let response = match self.backend.transact_atomic(&request) {
            Ok(response) => response,
            Err(PhysicalBackendError::Target { reason, context }) => {
                return Err(AtomicAccessError::TargetRejected(
                    AtomicTargetRejection::new(descriptor, reason, context),
                ));
            }
            Err(PhysicalBackendError::Host { context }) => {
                return Err(AtomicAccessError::BackendFailure(AtomicBackendFailure {
                    request: descriptor,
                    context,
                }));
            }
            Err(PhysicalBackendError::Protocol { context }) => {
                return Err(AtomicAccessError::Protocol(
                    AtomicProtocolError::BackendProtocol { context },
                ));
            }
            Err(PhysicalBackendError::Unknown { context }) => {
                return Err(AtomicAccessError::UnknownCompletion(
                    AtomicUnknownCompletion {
                        request: descriptor,
                        context,
                    },
                ));
            }
        };

        validate_atomic_response(&request, response)
    }
}

fn validate_atomic_response(
    request: &AtomicRequest<'_>,
    response: AtomicResponse,
) -> AtomicAccessResult {
    let expected_binding = request.descriptor();
    if response.binding() != expected_binding {
        return Err(AtomicAccessError::Protocol(
            AtomicProtocolError::ResponseBindingMismatch {
                expected: expected_binding,
                actual: response.binding(),
            },
        ));
    }

    let completion = response.completion();
    let expected_kind = request.kind();
    if completion.kind() != expected_kind {
        return Err(AtomicAccessError::Protocol(
            AtomicProtocolError::ResponseCompletionMismatch {
                expected: expected_kind,
                actual: completion.kind(),
            },
        ));
    }
    if matches!(
        completion,
        AtomicResponseCompletion::StoreConditional(ConditionalStatus::Success)
    ) {
        let Some(reservation) = request.reservation() else {
            return Err(AtomicAccessError::Protocol(
                AtomicProtocolError::ConditionalSuccessWithoutReservation,
            ));
        };
        let requested = request.span();
        let covered = reservation.reserved.paddr <= requested.paddr
            && requested
                .checked_end_inclusive()
                .zip(reservation.reserved.checked_end_inclusive())
                .is_some_and(|(requested_end, reserved_end)| requested_end <= reserved_end);
        if !covered {
            return Err(AtomicAccessError::Protocol(
                AtomicProtocolError::ConditionalSuccessOutsideReservation {
                    reserved: reservation.reserved,
                    requested,
                },
            ));
        }
    }

    match completion {
        AtomicResponseCompletion::Rmw(bytes)
        | AtomicResponseCompletion::LoadReserved {
            old_bytes: bytes, ..
        } => {
            validate_atomic_old_bytes(request, bytes)?;
        }
        AtomicResponseCompletion::StoreConditional(_) => {}
    }

    if let AtomicResponseCompletion::LoadReserved { snapshot, .. } = completion {
        if snapshot.is_empty() {
            return Err(AtomicAccessError::Protocol(
                AtomicProtocolError::EmptySnapshot,
            ));
        }
    }

    Ok(response)
}

fn validate_atomic_old_bytes(
    request: &AtomicRequest<'_>,
    bytes: PhysicalResponseBytes,
) -> Result<(), AtomicAccessError> {
    if bytes.supplied_len() != bytes.reported_len() {
        return Err(AtomicAccessError::Protocol(
            AtomicProtocolError::ResponsePayloadLengthMismatch {
                reported: bytes.reported_len(),
                supplied: bytes.supplied_len(),
            },
        ));
    }

    let expected_len = request.width().bytes();
    if bytes.reported_len() != expected_len {
        return Err(AtomicAccessError::Protocol(
            AtomicProtocolError::ResponseLengthMismatch {
                expected: expected_len,
                actual: bytes.reported_len(),
            },
        ));
    }
    // Widths are limited to eight bytes, so an exact length always has an
    // inline slice.  Retain an explicit invariant check rather than allowing
    // a malformed future representation to become success.
    if bytes.as_slice().is_none() {
        return Err(AtomicAccessError::Protocol(
            AtomicProtocolError::Invariant {
                context: "exact atomic response length has no inline byte representation".into(),
            },
        ));
    }
    Ok(())
}
