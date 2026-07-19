use crate::CodexError;
use serde_json::{Map, Value, json};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::process::{Child, ChildStdin};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const MAX_JSONL_BYTES: usize = 8 * 1024 * 1024;
const READER_QUEUE_CAPACITY: usize = 8;

#[derive(Debug)]
pub(crate) enum Incoming {
    Response {
        id: i64,
        result: Option<Value>,
        error: Option<WireError>,
    },
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
    },
}

#[derive(Debug)]
pub(crate) struct WireError {
    pub code: i64,
    pub message: String,
}

enum ReaderEvent {
    Line(Vec<u8>),
    Oversized,
    Io,
    Eof,
}

pub(crate) struct Transport {
    child: Option<Child>,
    writer: Option<BufWriter<ChildStdin>>,
    receiver: Receiver<ReaderEvent>,
    reader_overflow: Arc<AtomicBool>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
    process_lease_bound: bool,
}

impl Transport {
    pub(crate) fn new(mut child: Child) -> Result<Self, CodexError> {
        let stdin = child
            .stdin
            .take()
            .ok_or(CodexError::Process("missing stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(CodexError::Process("missing stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(CodexError::Process("missing stderr"))?;
        let (sender, receiver) = mpsc::sync_channel(READER_QUEUE_CAPACITY);
        let reader_overflow = Arc::new(AtomicBool::new(false));
        let overflow = reader_overflow.clone();
        let stdout_thread = thread::spawn(move || read_stdout(stdout, &sender, &overflow));
        let stderr_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut buffer = [0_u8; 8192];
            while reader.read(&mut buffer).is_ok_and(|read| read != 0) {}
        });
        Ok(Self {
            child: Some(child),
            writer: Some(BufWriter::new(stdin)),
            receiver,
            reader_overflow,
            stdout_thread: Some(stdout_thread),
            stderr_thread: Some(stderr_thread),
            process_lease_bound: false,
        })
    }

    /// Permanently removes numeric-signal authority from this transport after
    /// the exact child incarnation has been bound into the protected broker
    /// lease. From this point on, only the broker may signal that process by
    /// Mach audit token; Core can close its pipe and reap the eventual exit.
    pub(crate) const fn mark_process_lease_bound(&mut self) {
        self.process_lease_bound = true;
    }

    pub(crate) fn send_request(
        &mut self,
        id: i64,
        method: &str,
        params: &Value,
    ) -> Result<(), CodexError> {
        self.send_value(&json!({"id": id, "method": method, "params": params}))
    }

    pub(crate) fn send_notification(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), CodexError> {
        let mut object = Map::from_iter([("method".to_owned(), Value::String(method.to_owned()))]);
        if let Some(params) = params {
            object.insert("params".to_owned(), params);
        }
        self.send_value(&Value::Object(object))
    }

