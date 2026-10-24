//! `sasslint-mcp`: a Model Context Protocol server over stdio (newline-delimited
//! JSON-RPC 2.0) exposing the `review` tool. stdout carries ONLY protocol messages;
//! no logging is written there (so framing stays clean).

use std::io::{self, BufRead, Write};

use sasslint::handle_message;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    stdin.lock().lines().for_each(|line| {
        let Ok(Ok(req)) = line.map(|l| serde_json::from_str(l.trim())) else {
            return;
        };
        if let Some(resp) = handle_message(&req) {
            serde_json::to_writer(&mut out, &resp).ok();
            out.write_all(b"\n").ok();
            out.flush().ok();
        }
    });
}
