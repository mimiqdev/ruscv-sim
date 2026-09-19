//! Transport-neutral non-atomic physical-access vocabulary.
//!
//! This module is the A7 T1 seam only.  It describes one complete fetch, data
//! read, or data write using a physical address, an explicit byte width, and
//! raw bytes.  It deliberately does not provide a native address map, adapt
//! [`crate::memory::MemoryInterface`], interpret integer or floating-point
//! values, map faults to Hart traps, or model an AMO/LR/SC operation envelope.
//!
//! [`ValidatedPhysicalAccess`] is the small validation boundary intended for a
//! future native target adapter.  A backend reports typed target, host, protocol,
//! or unknown-completion results; the boundary validates the request before the
//! backend is called and validates every response before returning success.
//! Valid requests and responses use fixed-size storage (at most eight bytes),
//! so the boundary itself does not add a heap allocation to an ordinary step.
//! Error context is owned only on failure paths.

use std::fmt;

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
/// There is intentionally no atomic category here.  AMO/LR/SC operation
/// envelopes remain outside T1 and must not be represented as an ordinary
/// read/write pair by this interface.
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
