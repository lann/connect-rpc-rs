use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures_util::{stream, Stream, StreamExt, TryStream, TryStreamExt};
use http_body::Body;
use http_body_util::BodyExt;

use crate::{BoxError, Error};

pub struct ConnectFrame {
    pub compressed: bool,
    pub end: bool,
    pub data: Bytes,
}

const FLAGS_COMPRESSED: u8 = 0b1;
const FLAGS_END: u8 = 0b01;

impl ConnectFrame {
    pub fn body_stream<B>(body: B) -> impl Stream<Item = Result<Self, Error>>
    where
        B: Body<Error: Into<BoxError>>,
    {
        Self::bytes_stream(body.into_data_stream())
    }

    pub fn bytes_stream<S>(s: S) -> impl Stream<Item = Result<Self, Error>>
    where
        S: TryStream<Ok: Buf, Error: Into<BoxError>>,
    {
        let mut parse_state = FrameParseState::default();
        s.map_err(Error::body)
            .map(Some)
            .chain(stream::iter([None]))
            .flat_map(move |item| stream::iter(parse_state.feed(item)))
    }
}

#[derive(Default)]
struct FrameParseState {
    buf: BytesMut,
    failed: bool,
}

impl FrameParseState {
    fn feed(&mut self, item: Option<Result<impl Buf, Error>>) -> Vec<Result<ConnectFrame, Error>> {
        if self.failed {
            return vec![];
        }
        let data = match item {
            Some(Ok(data)) => data,
            Some(Err(err)) => {
                self.failed = true;
                return vec![Err(Error::body(err))];
            }
            None => {
                if !self.buf.is_empty() {
                    return vec![Err(Error::body("partial frame at end of stream"))];
                }
                return vec![];
            }
        };

        self.buf.put(data);

        let mut frames = vec![];
        loop {
            match self.parse_frame() {
                Ok(Some(frame)) => frames.push(Ok(frame)),
                Ok(None) => return frames,
                Err(err) => {
                    self.failed = true;
                    frames.push(Err(err));
                }
            }
        }
    }

    fn parse_frame(&mut self) -> Result<Option<ConnectFrame>, Error> {
        if self.buf.len() < 5 {
            return Ok(None);
        }
        let data_len = (&self.buf[1..]).get_u32();
        let Ok(frame_len) = ((data_len as u64) + 5).try_into() else {
            return Err(Error::body("frame too large"));
        };
        if self.buf.len() < frame_len {
            return Ok(None);
        }
        let mut frame = self.buf.split_to(frame_len);
        let data = frame.split_off(5).freeze();
        let flags = frame[0];
        Ok(Some(ConnectFrame {
            compressed: flags & FLAGS_COMPRESSED != 0,
            end: flags & FLAGS_END != 0,
            data,
        }))
    }
}
