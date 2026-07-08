//! \file
//! \brief Webserver test plugin: serves files from its vFAT folder over HTTP on
//!        port 80. Exercises the generic inbound TCP listener (host_net_*) and
//!        opt-in background residency (host_set_resident); it is the acceptance
//!        gate for the network API and the emulator.

#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

mod http;

#[cfg(target_arch = "wasm32")]
mod plugin {
    use crate::http;
    use alloc::format;
    use cdc_badge_plugin::socket::TcpStream;
    use cdc_badge_plugin::{fs, i18n, lifecycle, log, net, nvs, plugin_main, ui, wifi};

    plugin_main!();

    const TAG: &str = "webserver";
    const DEFAULT_PORT: u16 = 80;
    const ACT_HTTP: u32 = 1;

    /// Listen port: NVS "port" (default 80). Configurable so the emulator/tests
    /// can use a free unprivileged port. Out-of-range values fall back to the
    /// default instead of being silently truncated.
    fn configured_port() -> u16 {
        match nvs::get_u32("port") {
            Some(v @ 1..=65535) => v as u16,
            Some(v) => {
                log::warn(TAG, &format!("ignoring invalid NVS port {}", v));
                DEFAULT_PORT
            }
            None => DEFAULT_PORT,
        }
    }
    const INDEX: &str = "index.html";
    const REQ_MAX: usize = 2048;
    /// Largest file served in one response. The buffer must fit into the
    /// plugin's 128 KiB linear memory next to stack and response head, and
    /// fs::read pre-allocates its max_len up front, so stay well below it.
    const FILE_MAX: usize = 64 * 1024;

    const DEFAULT_INDEX: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<title>CDC Badge</title></head><body style=\"font-family:sans-serif\">\
<h1>CDC Badge webserver</h1><p>Serving files from the plugin vFAT folder.</p>\
</body></html>\n";

    struct State {
        served: u32,
        /// Port the listener was actually bound to (0 = not listening). The
        /// NVS value can change afterwards, so display and close must not
        /// re-read it.
        bound_port: u16,
    }
    static mut STATE: State = State {
        served: 0,
        bound_port: 0,
    };
    #[inline]
    fn s() -> &'static mut State {
        unsafe { &mut *(&raw mut STATE) }
    }

    /// Deploy the bundled test page if the folder has no index yet.
    fn ensure_index() {
        if fs::size(INDEX).is_none() {
            let _ = fs::write_str(INDEX, DEFAULT_INDEX);
        }
    }

    fn start_server() {
        ensure_index();
        // WiFi is optional: request it best-effort so real clients can reach us;
        // the listener binds regardless (localhost in the emulator).
        let _ = wifi::request(0);
        let p = configured_port();
        match net::listen(p, ACT_HTTP) {
            Ok(()) => {
                s().bound_port = p;
                lifecycle::set_resident(true).ok();
                log::info(TAG, &format!("listening on port {}", p));
            }
            Err(e) => log::warn(TAG, &format!("listen failed: {:?}", e)),
        }
    }

    /// Read until the request line is complete (first CRLF) so a client that
    /// fragments the request over several TCP segments still parses.
    fn read_request_line(stream: &TcpStream, buf: &mut [u8]) -> usize {
        let mut got = 0;
        for _ in 0..4 {
            match stream.read(&mut buf[got..], 1000) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    got += n;
                    if buf[..got].windows(2).any(|w| w == b"\r\n") || got == buf.len() {
                        break;
                    }
                }
            }
        }
        got
    }

    /// Serve one accepted connection: read the request, map it to a file, reply.
    fn serve(stream: &TcpStream) {
        let mut buf = [0u8; REQ_MAX];
        let n = read_request_line(stream, &mut buf);
        if n == 0 {
            return;
        }
        let req = core::str::from_utf8(&buf[..n]).unwrap_or("");
        let path = match http::parse_request(req) {
            http::Request::Get(p) => p,
            http::Request::OtherMethod => {
                return send_error(stream, "405 Method Not Allowed", "Allow: GET\r\n");
            }
            http::Request::Bad => {
                return send_error(stream, "400 Bad Request", "");
            }
        };
        // Query the size first: fs::read pre-allocates max_len, so passing
        // FILE_MAX unconditionally would allocate the full cap per request.
        // Oversized files get a 404 like missing ones (test-server scope).
        let loaded = http::resolve_file(&path).and_then(|f| {
            let size = fs::size(&f).filter(|&sz| sz <= FILE_MAX)?;
            fs::read(&f, size).map(|d| (f, d))
        });
        match loaded {
            Some((name, body)) => {
                let head = format!(
                    "HTTP/1.0 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    http::content_type(&name),
                    body.len()
                );
                write_all(stream, head.as_bytes());
                write_all(stream, &body);
                s().served = s().served.wrapping_add(1);
            }
            None => send_error(stream, "404 Not Found", ""),
        }
    }

    /// Send a minimal error response with the status line as plain-text body.
    fn send_error(stream: &TcpStream, status: &str, extra_headers: &str) {
        let head = format!(
            "HTTP/1.0 {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
            status,
            status.len() + 1,
            extra_headers
        );
        write_all(stream, head.as_bytes());
        write_all(stream, status.as_bytes());
        write_all(stream, b"\n");
    }

    fn write_all(stream: &TcpStream, data: &[u8]) {
        let mut off = 0;
        while off < data.len() {
            match stream.write(&data[off..], 4000) {
                Ok(0) | Err(_) => break,
                Ok(k) => off += k,
            }
        }
    }

    #[no_mangle]
    pub extern "C" fn plugin_init() -> i32 {
        start_server();
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_deinit() -> i32 {
        // Port 0 closes this plugin's listener regardless of the bound port,
        // so a changed NVS value cannot leak the real listener.
        let _ = net::close(0);
        s().bound_port = 0;
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_enter() -> i32 {
        // Foreground: show the URL so the user sees where to reach the server.
        // Include the port unless it is the HTTP default (80), matching browser
        // URL conventions. Display the actually bound port, not the (possibly
        // changed since) NVS value.
        let p = match s().bound_port {
            0 => configured_port(),
            bound => bound,
        };
        let url = wifi::ip()
            .map(|ip| {
                if p == 80 {
                    format!("http://{}/", ip)
                } else {
                    format!("http://{}:{}/", ip, p)
                }
            })
            .unwrap_or_else(|| i18n::tr_key("no_ip").into());
        ui::push_info(
            i18n::tr_meta("name"),
            format!("{}\n{}: {}", url, i18n::tr_key("served"), s().served),
        );
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_exit() -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_tick(_uptime_ms: u64) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, _user_data: u32) -> i32 {
        if action_id == ACT_HTTP {
            // Drain all pending connections this tick.
            while let Some(stream) = net::accept() {
                serve(&stream);
            }
        }
        0
    }
}
