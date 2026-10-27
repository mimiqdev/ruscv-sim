//! TCP `remote_bitbang` session polled by the hart run loop.
//!
//! D12's K=1 rule: while the hart is running, the loop calls [`BitbangSession::poll`]
//! at every Hart boundary, before the next fetch, and does not block. A halted
//! hart, or a WFI that is waiting for a debugger, blocks inside that poll
//! until the client sends a command or closes the socket. DMI reads and writes
//! run on the hart thread through the caller's [`super::DmiBus`], so they are
//! already at a boundary.

use super::{BitbangReply, DmiBus, DmiFault, JtagDtm, RemoteBitbangConfig};
use crate::machine::DebugPort;
use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::Ordering;

pub(crate) struct BitbangSession {
    listener: TcpListener,
    client: Option<TcpStream>,
    tap: JtagDtm,
    /// Keeps the Debug Module attached until the run drops this session.
    _port: DebugPort,
}

impl BitbangSession {
    pub(crate) fn bind(
        config: &RemoteBitbangConfig,
        port_handle: DebugPort,
    ) -> Result<Self, String> {
        let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, config.port));
        let listener = TcpListener::bind(address)
            .map_err(|error| format!("remote_bitbang bind {}: {error}", config.port))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let bound = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        if let Some(slot) = &config.bound_port {
            slot.store(bound, Ordering::Release);
        }
        eprintln!("Listening for remote bitbang connection on port {bound}.");
        Ok(Self {
            listener,
            client: None,
            tap: JtagDtm::new(),
            _port: port_handle,
        })
    }

    /// Take commands that are already buffered. `block` waits for the next
    /// one when the socket is idle.
    ///
    /// One call drains every byte that is already available, so a `haltreq`
    /// later in the same burst is applied before the caller fetches. Returns
    /// whether the transport has closed.
    pub(crate) fn poll(&mut self, bus: &mut dyn DmiBus, mut block: bool) -> Result<bool, String> {
        loop {
            if !self.ensure_client(block)? {
                return Ok(false);
            }
            match self.read_chunk(block)? {
                Chunk::Idle => return Ok(false),
                Chunk::Eof => return Ok(true),
                Chunk::Data(bytes) => {
                    if self.apply(&bytes, bus)? {
                        return Ok(true);
                    }
                    // Further bytes must not wait. The caller decides whether
                    // the hart is still halted after this burst.
                    block = false;
                }
            }
        }
    }

    fn read_chunk(&mut self, block: bool) -> Result<Chunk, String> {
        let mut buf = [0_u8; 512];
        let read = {
            let stream = self
                .client
                .as_mut()
                .expect("client checked by ensure_client");
            // A zero `read_timeout` is rejected on Linux. Nonblocking is the
            // idle poll; a halted hart or a waiting WFI blocks in `read`.
            stream
                .set_nonblocking(!block)
                .map_err(|error| error.to_string())?;
            let result = stream.read(&mut buf);
            if block {
                stream
                    .set_nonblocking(true)
                    .map_err(|error| error.to_string())?;
            }
            result
        };
        match read {
            Ok(0) => Ok(Chunk::Eof),
            Ok(n) => Ok(Chunk::Data(buf[..n].to_vec())),
            Err(error) if is_idle(&error) => Ok(Chunk::Idle),
            Err(error) => Err(error.to_string()),
        }
    }

    fn ensure_client(&mut self, block: bool) -> Result<bool, String> {
        if self.client.is_some() {
            return Ok(true);
        }
        match self.listener.accept() {
            Ok((stream, _)) => {
                prepare_client(&stream)?;
                self.client = Some(stream);
                Ok(true)
            }
            Err(error) if is_idle(&error) && !block => Ok(false),
            Err(error) if is_idle(&error) => {
                self.listener
                    .set_nonblocking(false)
                    .map_err(|error| error.to_string())?;
                let accepted = self.listener.accept();
                self.listener
                    .set_nonblocking(true)
                    .map_err(|error| error.to_string())?;
                let (stream, _) = accepted.map_err(|error| error.to_string())?;
                prepare_client(&stream)?;
                self.client = Some(stream);
                Ok(true)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn apply(&mut self, bytes: &[u8], bus: &mut dyn DmiBus) -> Result<bool, String> {
        for &byte in bytes {
            let BitbangReply { tdo, quit } =
                self.tap.command(byte, bus).map_err(|error| match error {
                    DmiFault::Failed => "DMI access failed".to_string(),
                    DmiFault::Fatal(message) => message,
                })?;
            if let Some(tdo) = tdo {
                self.client
                    .as_mut()
                    .expect("client checked before apply")
                    .write_all(&[tdo])
                    .map_err(|error| error.to_string())?;
            }
            if quit {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

enum Chunk {
    Idle,
    Eof,
    Data(Vec<u8>),
}

fn prepare_client(stream: &TcpStream) -> Result<(), String> {
    stream
        .set_nodelay(true)
        .map_err(|error| error.to_string())?;
    stream
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn is_idle(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    )
}
