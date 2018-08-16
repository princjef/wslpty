const wslpty = require('../../');

var ptyProcess = wslpty.spawn({
    cols: 80,
    rows: 30,
    cwd: '~'
});

ptyProcess.on('data', function (data) {
    process.stdout.write(data);
});

ptyProcess.on('close', function () {
    console.log('pty closed');
});

ptyProcess.write('ls -a\r');

setTimeout(() => {
    ptyProcess.resize(40, 40);
    ptyProcess.write('ls -a\r');
}, 5000);

setTimeout(() => {
    ptyProcess.kill();
}, 10000);