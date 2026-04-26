use std::io::Read;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use color_eyre::Result;

use crate::adb::Adb;

use super::{VitalsEvent, attach_running, decode_payload};

/// Owns the entire host-side path to one attached agent: pushes/attaches the
/// agent if needed, opens the abstract socket forward, and streams decoded
/// events on `rx`. Drop tears everything down.
pub struct VitalsHandle {
    pub rx: mpsc::Receiver<VitalsEvent>,
    adb: Arc<dyn Adb>,
    serial: String,
    local_port: u16,
    stop: Arc<AtomicBool>,
}

impl VitalsHandle {
    pub fn spawn(
        adb: Arc<dyn Adb>,
        serial: String,
        package: String,
        pid: u32,
    ) -> Result<Self> {
        let abstract_name = format!("holoagent-{pid}");
        let local_port = adb.forward_abstract(&serial, &abstract_name)?;

        // Attach in parallel with the reader. Idempotent: if cold-start
        // `--attach-agent` already loaded the agent, the device-side
        // `Agent_OnAttach` short-circuits via its `g_started` flag.
        let attach_adb = adb.clone();
        let attach_serial = serial.clone();
        std::thread::spawn(move || {
            let _ = attach_running(&*attach_adb, &attach_serial, &package);
        });

        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        std::thread::spawn(move || run_reader(local_port, tx, stop_thread));

        Ok(Self { rx, adb, serial, local_port, stop })
    }
}

impl Drop for VitalsHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.adb.forward_remove(&self.serial, self.local_port);
    }
}

/// Connect to the forwarded loopback port (the agent's `bind()` may not have
/// run yet, so we retry), then decode framed events until EOF or `stop`.
fn run_reader(port: u16, tx: mpsc::Sender<VitalsEvent>, stop: Arc<AtomicBool>) {
    if let Some(stream) = connect_with_retry(port, &stop) {
        pump_frames(stream, &tx, &stop);
    }
}

/// Frame size guard: ignore any frame claiming a payload larger than this.
/// 64 KiB is far more than any current event needs and protects us from
/// runaway lengths if the wire format ever desyncs.
const MAX_PAYLOAD: usize = 1 << 16;

/// Decode `[u8 kind][u32 len_be][payload]` frames from `stream` and send
/// recognized events on `tx` until EOF, `stop`, or a malformed frame.
fn pump_frames<R: Read>(
    mut stream: R,
    tx: &mpsc::Sender<VitalsEvent>,
    stop: &AtomicBool,
) {
    let mut header = [0u8; 5];
    let mut payload = Vec::with_capacity(64);
    loop {
        if stop.load(Ordering::Relaxed) || !read_full(&mut stream, &mut header, stop) {
            return;
        }
        let kind = header[0];
        let len = u32::from_be_bytes(header[1..5].try_into().unwrap()) as usize;
        if len > MAX_PAYLOAD {
            return;
        }
        payload.resize(len, 0);
        if !read_full(&mut stream, &mut payload, stop) {
            return;
        }
        if let Some(event) = decode_payload(kind, &payload)
            && tx.send(event).is_err()
        {
            return;
        }
    }
}

const CONNECT_DEADLINE: Duration = Duration::from_secs(30);

