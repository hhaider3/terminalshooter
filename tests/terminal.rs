#![cfg(unix)]
//! Exercises the real executable through a PTY. No desktop pointer or GUI use.
use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Pty {
    master: File,
    slave: File,
    child: Child,
    original: libc::termios,
    bytes: Vec<u8>,
    queried: bool,
    enhanced: bool,
}
impl Pty {
    fn launch(enhanced: bool) -> Self {
        let (mut master, mut slave) = (0, 0);
        let mut size = libc::winsize {
            ws_row: 30,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: valid output pointers and initialized winsize; returned fds
        // each transfer exactly once to File, which owns and closes them.
        unsafe {
            assert_eq!(
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut size
                ),
                0
            );
            libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC);
            libc::fcntl(slave, libc::F_SETFD, libc::FD_CLOEXEC);
            libc::fcntl(master, libc::F_SETFL, libc::O_NONBLOCK);
        }
        let (master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
        let mut original = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), &mut original) },
            0
        );
        let mut command = Command::new(env!("CARGO_BIN_EXE_terminalshooter"));
        command
            .args(["--seed", "7", "--256"])
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
        // SAFETY: only async-signal-safe libc calls run between fork and exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        Self {
            master,
            slave,
            child,
            original,
            bytes: Vec::new(),
            queried: false,
            enhanced,
        }
    }
    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).unwrap();
    }
    fn pump(&mut self, duration: Duration) {
        let until = Instant::now() + duration;
        let mut buffer = [0u8; 65536];
        while Instant::now() < until {
            match self.master.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => self.bytes.extend_from_slice(&buffer[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
                Err(e) => panic!("PTY read: {e}"),
            }
            if !self.queried && self.bytes.windows(4).any(|b| b == b"\x1b[?u") {
                self.queried = true;
                if self.enhanced {
                    self.send(b"\x1b[?0u\x1b[?1;2c");
                } else {
                    self.send(b"\x1b[?1;2c");
                }
            }
            std::thread::sleep(Duration::from_millis(3));
        }
    }
    fn wait_text(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            self.pump(Duration::from_millis(30));
            if self.screen().contains(text) {
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!(
                    "Game exited {status}: {}",
                    String::from_utf8_lossy(&self.bytes)
                );
            }
        }
        panic!("Missing {text:?} in screen:\n{}", self.screen());
    }
    fn screen(&self) -> String {
        let text = String::from_utf8_lossy(&self.bytes);
        let mut screen = vec![vec![' '; 220]; 70];
        let (mut x, mut y) = (0_usize, 0_usize);
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                let mut parameters = String::new();
                for part in chars.by_ref() {
                    if ('@'..='~').contains(&part) {
                        match part {
                            'H' => {
                                let values: Vec<usize> = parameters
                                    .split(';')
                                    .map(|n| n.parse().unwrap_or(1))
                                    .collect();
                                y = values[0].saturating_sub(1);
                                x = values.get(1).copied().unwrap_or(1).saturating_sub(1);
                            }
                            'J' if parameters == "2" => {
                                screen.iter_mut().for_each(|row| row.fill(' '))
                            }
                            _ => {}
                        }
                        break;
                    }
                    parameters.push(part);
                }
            } else if !c.is_control() {
                if y < screen.len() && x < screen[y].len() {
                    screen[y][x] = c;
                }
                x += 1;
            }
        }
        screen
            .iter()
            .map(|row| row.iter().collect::<String>() + "\n")
            .collect()
    }
    fn resize(&mut self, cols: u16, rows: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe { libc::ioctl(self.slave.as_raw_fd(), libc::TIOCSWINSZ as _, &size) },
            0
        );
    }
    fn assert_clean_exit(&mut self) {
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            self.pump(Duration::from_millis(20));
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < until, "Game did not quit");
        }
        let mut after: libc::termios = unsafe { std::mem::zeroed() };
        // On macOS the slave hangs up when its controlling session exits.
        // The master still exposes the same terminal's restored settings.
        assert_eq!(
            unsafe { libc::tcgetattr(self.master.as_raw_fd(), &mut after) },
            0
        );
        assert_eq!(after.c_iflag, self.original.c_iflag);
        assert_eq!(after.c_oflag, self.original.c_oflag);
        assert_eq!(
            after.c_lflag & !libc::PENDIN,
            self.original.c_lflag & !libc::PENDIN
        );
        assert!(self.bytes.windows(8).any(|b| b == b"\x1b[?1049l"));
    }
}
impl Drop for Pty {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn executable_controls_resize_and_restore_in_both_protocol_modes() {
    for enhanced in [false, true] {
        let mut tty = Pty::launch(enhanced);
        tty.wait_text("ENTER or CLICK");
        tty.send(b"\r");
        tty.wait_text("WAVE 1/5");
        // SGR down/up in a single write reproduces a click between frames.
        tty.send(b"\x1b[<0;50;15M\x1b[<0;50;15m");
        tty.wait_text("SHELLS 5/6");
        tty.pump(Duration::from_millis(450));
        assert!(tty.screen().contains("SHELLS 5/6"));
        tty.send(b"l");
        tty.wait_text("L mouse OFF");
        tty.pump(Duration::from_millis(300));
        tty.send(b"l");
        tty.wait_text("L mouse ON");
        tty.send(b"f");
        tty.wait_text("F fire ON");
        tty.send(b"p");
        tty.wait_text("PAUSED");
        assert!(!tty.screen().contains('▀'));
        tty.pump(Duration::from_millis(300));
        tty.send(b"p");
        tty.wait_text("WAVE 1/5");
        tty.send(b"M");
        tty.wait_text("TACTICAL MAP");
        tty.pump(Duration::from_millis(300));
        tty.send(b"m");
        tty.wait_text("WAVE 1/5");
        tty.resize(220, 70);
        tty.pump(Duration::from_millis(100));
        tty.send(b"l");
        tty.wait_text("L mouse OFF");
        tty.pump(Duration::from_millis(300));
        tty.send(b"l");
        tty.wait_text("L mouse ON");
        tty.resize(30, 10);
        tty.wait_text("Resize terminal");
        tty.resize(100, 30);
        tty.wait_text("WAVE 1/5");
        tty.resize(30, 10);
        tty.wait_text("Resize terminal");
        tty.send(b"\x1b[O");
        tty.pump(Duration::from_millis(100));
        tty.resize(100, 30);
        tty.wait_text("PAUSED");
        tty.send(b"\x1b[I");
        tty.pump(Duration::from_millis(100));
        assert!(tty.screen().contains("PAUSED"));
        tty.send(b"\x03");
        tty.assert_clean_exit();
    }
}

#[test]
fn held_mouse_fires_through_reload_and_stops_on_release() {
    let mut tty = Pty::launch(false);
    tty.wait_text("ENTER or CLICK");
    tty.send(b"\x1b[<0;50;15M");
    tty.wait_text("SHELLS 5/6");
    tty.wait_text("SHELLS 4/6");
    tty.wait_text("RELOAD");
    tty.wait_text("SHELLS 5/6 +42");
    tty.send(b"\x1b[<0;50;15m");
    tty.pump(Duration::from_millis(500));
    assert!(tty.screen().contains("SHELLS 5/6 +42"));
    tty.send(b"\x03");
    tty.assert_clean_exit();
}

#[test]
fn sigterm_restores_terminal() {
    let mut tty = Pty::launch(false);
    tty.wait_text("ENTER or CLICK");
    assert_eq!(
        unsafe { libc::kill(tty.child.id() as i32, libc::SIGTERM) },
        0
    );
    tty.assert_clean_exit();
}
