// the following code is based on https://github.com/serde-rs/json/blob/3fd6f5f49dc1c732d9b1d7dfece4f02c0d440d39/src/value/mod.rs.
// License: https://github.com/serde-rs/json/blob/3fd6f5f49dc1c732d9b1d7dfece4f02c0d440d39/LICENSE-MIT

use serde::Serialize;
use std::fmt::{self, Display};
use std::io;

pub struct AsJson<A>(pub A);

impl<A: Serialize> Display for AsJson<A> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        struct WriterFormatter<'a, 'b: 'a> {
            inner: &'a mut fmt::Formatter<'b>,
        }

        impl<'a, 'b> io::Write for WriterFormatter<'a, 'b> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                // Safety: the serializer below only emits valid utf8 when using
                // the default formatter.
                let s = unsafe { std::str::from_utf8_unchecked(buf) };
                self.inner.write_str(s).map_err(io_error)?;
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        fn io_error(_: fmt::Error) -> io::Error {
            // Error value does not matter because Display impl just maps it
            // back to fmt::Error.
            io::Error::new(io::ErrorKind::Other, "fmt error")
        }

        let alternate = f.alternate();
        let mut wr = WriterFormatter { inner: f };
        if alternate {
            // {:#}
            serde_json::ser::to_writer_pretty(&mut wr, &self.0).map_err(|_| fmt::Error)
        } else {
            // {}
            serde_json::ser::to_writer(&mut wr, &self.0).map_err(|_| fmt::Error)
        }
    }
}
