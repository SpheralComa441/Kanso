//! The way back from a sign-in done in the user's own browser.
//!
//! Microsoft sends the browser back to `http://localhost:<port>` when the user has signed in
//! (see [`kanso_core::xbox_account`]). For that, and only while a sign-in is under way, Kanso
//! listens on that port **on this PC alone**: the listener is bound to the loopback addresses,
//! which no other machine can reach. It answers one thing, the request that carries the mark
//! of the sign-in in progress, with a page that says the sign-in is over; anything else gets
//! "not found". It serves no file and runs nothing, and it is closed the moment the answer
//! arrives, the user gives up or the time is up.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long the user has to sign in.
const SIGN_IN_TIME: Duration = Duration::from_secs(10 * 60);
/// A request line with a code in it is a few hundred bytes: more than this is not one.
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const POLL_EVERY: Duration = Duration::from_millis(120);
/// How long the listener stays after the answer, for a browser that asks again.
const LINGER: Duration = Duration::from_millis(1500);

/// How waiting for the browser ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Wait<T> {
    /// The answer to this sign-in arrived.
    Answered(T),
    Cancelled,
    TimedOut,
}

/// What a request means: `Some((answer, page))` ends the wait and shows `page` in the
/// browser; `None` is a request that is not the answer.
pub(crate) type Reader<'a, T> = &'a dyn Fn(&str) -> Option<(T, String)>;

/// The listener for one sign-in. Binding it before the browser is opened is what tells a
/// port that is taken from a sign-in nobody completed.
pub(crate) struct Listener {
    sockets: Vec<TcpListener>,
}

impl Listener {
    /// Listens on `port`, on this PC only. `None`: another program holds the port.
    pub(crate) fn on(port: u16) -> Option<Self> {
        // `localhost` is the one or the other, as the browser pleases
        let v4 = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).ok()?;
        let v6 = TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port))).ok();
        let sockets: Vec<TcpListener> = std::iter::once(v4).chain(v6).collect();
        for socket in &sockets {
            socket.set_nonblocking(true).ok()?;
        }
        Some(Self { sockets })
    }

    /// The port actually listened on (tests ask for any free one).
    #[cfg(test)]
    fn port(&self) -> u16 {
        self.sockets[0].local_addr().map(|address| address.port()).unwrap_or_default()
    }

    /// Waits for the request `read` recognises as the answer, until `cancel` is set or the
    /// time is up. The listener is closed when this returns.
    pub(crate) fn wait<T>(self, read: Reader<'_, T>, cancel: &AtomicBool, time: Duration) -> Wait<T> {
        let deadline = Instant::now() + time;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Wait::Cancelled;
            }
            if Instant::now() >= deadline {
                return Wait::TimedOut;
            }
            for socket in &self.sockets {
                if let Ok((stream, _)) = socket.accept() {
                    if let Some(answer) = serve(stream, read) {
                        self.linger(read);
                        return Wait::Answered(answer);
                    }
                }
            }
            std::thread::sleep(POLL_EVERY);
        }
    }

    /// Stays a moment longer after the answer: a browser that did not get the page the first
    /// time (it opens more than one connection, and may ask twice) asks again, and would
    /// otherwise find nobody and show an error over a sign-in that went through.
    fn linger<T>(&self, read: Reader<'_, T>) {
        let until = Instant::now() + LINGER;
        while Instant::now() < until {
            for socket in &self.sockets {
                if let Ok((stream, _)) = socket.accept() {
                    let _ = serve(stream, read);
                }
            }
            std::thread::sleep(POLL_EVERY);
        }
    }

    pub(crate) fn wait_for_sign_in<T>(self, read: Reader<'_, T>, cancel: &AtomicBool) -> Wait<T> {
        self.wait(read, cancel, SIGN_IN_TIME)
    }
}

