//! `sasslint-mcp`：一个跑在 stdio 上的 Model Context Protocol 服务器（换行分隔的 JSON-RPC 2.0），
//! 对外提供 `review` 工具。stdout **只**承载协议消息；日志一律不往那里写（这样分帧才能保持干净）。

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
