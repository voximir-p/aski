"""Unix terminal smoke tests; requires a built binary and ffmpeg on PATH.

Run: python3 tests/terminal_smoke.py target/debug/aski
"""
import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time


def resize(fd, columns, rows):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))


def read_until(fd, process, marker, timeout=5):
    data = bytearray()
    deadline = time.monotonic() + timeout
    while marker not in data:
        if time.monotonic() > deadline:
            raise AssertionError(f"Timed out waiting for {marker!r}; exit={process.poll()}")
        if select.select([fd], [], [], 0.05)[0]:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            data.extend(chunk)
    assert marker in data, f"Missing {marker!r}: {bytes(data[-500:])!r}"
    return data


def check(binary, media, args, expected):
    master, slave = pty.openpty()
    resize(slave, 80, 24)
    process = subprocess.Popen(
        [binary, str(media), *args, "--verbose"],
        stdin=subprocess.DEVNULL, stdout=slave, stderr=slave,
    )
    os.close(slave)
    try:
        output = read_until(master, process, expected)
        assert b"\x1b[?1049h" in output, "Did not enter alternate screen"
        resize(master, 32, 12)
        output.extend(read_until(master, process, b"Ctrl+C"))
        start = time.monotonic()
        process.send_signal(signal.SIGINT)
        output.extend(read_until(master, process, b"Playback finished:"))
        assert process.wait(timeout=2) == 0
        assert time.monotonic() - start < 1, "Slow cancellation during frame wait"
        summary = output.index(b"Playback finished:")
        for restore in [b"\x1b[?7h", b"\x1b[?25h", b"\x1b[?1049l"]:
            assert 0 <= output.index(restore) < summary, "Terminal restored after summary"
        print(f"PASS: {' '.join(args)}; resize and Ctrl+C restore terminal")
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
        os.close(master)


def main():
    binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/aski").resolve())
    with tempfile.TemporaryDirectory(prefix="aski-terminal-") as directory:
        media = Path(directory) / "sample.mkv"
        subprocess.run([
            "ffmpeg", "-v", "error", "-f", "lavfi", "-i",
            "testsrc2=size=96x48:rate=20:duration=0.5", "-c:v", "ffv1", str(media),
        ], check=True)
        check(binary, media, ["--loop", "--precompute"], b"CACHE")
        check(binary, media, ["--loop", "--no-cache", "--prefetch", "0"], b"PLAY")
        check(binary, media, ["--loop", "--fps-limit", "1"], b"PLAY")
        broken = Path(directory) / "broken.gif"
        broken.write_bytes(b"not a GIF")
        master, slave = pty.openpty()
        process = subprocess.Popen([binary, str(broken)], stdout=slave, stderr=slave)
        os.close(slave)
        try:
            output = read_until(master, process, b"aski:")
            assert process.wait(timeout=2) != 0
            assert output.index(b"\x1b[?1049l") < output.index(b"aski:")
            print("PASS: decoder error restores terminal before reporting failure")
        finally:
            if process.poll() is None:
                process.kill()
            process.wait()
            os.close(master)


if __name__ == "__main__":
    main()