/// Probe the forwarded port until the device-side abstract socket is bound.
/// `adb forward` accepts on the host immediately, but if the agent hasn't
/// called `bind()` yet adb closes the connection on first read (EOF). One
/// successful peek byte (or a connected, idle socket) means the agent is up.
fn connect_with_retry(port: u16, stop: &AtomicBool) -> Option<PrimedStream<TcpStream>> {
    let addr = format!("127.0.0.1:{port}").parse().expect("valid loopback");
    let deadline = Instant::now() + CONNECT_DEADLINE;
    while !stop.load(Ordering::Relaxed) {
        if let Ok(s) = TcpStream::connect_timeout(&addr, Duration::from_secs(1)) {
            let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 1];
            match (&s).read(&mut buf) {
                Ok(0) => {} // EOF — agent not bound yet, retry
                Ok(n) => return Some(PrimedStream::new(s, &buf[..n])),
                Err(e) if is_timeout(&e) => return Some(PrimedStream::new(s, &[])),
                Err(_) => {}
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    None
}

fn is_timeout(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
}

/// Stream wrapper that returns bytes captured during the connect probe before
/// reading from the underlying socket.
struct PrimedStream<R: Read> {
    inner: R,
    prefix: Vec<u8>,
}

impl<R: Read> PrimedStream<R> {
    fn new(inner: R, prefix: &[u8]) -> Self {
        Self { inner, prefix: prefix.to_vec() }
    }
}

impl<R: Read> Read for PrimedStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.prefix.is_empty() {
            let n = buf.len().min(self.prefix.len());
            buf[..n].copy_from_slice(&self.prefix[..n]);
            self.prefix.drain(..n);
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

fn read_full<R: Read>(stream: &mut R, buf: &mut [u8], stop: &AtomicBool) -> bool {
    let mut filled = 0;
    while filled < buf.len() {
        match stream.read(&mut buf[filled..]) {
            Ok(0) => return false,
            Ok(n) => filled += n,
            Err(e) if is_timeout(&e) => {
                if stop.load(Ordering::Relaxed) { return false; }
            }
            Err(_) => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read_to_end<R: Read>(mut r: R) -> Vec<u8> {
        let mut buf = Vec::new();
        r.read_to_end(&mut buf).unwrap();
        buf
    }

    #[test]
    fn primed_stream_yields_prefix_then_inner() {
        let stream = PrimedStream::new(Cursor::new(b"world".to_vec()), b"hello-");
        assert_eq!(read_to_end(stream), b"hello-world");
    }

    #[test]
    fn primed_stream_with_empty_prefix_delegates_immediately() {
        let stream = PrimedStream::new(Cursor::new(b"abc".to_vec()), &[]);
        assert_eq!(read_to_end(stream), b"abc");
    }

    fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn gc_payload(ts_ns: i64, duration_us: u32) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&ts_ns.to_be_bytes());
        p.extend_from_slice(&duration_us.to_be_bytes());
        p
    }

    fn run_pump(bytes: Vec<u8>) -> Vec<VitalsEvent> {
        let (tx, rx) = mpsc::channel();
        let stop = AtomicBool::new(false);
        pump_frames(Cursor::new(bytes), &tx, &stop);
        drop(tx);
        rx.iter().collect()
    }

    #[test]
    fn pump_decodes_consecutive_frames() {
        let mut wire = Vec::new();
        wire.extend(frame(crate::vitals::KIND_GC, &gc_payload(100, 250)));
        wire.extend(frame(crate::vitals::KIND_GC, &gc_payload(200, 500)));

        let events = run_pump(wire);
        assert_eq!(
            events,
            vec![
                VitalsEvent::Gc { ts_ns: 100, duration_us: 250 },
                VitalsEvent::Gc { ts_ns: 200, duration_us: 500 },
            ],
        );
    }

    #[test]
    fn pump_skips_unknown_kinds_but_keeps_streaming() {
        let mut wire = Vec::new();
        wire.extend(frame(0x42, &[0, 1, 2, 3])); // unknown kind, valid frame
        wire.extend(frame(crate::vitals::KIND_GC, &gc_payload(7, 8)));

        let events = run_pump(wire);
        assert_eq!(events, vec![VitalsEvent::Gc { ts_ns: 7, duration_us: 8 }]);
    }

    #[test]
    fn pump_stops_on_oversized_length() {
        // Hand-craft a header claiming a payload one byte over the cap. The
        // body bytes that follow must NOT be decoded as a fresh frame.
        let huge = (MAX_PAYLOAD + 1) as u32;
        let mut wire = vec![crate::vitals::KIND_GC];
        wire.extend_from_slice(&huge.to_be_bytes());
        // Append a perfectly valid GC frame that pump should never reach:
        wire.extend(frame(crate::vitals::KIND_GC, &gc_payload(1, 1)));

        assert!(run_pump(wire).is_empty());
    }

    #[test]
    fn pump_returns_cleanly_on_mid_frame_eof() {
        // Header says len=12, body has only 4 bytes — read_full returns false.
        let mut wire = vec![crate::vitals::KIND_GC];
        wire.extend_from_slice(&12u32.to_be_bytes());
        wire.extend_from_slice(&[0u8; 4]);

        assert!(run_pump(wire).is_empty());
    }

    #[test]
    fn pump_returns_when_stop_is_set() {
        let wire = frame(crate::vitals::KIND_GC, &gc_payload(1, 1));
        let (tx, _rx) = mpsc::channel();
        let stop = AtomicBool::new(true);
        pump_frames(Cursor::new(wire), &tx, &stop);
        // No events observed; pump exited at the top-of-loop check.
    }

    #[test]
    fn primed_stream_drains_prefix_in_chunks() {
        let mut stream = PrimedStream::new(Cursor::new(b"XYZ".to_vec()), b"ABCDE");
        let mut chunk = [0u8; 2];

        // Two-byte reads consume the 5-byte prefix one chunk at a time before
        // touching the inner reader.
        assert_eq!(stream.read(&mut chunk).unwrap(), 2);
        assert_eq!(&chunk, b"AB");
        assert_eq!(stream.read(&mut chunk).unwrap(), 2);
        assert_eq!(&chunk, b"CD");
        assert_eq!(stream.read(&mut chunk).unwrap(), 1);
        assert_eq!(chunk[0], b'E');
        assert_eq!(stream.read(&mut chunk).unwrap(), 2);
        assert_eq!(&chunk, b"XY");
    }
}