/// Reads one request and answers it: the page of `read` for the answer to the sign-in,
/// "not found" for anything else.
fn serve<T>(mut stream: TcpStream, read: Reader<'_, T>) -> Option<T> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1500)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));

    // The whole request is read, headers included, before answering: a connection closed
    // with part of the request still unread is reset rather than closed, and the browser
    // then throws the answer away
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    while !request.windows(4).any(|end| end == b"\r\n\r\n") && request.len() < MAX_REQUEST_BYTES {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
        }
    }
    // A connection the browser opened ahead of time and sent nothing on
    if request.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&request);
    let first_line = text.lines().next().unwrap_or_default();

    let (answer, status, page) = match read(first_line) {
        Some((answer, page)) => (Some(answer), "200 OK", page),
        None => (None, "404 Not Found", String::new()),
    };
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Kanso</title></head><body style=\"font-family:system-ui,sans-serif;background:#0b0b0d;color:#e4e4e7;display:grid;place-items:center;height:100vh;margin:0\"><p>{}</p></body></html>",
        escape(&page)
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
    // Closed in order: this side says it is done, then waits for the browser to close its
    // own, so that the answer is delivered before the connection goes
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    while matches!(stream.read(&mut buffer), Ok(read) if read > 0) {}
    answer
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn get(port: u16, target: &str) -> String {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream.write_all(format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes()).unwrap();
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        answer
    }

    fn reader(line: &str) -> Option<(String, String)> {
        let code = line.strip_prefix("GET /xbox?code=")?.split(' ').next()?;
        Some((code.to_string(), "Fatto <ok> & chiuso".to_string()))
    }

    #[test]
    fn only_the_answer_ends_the_wait_and_everything_else_is_not_found() {
        let listener = Listener::on(0).expect("a free port");
        let port = listener.port();
        let cancel = Arc::new(AtomicBool::new(false));
        let waiting = {
            let cancel = Arc::clone(&cancel);
            std::thread::spawn(move || listener.wait(&reader, &cancel, Duration::from_secs(20)))
        };

        let stray = get(port, "/favicon.ico");
        assert!(stray.starts_with("HTTP/1.1 404 Not Found"), "{stray}");
        let answer = get(port, "/xbox?code=abc123");
        assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
        assert!(answer.contains("Fatto &lt;ok&gt; &amp; chiuso"), "the page is text, never markup");
        assert!(answer.contains("Cache-Control: no-store"));

        // A browser that asks again right after still finds the page
        let again = get(port, "/xbox?code=abc123");
        assert!(again.starts_with("HTTP/1.1 200 OK"), "{again}");

        assert_eq!(waiting.join().unwrap(), Wait::Answered("abc123".to_string()));
        // The listener is gone with the wait
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
    }

    #[test]
    fn the_answer_reaches_a_browser_that_sends_long_headers_and_opens_spare_connections() {
        let listener = Listener::on(0).expect("a free port");
        let port = listener.port();
        let cancel = Arc::new(AtomicBool::new(false));
        let waiting = {
            let cancel = Arc::clone(&cancel);
            std::thread::spawn(move || listener.wait(&reader, &cancel, Duration::from_secs(20)))
        };

        // A connection opened ahead of time, on which nothing is ever sent
        let spare = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();

        // The request, with the headers a browser sends, arriving in two pieces
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream.write_all(b"GET /xbox?code=abc123 HTTP/1.1\r\nHost: localhost\r\n").unwrap();
        stream.flush().unwrap();
        std::thread::sleep(Duration::from_millis(150));
        let headers = format!("User-Agent: {}\r\nAccept: text/html\r\nCookie: {}\r\n\r\n", "Mozilla/5.0 ".repeat(20), "a=b; ".repeat(300));
        stream.write_all(headers.as_bytes()).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).expect("the connection is closed, not reset");
        assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
        assert!(answer.ends_with("</html>"), "the whole page arrived");

        assert_eq!(waiting.join().unwrap(), Wait::Answered("abc123".to_string()));
        drop(spare);
    }

    #[test]
    fn giving_up_or_running_out_of_time_ends_the_wait_too() {
        let cancel = AtomicBool::new(false);
        let listener = Listener::on(0).unwrap();
        assert_eq!(listener.wait(&reader, &cancel, Duration::from_millis(300)), Wait::TimedOut);

        let listener = Listener::on(0).unwrap();
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(listener.wait(&reader, &cancel, Duration::from_secs(20)), Wait::Cancelled);
    }

    #[test]
    fn a_port_somebody_else_holds_is_told_at_once() {
        let first = Listener::on(0).unwrap();
        assert!(Listener::on(first.port()).is_none());
    }
}
