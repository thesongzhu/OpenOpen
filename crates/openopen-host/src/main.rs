use openopen_host::{Host, HostPaths, read_bootstrap};
use openopen_protocol::{RpcError, RpcResponse};
use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use zeroize::Zeroize;

const MAX_RPC_FRAME_BYTES: usize = 8 * 1024 * 1024;
const RESPONSE_QUEUE_CAPACITY: usize = 32;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A private process group remains a cooperative containment layer. The
    // security boundary is the root broker's signed lease over the exact Core
    // and persistent Codex audit tokens, not this mutable PGID.
    rustix::process::setpgid(None, None)?;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let paths = HostPaths::production()?;
    let mut master = read_bootstrap(&mut input)?;
    let host = Host::open(paths, master);
    master.zeroize();
    let mut host = host?;

    let (responses, output) = mpsc::sync_channel(RESPONSE_QUEUE_CAPACITY);
    let writer = std::thread::spawn(move || -> io::Result<()> {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        for response in output {
            let encoded = encode_bounded_response(&response)?;
            stdout.write_all(&encoded)?;
            writeln!(stdout)?;
            stdout.flush()?;
        }
        Ok(())
    });

    let mut frame = Vec::new();
    loop {
        frame.clear();
        if read_bounded_frame(&mut input, &mut frame)? == 0 {
            break;
        }
        if frame.last() == Some(&b'\n') {
            frame.pop();
        }
        if frame.last() == Some(&b'\r') {
            frame.pop();
        }
        let line = std::str::from_utf8(&frame)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "RPC frame is not UTF-8"))?;
        host.handle_line(line, &responses);
    }
    drop(host);
    drop(responses);
    writer.join().map_err(|_| "response writer panicked")??;
    Ok(())
}

fn encode_bounded_response(response: &RpcResponse) -> io::Result<Vec<u8>> {
    let id = response.id;
    let encoded = serde_json::to_vec(&response)?;
    if encoded.len() <= MAX_RPC_FRAME_BYTES {
        return Ok(encoded);
    }
    serde_json::to_vec(&RpcResponse::failure(
        id,
        RpcError {
            code: -32_013,
            message: "Core response exceeded the 8 MiB limit".to_owned(),
            data: None,
        },
    ))
    .map_err(io::Error::other)
}

fn read_bounded_frame(input: &mut impl BufRead, frame: &mut Vec<u8>) -> io::Result<usize> {
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(frame.len());
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        let content_take = take - usize::from(available.get(take - 1) == Some(&b'\n'));
        if frame
            .len()
            .checked_add(content_take)
            .is_none_or(|length| length > MAX_RPC_FRAME_BYTES)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "RPC frame exceeds the 8 MiB limit",
            ));
        }
        frame.extend_from_slice(&available[..take]);
        input.consume(take);
        if frame.last() == Some(&b'\n') {
            return Ok(frame.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn bounded_frame_accepts_exact_limit_and_newline() {
        let mut bytes = vec![b'x'; MAX_RPC_FRAME_BYTES];
        bytes.push(b'\n');
        let mut frame = Vec::new();
        assert_eq!(
            read_bounded_frame(&mut Cursor::new(bytes), &mut frame).unwrap(),
            MAX_RPC_FRAME_BYTES + 1
        );
    }

    #[test]
    fn bounded_frame_rejects_limit_plus_one_with_or_without_newline() {
        for newline in [false, true] {
            let mut bytes = vec![b'x'; MAX_RPC_FRAME_BYTES + 1];
            if newline {
                bytes.push(b'\n');
            }
            let error = read_bounded_frame(&mut Cursor::new(bytes), &mut Vec::new()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        }
    }

    #[test]
    fn oversized_outbound_response_is_replaced_by_bounded_failure() {
        let response = RpcResponse::success(
            9,
            serde_json::Value::String("x".repeat(MAX_RPC_FRAME_BYTES)),
        );
        let encoded = encode_bounded_response(&response).unwrap();
        assert!(encoded.len() < MAX_RPC_FRAME_BYTES);
        let decoded: RpcResponse = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.id, Some(9));
        assert_eq!(decoded.error.unwrap().code, -32_013);
    }
}
