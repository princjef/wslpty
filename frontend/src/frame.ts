export interface DataFrame {
    type: FrameType.Data;
    size: number;
    data: string;
}

export interface SizeFrame {
    type: FrameType.Size;
    size: number;
    cols: number;
    rows: number;
}

export interface NameFrame {
    type: FrameType.Name;
    size: number;
    name: string;
}

export interface CwdFrame {
    type: FrameType.Cwd;
    size: number;
    cwd: string;
}

export enum FrameType {
    Data = 0,
    Size = 1,
    Name = 2,
    Cwd = 3
}

export type Frame = DataFrame | SizeFrame | NameFrame | CwdFrame;

export function decode(buf: Buffer): Frame | null {
    if (buf.length < 4) {
        return null;
    }

    const size = buf.readUInt32BE(0);
    if (buf.length < size + 4) {
        return null;
    }

    const type: FrameType = buf.readUInt8(4);

    switch (type) {
        case FrameType.Data:
            return {
                type: FrameType.Data,
                size: size + 4,
                data: buf.slice(5, 5 + size - 1).toString('utf8')
            };
        case FrameType.Size:
            return {
                type: FrameType.Size,
                size: size + 4,
                cols: buf.readUInt16BE(5),
                rows: buf.readUInt16BE(7)
            };
        case FrameType.Name:
            return {
                type: FrameType.Name,
                size: size + 4,
                name: buf.slice(5, 5 + size - 1).toString('utf8')
            };
        case FrameType.Cwd:
            return {
                type: FrameType.Cwd,
                size: size + 4,
                cwd: buf.slice(5, 5 + size - 1).toString('utf8')
            };
        default:
            throw new Error(`Unknown frame type: ${type}`);
    }
}

export function encodeData(data: string): Buffer {
    const dataBuf = Buffer.from(data, 'utf8');
    const buf = Buffer.alloc(5 + dataBuf.length);
    buf.writeUInt32BE(dataBuf.length + 1, 0);
    buf.writeUInt8(FrameType.Data, 4);
    dataBuf.copy(buf, 5);
    return buf;
}

export function encodeSize(cols: number, rows: number): Buffer {
    const buf = Buffer.alloc(9);
    buf.writeUInt32BE(5, 0);
    buf.writeUInt8(FrameType.Size, 4);
    buf.writeUInt16BE(cols, 5);
    buf.writeUInt16BE(rows, 7);
    return buf;
}
