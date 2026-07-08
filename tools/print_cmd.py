#!/usr/bin/env python3
"""
CDC Badge print client - send text or images to the thermo_printer plugin's
print server, over the network (TCP) or directly over USB serial.

The badge does the rendering: text is laid out with the on-device fonts and
images are scaled+dithered to the 384 px print width. This client only frames
and ships the payload.

Wire (network / raw): the "PJ" print-job frame
    "PJ" | u8 type | u8 flags | u32 LE len | payload
    type 0 = text (UTF-8), 1 = image (PNG/JPEG), 2 = raster (pre-made TP body)

The server replies "OK" when the job was accepted and queued (NOT when it has
finished printing) and "ERR" when it was rejected.

Examples:
    echo "hello" | print_cmd.py --host badge.local
    print_cmd.py --host 10.0.0.5 --text "Ticket #42"
    print_cmd.py --host 10.0.0.5:9100 --image photo.jpg
    print_cmd.py --host "[fe80::1]:9100" --text "hello"
    print_cmd.py --serial --text "hello" --pin 0000
    print_cmd.py --serial /dev/ttyACM0 --image logo.png --pin 0000

Requires: pyserial (only for --serial). Pillow is optional (only used to
pre-scale images before sending; without it the badge scales instead).

Exit codes: 0 job accepted, 1 transport/badge error, 2 usage/input error.
"""

import argparse
import binascii
import glob
import socket
import struct
import sys
import time

MAGIC = b"PJ"
TYPE_TEXT = 0
TYPE_IMAGE = 1
TYPE_RASTER = 2
DEFAULT_TCP_PORT = 9100
PRINT_WIDTH = 384
UPLOAD_CHUNK = 256
SERIAL_BAUD = 115200


class ClientError(Exception):
    """User-facing failure; message printed to stderr, no traceback."""

    def __init__(self, message: str, exit_code: int = 1):
        super().__init__(message)
        self.exit_code = exit_code


def build_frame(kind: int, payload: bytes) -> bytes:
    """Frame a payload as a PJ print job."""
    return MAGIC + bytes([kind, 0]) + struct.pack("<I", len(payload)) + payload


def maybe_scale_image(data: bytes) -> bytes:
    """Pre-scale an image to the print width if Pillow is available; without
    Pillow the data passes through unchanged and the badge scales instead."""
    try:
        import io

        from PIL import Image
    except ImportError:
        return data
    try:
        img = Image.open(io.BytesIO(data))
        img.load()
    except Exception as e:
        raise ClientError(f"cannot decode image: {e}", 2)
    if img.width <= PRINT_WIDTH and img.mode not in ("RGBA", "LA", "P"):
        return data
    if img.mode in ("RGBA", "LA", "P"):
        # Composite transparency onto white: a plain RGB conversion would
        # turn transparent areas black after dithering.
        img = img.convert("RGBA")
        from PIL import Image as PILImage

        bg = PILImage.new("RGBA", img.size, (255, 255, 255, 255))
        img = PILImage.alpha_composite(bg, img)
    img = img.convert("RGB")
    if img.width > PRINT_WIDTH:
        h = max(1, round(img.height * PRINT_WIDTH / img.width))
        img = img.resize((PRINT_WIDTH, h))
    out = io.BytesIO()
    img.save(out, format="PNG")
    return out.getvalue()


# --- payload selection ------------------------------------------------------

def read_payload(args):
    """Return (kind, payload_bytes) from the chosen input."""
    if args.image:
        try:
            with open(args.image, "rb") as f:
                return TYPE_IMAGE, maybe_scale_image(f.read())
        except OSError as e:
            raise ClientError(f"cannot read image {args.image}: {e}", 2)
    if args.raster:
        try:
            with open(args.raster, "rb") as f:
                return TYPE_RASTER, f.read()
        except OSError as e:
            raise ClientError(f"cannot read raster {args.raster}: {e}", 2)
    if args.text is not None:
        return TYPE_TEXT, args.text.encode("utf-8")
    if sys.stdin.isatty():
        print("reading text from stdin, finish with Ctrl-D ...", file=sys.stderr)
    data = sys.stdin.buffer.read()
    if not data.strip():
        raise ClientError("empty input: pass --text/--image or pipe data in", 2)
    return TYPE_TEXT, data


# --- network transport ------------------------------------------------------

def parse_host(host: str):
    """Split host[:port], supporting bracketed IPv6 literals ([::1]:9100)."""
    if host.startswith("["):
        addr, sep, rest = host[1:].partition("]")
        if not sep:
            raise ClientError(f"invalid IPv6 host {host!r} (missing ])", 2)
        if rest.startswith(":"):
            rest = rest[1:]
        port_str = rest
    elif host.count(":") == 1:
        addr, port_str = host.split(":", 1)
    else:
        # No colon, or a bare IPv6 literal with several colons: no port part.
        addr, port_str = host, ""
    if not port_str:
        return addr, DEFAULT_TCP_PORT
    try:
        port = int(port_str)
        if not 1 <= port <= 65535:
            raise ValueError
    except ValueError:
        raise ClientError(f"invalid port {port_str!r} in {host!r}", 2)
    return addr, port


