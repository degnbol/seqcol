use anyhow::Result;
use std::fs::File;
use flate2::read::MultiGzDecoder;
use std::io::{self, BufRead, BufReader, Cursor, Read};
use std::mem;

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Reader that gunzips its input if the input starts with [`GZIP_MAGIC`].
///
/// The magic number is checked on the first `read`, so creating a `Gunzip` reads nothing.
enum Gunzip {
    /// Input before the magic number is checked, and the bytes read from it so far.
    Unchecked { reader: Box<dyn Read>, prefix: Vec<u8> },
    /// Plain or gunzipping reader of the input.
    Checked(Box<dyn Read>),
}

impl Read for Gunzip {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Gunzip::Checked(reader) => reader.read(buf),
            Gunzip::Unchecked { reader, prefix } => {
                let n_missing = GZIP_MAGIC.len() - prefix.len();
                reader.take(n_missing as u64).read_to_end(prefix)?;
                let is_gzip = *prefix == GZIP_MAGIC;
                let chained = Cursor::new(mem::take(prefix)).chain(mem::replace(reader, Box::new(io::empty())));
                *self = Gunzip::Checked(if is_gzip {
                    Box::new(MultiGzDecoder::new(chained))
                } else {
                    Box::new(chained)
                });
                self.read(buf)
            }
        }
    }
}

/// Bytes of `reader`, gunzipped if they start with the gzip magic number.
///
/// All gzip members are read, e.g. of bgzip files.
///
/// # Arguments
/// * `reader` - Plain or gzip-compressed input. Nothing is read from it before the first read.
///
/// # Returns
/// Buffered plain bytes. Corrupt gzip data gives a read error.
fn decompressed(reader: impl Read + 'static) -> Box<dyn BufRead> {
    Box::new(BufReader::new(Gunzip::Unchecked { reader: Box::new(reader), prefix: Vec::new() }))
}

/// Opens a file, or stdin for `-`, gunzipped if it starts with the gzip magic number.
///
/// All gzip members are read, e.g. of bgzip files.
///
/// # Arguments
/// * `filename` - Path, or `-` for stdin
///
/// # Returns
/// Buffered plain bytes. Corrupt gzip data gives a read error.
///
/// # Errors
/// If the file does not open.
pub fn open(filename: &str) -> Result<Box<dyn BufRead>> {
    match filename {
        "-" => Ok(decompressed(io::stdin())),
        _ => Ok(decompressed(File::open(filename)?)),
    }
}

/// Lines of `reader` as bytes, like [`BufRead::split`] on `\n`.
///
/// The lines end after the first read error.
///
/// # Arguments
/// * `reader` - Input
///
/// # Returns
/// Each line without its trailing `\n`, `\r\n`, or `\r`, or the read error.
pub fn read_lines(reader: impl BufRead) -> impl Iterator<Item = io::Result<Vec<u8>>> {
    let mut lines = reader.split(b'\n');
    let mut failed = false;
    std::iter::from_fn(move || {
        if failed {
            return None;
        }
        let line = lines.next()?;
        failed = line.is_err();
        Some(line.map(|mut line| {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            line
        }))
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    #[test]
    fn read_lines_keeps_invalid_utf8() {
        let reader: Box<dyn BufRead> = Box::new(&b"A\xffC\r\nGT"[..]);
        let lines: Vec<Vec<u8>> = read_lines(reader).map(Result::unwrap).collect();
        assert_eq!(lines, [&b"A\xffC"[..], b"GT"]);
    }

    pub(crate) struct FailingReader;

    impl io::Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read failed"))
        }
    }

    #[test]
    fn read_lines_ends_after_error() {
        assert_eq!(read_lines(BufReader::new(FailingReader)).count(), 1);
    }

    /// Reader that returns `chunks` in order, at most one per `read`, then end of input.
    struct ScriptedReader {
        chunks: std::collections::VecDeque<io::Result<&'static [u8]>>,
    }

    impl ScriptedReader {
        fn new(chunks: impl IntoIterator<Item = io::Result<&'static [u8]>>) -> Self {
            Self { chunks: chunks.into_iter().collect() }
        }
    }

    impl Read for ScriptedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let Some(chunk) = self.chunks.pop_front() else { return Ok(0) };
            let chunk = chunk?;
            let (head, tail) = chunk.split_at(buf.len().min(chunk.len()));
            buf[..head.len()].copy_from_slice(head);
            if !tail.is_empty() {
                self.chunks.push_front(Ok(tail));
            }
            Ok(head.len())
        }
    }

    #[test]
    fn read_lines_stops_reading_after_error() {
        let mut reader = ScriptedReader::new([Err(io::Error::other("read failed")), Ok(&b"GT"[..])]);
        assert_eq!(read_lines(BufReader::new(&mut reader)).count(), 1);
        assert_eq!(reader.chunks.len(), 1);
    }

    #[test]
    fn decompressed_continues_after_read_error() {
        let reader = ScriptedReader::new([Ok(&b"A"[..]), Err(io::Error::other("read failed")), Ok(b"CGT")]);
        let mut text = decompressed(reader);
        let mut bytes = Vec::new();
        assert!(text.read_to_end(&mut bytes).is_err());
        text.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"ACGT");
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn read_all(reader: impl Read + 'static) -> Vec<u8> {
        let mut bytes = Vec::new();
        decompressed(reader).read_to_end(&mut bytes).unwrap();
        bytes
    }

    const FASTA: &[u8] = b">s1\nACGT\n>s2\nTTGA\n";

    #[test]
    fn decompressed_passes_plain_through() {
        assert_eq!(read_all(FASTA), FASTA);
    }

    #[test]
    fn decompressed_gunzips() {
        assert_eq!(read_all(io::Cursor::new(gzip(FASTA))), FASTA);
    }

    #[test]
    fn decompressed_reads_all_gzip_members() {
        let members = [gzip(b">s1\nACGT\n"), gzip(b">s2\nTTGA\n")].concat();
        assert_eq!(read_all(io::Cursor::new(members)), FASTA);
    }

    #[test]
    fn decompressed_finds_magic_split_across_reads() {
        let gz = gzip(FASTA);
        let split = io::Cursor::new(gz[..1].to_vec()).chain(io::Cursor::new(gz[1..].to_vec()));
        assert_eq!(read_all(split), FASTA);
    }

    #[test]
    fn decompressed_passes_short_input_through() {
        for short in [&b""[..], b"A", b"\x1f"] {
            assert_eq!(read_all(short), short);
        }
    }

    #[test]
    fn decompressed_fails_on_corrupt_gzip() {
        let gz = gzip(FASTA);
        for corrupt in [b"\x1f\x8bnot gzip\n".to_vec(), gz[..gz.len() / 2].to_vec()] {
            assert!(decompressed(io::Cursor::new(corrupt)).read_to_end(&mut Vec::new()).is_err());
        }
    }
}
