use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct FixtureHttpRequest {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: String,
}

pub fn run_fixture_http_server_once(
    response_json: &str,
) -> (
    String,
    Arc<Mutex<Option<FixtureHttpRequest>>>,
    JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture http server");
    let addr = listener.local_addr().expect("fixture local addr");
    let captured = Arc::new(Mutex::new(None));
    let captured_bg = Arc::clone(&captured);
    let response = response_json.to_string();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("fixture accept");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("fixture set read timeout");
        let mut buf = vec![0u8; 64 * 1024];
        let mut req = Vec::new();
        loop {
            let n = stream.read(&mut buf).expect("fixture read");
            if n == 0 {
                break;
            }
            req.extend_from_slice(&buf[..n]);
            if req.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let headers_end = req
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| i + 4)
            .expect("fixture request headers");
        let head = String::from_utf8_lossy(&req[..headers_end]).to_string();
        let mut lines = head.lines();
        let req_line = lines.next().unwrap_or_default();
        let mut req_parts = req_line.split_whitespace();
        let method = req_parts.next().unwrap_or_default().to_string();
        let path = req_parts.next().unwrap_or_default().to_string();
        let mut content_len = 0usize;
        let mut auth = None;
        for line in lines {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("content-length:")
                && let Some(v) = line.split(':').nth(1)
            {
                content_len = v.trim().parse::<usize>().unwrap_or(0);
            }
            if lower.starts_with("authorization:")
                && let Some(v) = line.split(':').nth(1)
            {
                auth = Some(v.trim().to_string());
            }
        }
        let mut body = req[headers_end..].to_vec();
        while body.len() < content_len {
            let n = stream.read(&mut buf).expect("fixture read body");
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n]);
        }
        let body = String::from_utf8_lossy(&body).to_string();
        if let Ok(mut slot) = captured_bg.lock() {
            *slot = Some(FixtureHttpRequest {
                method,
                path,
                authorization: auth,
                body,
            });
        }
        let response_bytes = response.as_bytes();
        let http = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_bytes.len(),
            response
        );
        stream
            .write_all(http.as_bytes())
            .expect("fixture write response");
        let _ = stream.flush();
    });
    (format!("http://{}/infer", addr), captured, handle)
}
