import * as net from 'net';
import * as path from 'path';
import * as child_process from 'child_process';
import { EventEmitter } from 'events';
import * as getPort from 'get-port';

import * as frame from './frame';

/**
 * Configuration options for the pseudoterminal.
 */
export type Options = Partial<ResolvedOptions>;

export interface ResolvedOptions {
    /**
     * Number of columns in the terminal. Defaults to 80.
     */
    cols: number;

    /**
     * Number of rows in the terminal. Defaults to 30.
     */
    rows: number;

    /**
     * Directory where the terminal should start.
     */
    cwd?: string;

    /**
     * Startup shell to use in the terminal. Defaults to the SHELL environment
     * variable in the terminal process.
     */
    shell?: string;

    /**
     * The set of environment variables to set in the terminal process. Defaults
     * to the current process's environment.
     */
    env: { [key: string]: string; };
}

/**
 * Pseudoterminal control interface
 */
export interface IPty extends EventEmitter {
    /**
     * Number of columns in the terminal.
     */
    cols: number;

    /**
     * Number of rows in the terminal.
     */
    rows: number;

    /**
     * The current working directory of the terminal.
     */
    readonly cwd: string;

    /**
     * The name of the current process running in the terminal.
     */
    readonly process: string;

    /**
     * Write the provided data to the pseudoterminal.
     * @param data Data to write to the terminal.
     */
    write(data: string): void;

    /**
     * Resizes the pseudoterminal to the given dimensions.
     * @param cols Number of columns in the resized terminal.
     * @param rows Number of rows in the resized terminal.
     */
    resize(cols: number, rows: number): void;

    /**
     * Terminate the current pseudoterminal instance.
     */
    kill(): void;

    on(event: string, listener: Function): this;
    on(event: 'data', listener: (data: string) => void): this;
    on(event: 'exit', listener: () => void): this;
    on(event: 'error', listener: (err: any) => void): this;

    emit(event: string | symbol, ...args: any[]): boolean;
    emit(event: 'data', data: string): boolean;
    emit(event: 'exit'): boolean;
    emit(event: 'error', err: any): boolean;
}

class Pty extends EventEmitter implements IPty {
    cols: number;
    rows: number;

    private _server: net.Server;
    private _socket?: net.Socket;
    private _pendingFrames: Buffer[] = [];
    private _pendingBuffer?: Buffer;
    private _procname?: string;
    private _cwd?: string;
    private _shell?: string;
    private _pty?: child_process.ChildProcess;
    private _closed: boolean = false;

    get process(): string {
        return this._procname || '';
    }

    get cwd(): string {
        return this._cwd || '';
    }

    constructor(options: ResolvedOptions) {
        super();
        this.cols = options.cols;
        this.rows = options.rows;
        this._cwd = options.cwd;

        this._server = net.createServer(socket => {
            // Don't allow more than one socket
            if (this._socket) {
                socket.destroy();
            }

            this._socket = socket;

            // Don't batch up tcp writes
            this._socket.setNoDelay(true);

            this._socket.on('data', data => {
                // TODO: this results in more data copies than needed
                let buf = this._pendingBuffer
                    ? Buffer.concat([this._pendingBuffer, data])
                    : data;

                let f: frame.Frame | null;
                do {
                    f = frame.decode(buf);
                    if (f) {
                        switch (f.type) {
                            case frame.FrameType.Data:
                                this.emit('data', f.data);
                                break;
                            case frame.FrameType.Size:
                                // The backend can't send size frames
                                break;
                            case frame.FrameType.Name:
                                this._procname = f.name;
                                break;
                            case frame.FrameType.Cwd:
                                this._cwd = f.cwd;
                        }
                        if (buf.length - f.size > 0) {
                            this._pendingBuffer = Buffer.alloc(buf.length - f.size);
                            buf.copy(this._pendingBuffer, 0, f.size);
                            buf = this._pendingBuffer;
                        } else {
                            this._pendingBuffer = undefined;
                        }
                    } else {
                        this._pendingBuffer = buf;
                    }
                } while (f && this._pendingBuffer);
            });

            this._socket.on('close', hadError => this.emit('exit'));

            // Flush any pending frames
            for (const f of this._pendingFrames) {
                this._socket.write(f);
            }

            this._pendingFrames = [];
        });

        this._setup(options).catch(e => {
            if (!this._closed) {
                this.emit('error', e);
            }
        });
    }

    write(data: string) {
        const f = frame.encodeData(data);
        if (this._socket) {
            // TODO: use res
            this._socket.write(f);
        } else {
            this._pendingFrames.push(f);
        }
    }

    resize(cols: number, rows: number) {
        const f = frame.encodeSize(cols, rows);
        if (this._socket) {
            // TODO: use res
            this._socket.write(f);
        } else {
            this._pendingFrames.push(f);
        }
    }

    kill() {
        this._closed = true;
        this._server.close();
        if (this._socket) {
            this._socket.destroy();
        }

        if (this._pty && !this._pty.killed) {
            this._pty.kill();
        }

        this.removeAllListeners();
    }

    private async _setup(options: ResolvedOptions): Promise<void> {
        const backendPath = await this._getWslPath(path.join(__dirname, '../../backend/target/release/wslpty'));
        const port = await getPort();

        const escapedBackendPath = backendPath.replace(/([\/\\])app\.asar([\/\\])/, '$1app.asar.unpacked$2');
        const args = [escapedBackendPath, String(port)];

        args.push('--cols', String(options.cols));
        args.push('--rows', String(options.rows));

        if (this.cwd) {
            args.push('--cwd', await this._getWslPath(this.cwd));
        }

        if (this._shell) {
            args.push('--shell', this._shell);
        }

        this._server.listen(port, '127.0.0.1', () => {
            this._pty = child_process.spawn('wsl.exe', args, {
                stdio: 'ignore',
                env: options.env
            });
        });

        this._server.on('error', err => {
            if (!this._closed) {
                this.emit('error', err);
            }
        });

        this._server.on('close', () => {
            if (!this._closed) {
                this.emit('exit');
                if (this._pty && !this._pty.killed) {
                    this._pty.kill();
                }
            }
        });
    }

    // TODO: handle nonexistent paths (currently crashes backend)
    private async _getWslPath(windowsPath: string): Promise<string> {
        // Assume linux path if it starts with a slash
        // TODO: beef this up
        if (windowsPath.startsWith('/') || windowsPath.startsWith('~')) {
            return windowsPath;
        }

        return new Promise<string>((resolve, reject) => {
            const proc = child_process.spawn(
                'wsl.exe',
                // we have to double encode the backslashes
                ['wslpath', windowsPath.replace(/\\/g, '\\\\')]
            );

            let output = '';
            proc.stdout.on('data', data => output += data);

            proc.on('error', err => {
                reject(err);
            });

            proc.on('close', code => {
                if (code === 0) {
                    resolve(output.trim());
                } else {
                    reject(new Error(`Failed to get backend path with error code: ${code}`));
                }
            });
        });
    }
}

/**
 * Create a new pseudoterminal for communicating with the Windows Subsystem for
 * Linux, spawned in a new process.
 * @param options Options to configure the pseudoterminal.
 */
export function spawn(options?: Options): IPty {
    return new Pty({
        cols: 80,
        rows: 30,
        env: process.env,
        ...options
    });
}
