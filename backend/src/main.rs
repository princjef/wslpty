extern crate byteorder;
extern crate bytes;
extern crate clap;
extern crate libc;

mod frame;
mod pty;

use std::fs::File;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::os::unix::io::{FromRawFd, RawFd};
use std::thread;
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};

use clap::{App, Arg};

use frame::{encode, Frame, FrameDecoder};

const INITIAL_CAPACITY: usize = 8 * 1024;

fn validator_u16(field_name: &'static str) -> impl Fn(String) -> Result<(), String> {
    move |val| match val.parse::<u16>() {
        Ok(_) => Ok(()),
        Err(_) => Err(format!(
            "{} must be a number between 0 and 65536",
            field_name
        )),
    }
}

fn main() {
    let args = App::new("wslpty")
        .version("0.1.0")
        .author("Jeff Principe <princjef@gmail.com>")
        .about("Backend terminal process for wslpty")
        .arg(
            Arg::with_name("port")
                .help("Port to use for the TCP connection with the frontend")
                .required(true)
                .index(1)
                .validator(validator_u16("port")),
        ).arg(
            Arg::with_name("cols")
                .long("cols")
                .help("Number of columns in the spawned terminal")
                .takes_value(true)
                .validator(validator_u16("cols")),
        ).arg(
            Arg::with_name("rows")
                .long("rows")
                .help("Number of rows in the spawned terminal")
                .takes_value(true)
                .validator(validator_u16("rows")),
        ).arg(
            Arg::with_name("cwd")
                .long("cwd")
                .help("Working directory in which to start the terminal")
                .takes_value(true),
        ).arg(
            Arg::with_name("shell")
                .long("shell")
                .help("Shell name/file to run. Falls back to the SHELL env variable")
                .takes_value(true),
        ).get_matches();
    let port = args.value_of("port").unwrap().parse::<u16>().unwrap();

    let cols = match args.value_of("cols") {
        Some(arg) => arg.parse::<u16>().unwrap(),
        _ => 80,
    };
    let rows = match args.value_of("rows") {
        Some(arg) => arg.parse::<u16>().unwrap(),
        _ => 40,
    };

    let cwd = args.value_of("cwd");
    let shell = args.value_of("shell");

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let mut stream = TcpStream::connect(&addr).expect("Socket failure");
    stream.set_nodelay(true).expect("Couldn't set nodelay");

    let mut write_data_stream = stream.try_clone().expect("Failed to clone stream");
    let mut write_procname_stream = stream.try_clone().expect("Failed to clone stream");
    let mut write_cwd_stream = stream.try_clone().expect("Failed to clone stream");

    let (_, term_fd) = pty::fork(cols, rows, cwd, shell).expect("Couldn't spawn pty");
    let mut pty_reader: File = unsafe { File::from_raw_fd(term_fd) };
    let mut pty_writer = pty_reader.try_clone().expect("Failed to clone pty file");

    let pty_to_tcp_thread = thread::spawn(move || {
        let mut buf = BytesMut::with_capacity(INITIAL_CAPACITY);
        loop {
            let read_result = unsafe {
                let res = {
                    let b = buf.bytes_mut();
                    for i in 0..b.len() {
                        b[i] = 0;
                    }
                    pty_reader.read(b)
                };

                if let Ok(n) = res {
                    buf.advance_mut(n);
                }

                res
            };

            let res = match read_result {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    let bytes = buf.split_to(n).freeze();
                    let mut frame_buf = BytesMut::with_capacity(INITIAL_CAPACITY);
                    let res = match encode(Frame::Data(bytes), &mut frame_buf) {
                        Ok(()) => write_data_stream.write_all(&mut frame_buf),
                        Err(e) => Err(e),
                    };
                    buf.reserve(1);
                    res
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => Ok(()),
                Err(e) => Err(e),
            };

            if let Err(e) = res {
                println!("{:?}", e);
                panic!(e);
            }
        }
    });

    let procname_thread = thread::spawn(move || {
        let mut current_name = String::from("");
        loop {
            match get_procname(&mut write_procname_stream, term_fd, &current_name) {
                Ok(name) => {
                    current_name = name;
                }
                Err(e) => {
                    println!("{:?}", e);
                }
            };

            thread::sleep(Duration::from_millis(200));
        }
    });

    let cwd_thread = thread::spawn(move || {
        let mut current_cwd = String::from("");
        loop {
            match get_cwd(&mut write_cwd_stream, term_fd, &current_cwd) {
                Ok(cwd) => {
                    current_cwd = cwd;
                }
                Err(e) => {
                    println!("{:?}", e);
                }
            };

            thread::sleep(Duration::from_millis(200));
        }
    });

    let tcp_to_pty_thread = thread::spawn(move || {
        let mut decoder = FrameDecoder::new();
        let mut data = BytesMut::with_capacity(INITIAL_CAPACITY);

        loop {
            let read_result = unsafe {
                let res = {
                    let b = data.bytes_mut();
                    for i in 0..b.len() {
                        b[i] = 0;
                    }
                    stream.read(b)
                };

                if let Ok(n) = res {
                    data.advance_mut(n);
                }

                res
            };

            let res = match read_result {
                Ok(0) => {
                    break;
                }
                Ok(_) => {
                    let mut try_decode = true;
                    let mut res: Result<(), io::Error> = Ok(());
                    while try_decode {
                        try_decode = false;
                        let decode_res = match decoder.decode(&mut data) {
                            Ok(Some(frame)) => {
                                try_decode = true;
                                match frame {
                                    Frame::Data(bytes) => {
                                        pty_writer.write_all(&bytes)
                                    }
                                    Frame::Size(cols, rows) => {
                                        pty::resize(term_fd, cols, rows)
                                    }
                                    _ => Ok(()),
                                }
                            }
                            Ok(None) => Ok(()),
                            Err(e) => Err(e),
                        };

                        if let Err(e) = decode_res {
                            res = Err(e);
                            break;
                        }
                    }
                    data.reserve(1);
                    res
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => Ok(()),
                Err(e) => Err(e),
            };

            if let Err(e) = res {
                println!("{:?}", e);
                panic!(e)
            }
        }
    });

    let finish = move || -> thread::Result<()> {
        pty_to_tcp_thread.join()?;
        tcp_to_pty_thread.join()?;
        procname_thread.join()?;
        cwd_thread.join()?;
        Ok(())
    };

    match finish() {
        Ok(_) => {
            println!("process exited without error");
        }
        Err(e) => {
            println!("process panicked with error: {:?}", e);
        }
    }
}

fn get_procname(
    mut stream: &TcpStream,
    fd: RawFd,
    current_name: &str,
) -> Result<String, io::Error> {
    let name = pty::procname(fd)?;
    if &name != current_name {
        let mut buf = BytesMut::with_capacity(INITIAL_CAPACITY);
        encode(Frame::Name(Bytes::from(name.clone())), &mut buf)?;
        stream.write_all(&mut buf)?;
    }
    Ok(name)
}

fn get_cwd(
    mut stream: &TcpStream,
    fd: RawFd,
    current_cwd: &str,
) -> Result<String, io::Error> {
    let cwd = pty::cwd(fd)?;
    if &cwd != current_cwd {
        let mut buf = BytesMut::with_capacity(INITIAL_CAPACITY);
        encode(Frame::Cwd(Bytes::from(cwd.clone())), &mut buf)?;
        stream.write_all(&mut buf)?;
    }
    Ok(cwd)
}