    pub(crate) fn send_server_rejection(
        &mut self,
        id: &Value,
        method: &str,
    ) -> Result<(), CodexError> {
        let result = match method {
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                Some(json!({"decision": "cancel"}))
            }
            "execCommandApproval" | "applyPatchApproval" => Some(json!({"decision": "abort"})),
            "mcpServer/elicitation/request" => Some(json!({"action": "cancel"})),
            "item/tool/call" => Some(json!({
                "contentItems": [{"text": "OpenOpen does not expose dynamic tools", "type": "inputText"}],
                "success": false
            })),
            _ => None,
        };
        match result {
            Some(result) => self.send_value(&json!({"id": id, "result": result})),
            None => self.send_value(&json!({
                "error": {"code": -32601, "message": "OpenOpen rejects server-initiated authority"},
                "id": id
            })),
        }
    }

    pub(crate) fn recv(&mut self, timeout: Duration) -> Result<Incoming, CodexError> {
        if self.reader_overflow.load(Ordering::Acquire) {
            return Err(CodexError::Protocol("app-server output queue overflow"));
        }
        let event = match self.receiver.recv_timeout(timeout) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => return Err(CodexError::Timeout),
            Err(RecvTimeoutError::Disconnected) if self.reader_overflow.load(Ordering::Acquire) => {
                return Err(CodexError::Protocol("app-server output queue overflow"));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(CodexError::Process("app-server output reader stopped"));
            }
        };
        match event {
            ReaderEvent::Line(line) => parse_line(&line),
            ReaderEvent::Oversized => Err(CodexError::Protocol("oversized JSONL frame")),
            ReaderEvent::Io => Err(CodexError::Process("stdout read failed")),
            ReaderEvent::Eof => Err(CodexError::Process("unexpected app-server EOF")),
        }
    }

    fn send_value(&mut self, value: &Value) -> Result<(), CodexError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or(CodexError::Process("app-server stdin closed"))?;
        serde_json::to_writer(&mut *writer, value)
            .map_err(|_| CodexError::Protocol("request serialization failed"))?;
        writer
            .write_all(b"\n")
            .and_then(|()| writer.flush())
            .map_err(|_| CodexError::Process("stdin write failed"))
    }

    pub(crate) fn terminate(&mut self) {
        if let Some(mut writer) = self.writer.take() {
            let _ = writer.flush();
            drop(writer);
        }
        let Some(mut child) = self.child.take() else {
            return;
        };
        if self.process_lease_bound {
            let stdout_thread = self.stdout_thread.take();
            let stderr_thread = self.stderr_thread.take();
            let _ = thread::Builder::new()
                .name("openopen-codex-reaper".to_owned())
                .spawn(move || {
                    // Deliberately wait-only: the broker owns the sole
                    // post-lease signal authority and targets the exact Mach
                    // audit-token process incarnation. This thread only reaps
                    // that child after pipe EOF or broker termination.
                    let _ = child.wait();
                    if let Some(handle) = stdout_thread {
                        let _ = handle.join();
                    }
                    if let Some(handle) = stderr_thread {
                        let _ = handle.join();
                    }
                });
            return;
        }
        let _ = child.kill();
        let _ = child.wait();
        if let Some(handle) = self.stdout_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn read_stdout(stdout: impl Read, sender: &SyncSender<ReaderEvent>, overflow: &AtomicBool) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    loop {
        let available = match reader.fill_buf() {
            Ok([]) => {
                let _ = send_reader_event(
                    sender,
                    overflow,
                    if line.is_empty() {
                        ReaderEvent::Eof
                    } else {
                        ReaderEvent::Io
                    },
                );
                return;
            }
            Ok(available) => available,
            Err(_) => {
                let _ = send_reader_event(sender, overflow, ReaderEvent::Io);
                return;
            }
        };
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let payload_len = newline.unwrap_or(available.len());
        if line.len().saturating_add(payload_len) > MAX_JSONL_BYTES {
            let _ = send_reader_event(sender, overflow, ReaderEvent::Oversized);
            return;
        }
        line.extend_from_slice(&available[..payload_len]);
        reader.consume(consumed);
        if newline.is_some() {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if !send_reader_event(
                sender,
                overflow,
                ReaderEvent::Line(std::mem::take(&mut line)),
            ) {
                return;
            }
        }
    }
}

fn send_reader_event(
    sender: &SyncSender<ReaderEvent>,
    overflow: &AtomicBool,
    event: ReaderEvent,
) -> bool {
    match sender.try_send(event) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            overflow.store(true, Ordering::Release);
            false
        }
        Err(TrySendError::Disconnected(_)) => false,
    }
}