def send_tcp(host: str, kind: int, payload: bytes) -> int:
    addr, port = parse_host(host)
    frame = build_frame(kind, payload)
    try:
        with socket.create_connection((addr, port), timeout=10) as s:
            s.sendall(frame)
            s.settimeout(10)
            try:
                resp = s.recv(16).strip()
            except socket.timeout:
                resp = b""
    except OSError as e:
        raise ClientError(f"cannot reach {addr}:{port}: {e}")
    print(f"sent {len(frame)} bytes to {addr}:{port} -> "
          f"{resp.decode(errors='replace') or '(no reply)'}")
    if resp.startswith(b"OK"):
        return 0
    if not resp:
        # No reply within the timeout is a failure: the server always answers
        # OK/ERR once it accepted or rejected the job.
        print("error: no reply from the print server", file=sys.stderr)
    return 1


# --- serial transport -------------------------------------------------------

def detect_serial():
    """Prefer pyserial's port enumeration (works on Windows, where COM ports
    are not filesystem globs); fall back to device-path globs."""
    try:
        from serial.tools import list_ports

        for p in list_ports.comports():
            dev = p.device
            if any(k in dev for k in ("ACM", "USB", "usbmodem", "COM")):
                return dev
    except ImportError:
        pass
    for pat in ("/dev/ttyACM*", "/dev/ttyUSB*", "/dev/cu.usbmodem*"):
        hits = glob.glob(pat)
        if hits:
            return hits[0]
    return None


def wait_response(s, deadline_s: float) -> str:
    """Collect serial output until an OK/ERR line shows up or time runs out."""
    got = ""
    end = time.monotonic() + deadline_s
    while time.monotonic() < end:
        chunk = s.read_all().decode(errors="replace")
        if chunk:
            got += chunk
            if "OK" in got or "ERR" in got:
                break
        time.sleep(0.1)
    return got


def send_serial(port, pin, kind, payload) -> int:
    try:
        import serial  # pyserial
    except ImportError:
        raise ClientError("pyserial is not installed (pip install pyserial)", 2)

    port = port or detect_serial()
    if not port:
        raise ClientError("no serial port found (pass --serial <device>)", 2)
    try:
        s = serial.Serial(port, SERIAL_BAUD, timeout=2)
    except serial.SerialException as e:
        raise ClientError(f"cannot open {port}: {e}")
    try:
        time.sleep(0.3)
        s.reset_input_buffer()

        def line(cmd):
            s.write(cmd.encode() + b"\r\n")
            time.sleep(0.4)
            return s.read_all().decode(errors="replace")

        if pin:
            resp = line(f"AUTH {pin}")
            if "ERR" in resp:
                raise ClientError(f"AUTH rejected: {resp.strip()}")

        if kind == TYPE_TEXT:
            # Text rides the line-based command channel directly. Newlines are
            # flattened to spaces here (the network path preserves them).
            text = (payload.decode("utf-8", errors="replace")
                    .replace("\r", " ").replace("\n", " ").strip())
            if not text:
                raise ClientError("empty text after flattening newlines", 2)
            resp = line(f"PLUGIN CMD thermo_printer text {text}")
            print(resp.strip())
            return 1 if "ERR" in resp else 0

        # Image: stage into the plugin's vFAT folder, then print it.
        name = "netjob.png"
        s.reset_input_buffer()
        crc = binascii.crc32(payload) & 0xFFFFFFFF
        s.write(f"VFAT RECEIVE {name} {len(payload)} {crc:08x}\r\n".encode())
        time.sleep(0.5)
        ready = s.read_all().decode(errors="replace")
        if "READY" not in ready:
            raise ClientError(f"badge did not accept upload: {ready.strip()}")
        for i in range(0, len(payload), UPLOAD_CHUNK):
            s.write(payload[i:i + UPLOAD_CHUNK])
            s.flush()
        # The badge answers "OK <total_bytes>" after verifying the stream CRC
        # (or "ERR ..."); FAT writes on large images take a while.
        done = wait_response(s, 3.0 + len(payload) / 8192.0)
        if "OK" not in done:
            raise ClientError(f"upload failed: {done.strip() or '(no response)'}")
        resp = line(f"PLUGIN CMD thermo_printer file {name}")
        print(resp.strip())
        return 1 if "ERR" in resp else 0
    finally:
        s.close()


def main():
    ap = argparse.ArgumentParser(description="Print to the CDC Badge thermo_printer server.")
    tr = ap.add_mutually_exclusive_group(required=True)
    tr.add_argument("--host", help="badge IP[:port] for the network print server (default port 9100)")
    tr.add_argument("--serial", nargs="?", const="", metavar="PORT",
                    help="print over USB serial (optional device path; auto-detected if omitted)")
    src = ap.add_mutually_exclusive_group()
    src.add_argument("--text", help="text to print (else read from stdin)")
    src.add_argument("--image", help="image file (PNG/JPEG) to print, always scaled")
    src.add_argument("--raster", help="pre-made 1-bpp TP raster body (network only)")
    ap.add_argument("--pin", default="", help="serial AUTH pin (serial transport)")
    args = ap.parse_args()

    if args.raster and args.host is None:
        # The plugin's file command renders text/images only; a staged .tp
        # raster cannot be printed over the serial path.
        ap.error("--raster works over the network path only (use --host)")

    try:
        kind, payload = read_payload(args)
        if args.host:
            return send_tcp(args.host, kind, payload)
        return send_serial(args.serial, args.pin, kind, payload)
    except ClientError as e:
        print(f"error: {e}", file=sys.stderr)
        return e.exit_code
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    sys.exit(main())
