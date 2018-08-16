# wslpty

Spawn native pseudoterminals for the Windows Subsystem for Linux.

Notable features:

 * Full support for standard and custom escape sequences
 * Get name of current foreground terminal process
 * Customizable shell, working directory, environment and more

## Usage

Install wslpty by running:

```
npm install wslpty
```

The following example sets up a terminal piped through stdout, then closes it
after 5 seconds:

```ts
import * as wslpty from 'wslpty';

var ptyProcess = wslpty.spawn({
    cols: 80,
    rows: 30,
    cwd: '~'
});

ptyProcess.on('data', function (data) {
    process.stdout.write(data);
});

ptyProcess.write('ls -a\r');
ptyProcess.resize(40, 40);
ptyProcess.write('ls -a\r');

setTimeout(() => {
    ptyProcess.kill();
}, 5000);
```

## Organization

Wslpty is built in two parts that communicate to each other over a TCP socket:

### Frontend

A client written in Node.js that exposes the user-facing API and spawns the
backend portion of the code. It spawns a TCP server on a randomly chosen port
and starts the backend process. It is located in the [frontend/](./frontend)
folder and runs on Windows at runtime.

### Backend

A TCP client written in [Rust][] that forks the pty and provides a translation
layer between the socket communication and the pty. It is located in the
[backend/](./backend) folder and runs in WSL at runtime.

The backend is written in Rust because Rust can interop cleanly into C (where
the forkpty capabilities live) and it compiles to a standalone binary. This is
important because the backend runs within WSL itself. Even if a user is running
Node.js from Windows (a must when using this package), they do not necessarily
have Node.js installed within WSL. Providing a standalone binary avoids this
problem.

## Development

It is highly recommended that you develop wslpty within WSL itself. To develop,
you will need the following:

 * Windows installation with WSL enabled
 * [Node.js][] - 8.x or above
 * [Rust][Rust install] - stable toolchain



## Motivation

There are some wonderful tools out there that make working with pseudoterminals
on Windows possible outside of the standard console host. They heavily inspired
the design and implementation of this package, but come with some limitations:

 * [winpty][] - makes it possible to create a pty in windows using an embedded
   conhost
    * Unable to track process names in the terminal
    * Conhost swallows or transforms some escape sequences, making TrueColor and
      some terminal integrations difficult or impossible
 * [wslbridge][] - creates a pty in WSL and passes data out on a TCP socket to
   avoid the limitations of winpty
    * Designed for use specifically in Cygwin - not Node.js compatible
    * Associated [wsltty][] terminal application lacks some features (such as
      tabs) that many people desire in terminals
 * [node-pty][] - creates a pty in Linux/MacOS/Windows in Node.js
    * Uses winpty to create terminals in Windows, with all of its limitations

This package combines the Node.js interface of [node-pty][] (and some of its
backend pseudoterminal code) with the socket-based communication pattern of
[wslbridge][] to make creation of a fully functional WSL pty effortless in
Node.js.

[Rust]: https://www.rust-lang.org/en-US/
[Node.js]: https://nodejs.org/en/
[Rust install]: https://www.rust-lang.org/en-US/install.html
[winpty]: https://github.com/rprichard/winpty
[wslbridge]: https://github.com/rprichard/wslbridge
[wsltty]: https://github.com/mintty/wsltty
[node-pty]: https://github.com/Microsoft/node-pty