pub(crate) fn parse_line(line: &[u8]) -> Result<Incoming, CodexError> {
    let value: Value = serde_json::from_slice(line)
        .map_err(|_| CodexError::Protocol("malformed app-server JSON"))?;
    let object = value
        .as_object()
        .ok_or(CodexError::Protocol("app-server frame is not an object"))?;
    if object.contains_key("jsonrpc") {
        return Err(CodexError::Protocol("unexpected jsonrpc field"));
    }
    let method = object.get("method").and_then(Value::as_str);
    let id = object.get("id");
    if let Some(method) = method {
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        return match id {
            Some(id) if valid_request_id(id) => Ok(Incoming::Request {
                id: id.clone(),
                method: method.to_owned(),
            }),
            None => Ok(Incoming::Notification {
                method: method.to_owned(),
                params,
            }),
            _ => Err(CodexError::Protocol("invalid server request id")),
        };
    }
    let id = id
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
        .ok_or(CodexError::Protocol("invalid response id"))?;
    let result = object.get("result").cloned();
    let error = object.get("error");
    if result.is_some() == error.is_some() {
        return Err(CodexError::Protocol("ambiguous app-server response"));
    }
    let error = error
        .map(|value| {
            let object = value
                .as_object()
                .ok_or(CodexError::Protocol("invalid error response"))?;
            Ok(WireError {
                code: object
                    .get("code")
                    .and_then(Value::as_i64)
                    .ok_or(CodexError::Protocol("missing error code"))?,
                message: object
                    .get("message")
                    .and_then(Value::as_str)
                    .ok_or(CodexError::Protocol("missing error message"))?
                    .chars()
                    .take(256)
                    .collect(),
            })
        })
        .transpose()?;
    Ok(Incoming::Response { id, result, error })
}

fn valid_request_id(value: &Value) -> bool {
    value.as_i64().is_some_and(|id| id >= 0)
        || value
            .as_str()
            .is_some_and(|id| !id.is_empty() && id.len() <= 128)
}

#[cfg(test)]
mod tests {
    use super::{Incoming, Transport, parse_line, read_stdout};
    use std::fs;
    use std::io::Cursor;
    use std::process::{Command, Stdio};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::Duration;

    #[test]
    fn codex_wire_omits_jsonrpc_and_requires_unambiguous_response() {
        assert!(matches!(
            parse_line(br#"{"id":1,"result":{}}"#).unwrap(),
            Incoming::Response { id: 1, .. }
        ));
        assert!(parse_line(br#"{"jsonrpc":"2.0","id":1,"result":{}}"#).is_err());
        assert!(parse_line(br#"{"id":1,"result":{},"error":{"code":1,"message":"x"}}"#).is_err());
    }

    #[test]
    fn server_requests_and_notifications_remain_distinct() {
        assert!(matches!(
            parse_line(
                br#"{"id":"approval-1","method":"item/fileChange/requestApproval","params":{}}"#
            )
            .unwrap(),
            Incoming::Request { .. }
        ));
        assert!(matches!(
            parse_line(br#"{"method":"turn/completed","params":{}}"#).unwrap(),
            Incoming::Notification { .. }
        ));
    }

    #[test]
    fn stdout_reader_fails_closed_without_blocking_when_queue_is_full() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let overflow = AtomicBool::new(false);
        read_stdout(Cursor::new(b"{}\n{}\n"), &sender, &overflow);
        assert!(overflow.load(Ordering::Acquire));
        assert!(receiver.try_recv().is_ok());
    }

    #[test]
    fn leased_transport_closes_stdin_and_reaps_without_signalling_numeric_pid() {
        let marker =
            std::env::temp_dir().join(format!("openopen-leased-transport-{}", std::process::id()));
        let _ = fs::remove_file(&marker);
        let child = Command::new("/bin/sh")
            .arg("-c")
            .arg("read _ || printf closed > \"$1\"")
            .arg("openopen-leased-transport")
            .arg(&marker)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut transport = Transport::new(child).unwrap();
        transport.mark_process_lease_bound();
        transport.terminate();
        for _ in 0..100 {
            if matches!(fs::read_to_string(&marker).as_deref(), Ok("closed")) {
                let _ = fs::remove_file(&marker);
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("leased child did not observe pipe closure");
    }
}
