use std::io;
use std::io::Cursor;

use byteorder::{BigEndian, ReadBytesExt};
use bytes::{BufMut, Bytes, BytesMut};

const DATA_FRAME_TYPE: u8 = 0;
const SIZE_FRAME_TYPE: u8 = 1;
const NAME_FRAME_TYPE: u8 = 2;

#[derive(Debug)]
pub enum Frame {
    Data(Bytes),
    Size(u16, u16),
    Name(Bytes),
}

pub fn encode(frame: Frame, buf: &mut BytesMut) -> Result<(), io::Error> {
    match frame {
        Frame::Data(bytes) => {
            buf.reserve(5 + bytes.len());
            buf.put_u32_be(1 + bytes.len() as u32);
            buf.put_u8(DATA_FRAME_TYPE);
            buf.put(bytes);
        }
        Frame::Size(rows, cols) => {
            buf.reserve(9);
            buf.put_u32_be(5);
            buf.put_u8(SIZE_FRAME_TYPE);
            buf.put_u16_be(cols);
            buf.put_u16_be(rows);
        }
        Frame::Name(bytes) => {
            buf.reserve(5 + bytes.len());
            buf.put_u32_be(1 + bytes.len() as u32);
            buf.put_u8(NAME_FRAME_TYPE);
            buf.put(bytes);
        }
    };
    Ok(())
}

pub struct FrameDecoder {
    cur_size: usize,
}

impl FrameDecoder {
    pub fn new() -> FrameDecoder {
        FrameDecoder { cur_size: 0 }
    }

    pub fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Frame>, io::Error> {
        // We don't have a size yet
        if self.cur_size == 0 && buf.len() >= 4 {
            let size = buf.split_to(4);
            let mut reader = Cursor::new(size);
            self.cur_size = reader
                .read_u32::<BigEndian>()
                .expect("Didn't receive a valid size") as usize;
        }

        // Only process the data if we were actually able to process a size
        if self.cur_size != 0 && buf.len() >= self.cur_size {
            let frame_type = buf[0];
            buf.advance(1);
            let res = match frame_type {
                DATA_FRAME_TYPE => {
                    assert!(self.cur_size > 0);
                    let bytes = buf.split_to(self.cur_size - 1).freeze();
                    Ok(Some(Frame::Data(bytes)))
                }
                SIZE_FRAME_TYPE => {
                    let dimensions = buf.split_to(4);
                    let mut reader = Cursor::new(dimensions);
                    let cols = reader.read_u16::<BigEndian>().expect("Couldn't parse cols");
                    let rows = reader.read_u16::<BigEndian>().expect("Couldn't parse rows");
                    Ok(Some(Frame::Size(cols, rows)))
                }
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Encountered unknown frame type",
                )),
            };
            self.cur_size = 0;
            res
        } else {
            Ok(None)
        }
    }
}
