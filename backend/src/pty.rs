use std::env;
use std::ffi::CString;
use std::fs::File;
use std::io;
use std::io::Read;
use std::os::raw::{c_char, c_int};
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};
use std::ptr;

use libc;

pub fn fork(
    cols: u16,
    rows: u16,
    cwd: Option<&str>,
    shell: Option<&str>,
) -> Result<(libc::pid_t, c_int), io::Error> {
    let wsize = libc::winsize {
        ws_col: cols,
        ws_row: rows,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let mut term = libc::termios {
        c_iflag: libc::ICRNL
            | libc::IXON
            | libc::IXANY
            | libc::IMAXBEL
            | libc::BRKINT
            | libc::IUTF8,
        c_oflag: libc::OPOST | libc::ONLCR,
        c_cflag: libc::CREAD | libc::CS8 | libc::HUPCL,
        c_lflag: libc::ICANON
            | libc::ISIG
            | libc::IEXTEN
            | libc::ECHO
            | libc::ECHOE
            | libc::ECHOK
            | libc::ECHOKE
            | libc::ECHOCTL,
        c_line: 0,
        c_cc: [0; 32],
        c_ispeed: libc::B38400,
        c_ospeed: libc::B38400,
    };

    unsafe {
        libc::cfsetispeed(&mut term, libc::B38400);
        libc::cfsetospeed(&mut term, libc::B38400);
    }

    term.c_cc[libc::VEOF] = 4;
    term.c_cc[libc::VEOL] = 0xff; // TODO: what if I want to put -1 in here? wrap or nowrap?
    term.c_cc[libc::VEOL2] = 0xff; // TODO: same as previous line
    term.c_cc[libc::VERASE] = 0x7f;
    term.c_cc[libc::VWERASE] = 23;
    term.c_cc[libc::VKILL] = 21;
    term.c_cc[libc::VREPRINT] = 18;
    term.c_cc[libc::VINTR] = 3;
    term.c_cc[libc::VQUIT] = 0x1c;
    term.c_cc[libc::VSUSP] = 26;
    term.c_cc[libc::VSTART] = 17;
    term.c_cc[libc::VSTOP] = 19;
    term.c_cc[libc::VLNEXT] = 22;
    term.c_cc[libc::VDISCARD] = 15;
    term.c_cc[libc::VMIN] = 1;
    term.c_cc[libc::VTIME] = 0;

    let mut master = -1;
    let pid = unsafe { forkpty(&mut master, &mut 0, &term, &wsize) };
    match pid {
        -1 => Err(io::Error::last_os_error()),
        0 => {
            // This is where we actually start the process in the pty. We need
            // to:
            //
            //   * Change to the specified directory
            //   * Apply the given environment variables
            //   * Run the shell

            if let Some(cwd) = cwd {
                env::set_current_dir(Path::new(cwd))?;
            }

            // TODO: investigate using execvpe
            let shell = if let Some(shell) = shell {
                shell.to_owned()
            } else if let Some(shell) = env::var("SHELL").ok() {
                shell
            } else {
                "/bin/sh".to_owned()
            };
            let c_shell = CString::new(shell).expect("Couldn't convert shell name to c string");
            let c_argv: Vec<_> = env::args()
                .skip(1)
                .take(1)
                .map(|arg| CString::new(arg).unwrap())
                .collect();
            let mut p_argv: Vec<_> = c_argv.iter().map(|arg| arg.as_ptr()).collect();
            p_argv.push(ptr::null());
            unsafe {
                libc::execvp(c_shell.as_ptr(), p_argv.as_ptr());
            }
            Err(io::Error::last_os_error())
        }
        pid => {
            Ok((pid, master))
        }
    }
}

pub fn resize(fd: RawFd, cols: u16, rows: u16) -> Result<(), io::Error> {
    let wsize = libc::winsize {
        ws_col: cols,
        ws_row: rows,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe {
        match libc::ioctl(fd, libc::TIOCSWINSZ, &wsize) {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }
}

pub fn procname(fd: RawFd) -> Result<String, io::Error> {
    unsafe {
        match libc::tcgetpgrp(fd) {
            -1 => Err(io::Error::last_os_error()),
            pid => {
                let path = format!("/proc/{}/cmdline", pid);
                let mut path_buf = PathBuf::new();
                path_buf.push(path);
                let mut file = File::open(path_buf)?;
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)?;
                if let Some(pos) = buf.iter().position(|&r| r == 0) {
                    buf.truncate(pos)
                }

                String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            }
        }
    }
}

#[link(name = "util")]
extern "C" {
    pub fn forkpty(
        amaster: *mut c_int,
        name: *mut c_char,
        termp: *const libc::termios,
        winp: *const libc::winsize,
    ) -> libc::pid_t;
}
