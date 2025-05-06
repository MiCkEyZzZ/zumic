use std::{
    fs::{File, OpenOptions},
    io::{self, BufWriter, Read, Seek, Write},
    path::Path,
};

/// 4-byte magic header for the AOF file format (version identifier).
const MAGIC: &[u8; 4] = b"AOF1";

/// Operation code used in AOF log.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AofOp {
    Set = 1,
    Del = 2,
}

/// Append-Only File (AOF) log with buffered writing and safe replay functionality.
pub struct AofLog {
    writer: BufWriter<File>,
    reader: File,
}

impl AofLog {
    /// Opens (or creates) the AOF file and verifies or writes the magic header.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        // Open the file for reading and appending.
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)?;

        // Read or initialize the magic header.
        {
            let mut header = [0u8; 4];
            let n = file.read(&mut header)?;
            if n == 4 {
                if &header != MAGIC {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid AOF magic header",
                    ));
                }
            } else {
                // Empty file - write the header.
                file.seek(io::SeekFrom::Start(0))?;
                file.write_all(MAGIC)?;
                file.flush()?;
            }
        }

        // Reset the file cursor to the beginning for reading.
        file.seek(io::SeekFrom::Start(0))?;

        // Create a separate buffered writer.
        let writer = BufWriter::new(OpenOptions::new().create(true).append(true).open(path)?);

        Ok(AofLog {
            writer,
            reader: file,
        })
    }

    /// Appends a SET operation to the AOF log.
    pub fn append_set(&mut self, key: &[u8], value: &[u8]) -> io::Result<()> {
        self.writer.write_all(&[AofOp::Set as u8])?;
        Self::write_32(&mut self.writer, key.len() as u32)?;
        self.writer.write_all(key)?;
        Self::write_32(&mut self.writer, value.len() as u32)?;
        self.writer.write_all(value)?;
        self.writer.flush()?; // Ensure durability
        Ok(())
    }

    /// Appends a DEL operation to the AOF log.
    pub fn append_del(&mut self, key: &[u8]) -> io::Result<()> {
        self.writer.write_all(&[AofOp::Del as u8])?;
        Self::write_32(&mut self.writer, key.len() as u32)?;
        self.writer.write_all(key)?;
        self.writer.flush()?; // Ensure durability
        Ok(())
    }

    /// Replays all operations in the log by calling the provided callback for each entry.
    pub fn replay<F>(&mut self, mut f: F) -> io::Result<()>
    where
        F: FnMut(AofOp, Vec<u8>, Option<Vec<u8>>),
    {
        // Reset reader to the beginning.
        self.reader.seek(io::SeekFrom::Start(0))?;

        // Read and validate the magic header.
        let mut header = [0u8; 4];
        self.reader.read_exact(&mut header)?;
        if &header != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad AOF header"));
        }

        // Read the full log contents into memory.
        let mut buf = Vec::new();
        self.reader.read_to_end(&mut buf)?;
        let mut pos = 0;

        while pos < buf.len() {
            // Read operation code.
            let op = AofOp::try_from(buf[pos])?;
            pos += 1;

            // Read key.
            let key_len = Self::read_u32(&buf, &mut pos)? as usize;
            if pos + key_len > buf.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Key truncated",
                ));
            }
            let key = buf[pos..pos + key_len].to_vec();
            pos += key_len;

            // If SET, read value.
            let val = if op == AofOp::Set {
                let vlen = Self::read_u32(&buf, &mut pos)? as usize;
                if pos + vlen > buf.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Value truncated",
                    ));
                }
                let v = buf[pos..pos + vlen].to_vec();
                pos += vlen;
                Some(v)
            } else {
                None
            };

            f(op, key, val);
        }

        Ok(())
    }

    /// Placeholder for a future AOF compaction/rewrite feature.
    /// Accepts an iterator of live key-value pairs to write into a new compacted file.
    pub fn rewrite<I>(&mut self, _live: I) -> io::Result<()>
    where
        I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
    {
        // TODO: create a new file, write the MAGIC header and only the latest SET entries from `live`,
        // then atomically replace the old AOF file.
        unimplemented!()
    }

    /// Helper to write a u32 in big-endian format.
    #[inline]
    fn write_32<W: Write>(w: &mut W, v: u32) -> io::Result<()> {
        w.write_all(&v.to_be_bytes())
    }

    /// Safely reads a u32 in big-endian format from a buffer with bounds checking.
    #[inline]
    fn read_u32(buf: &[u8], pos: &mut usize) -> io::Result<u32> {
        if *pos + 4 > buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Unexpected EOF while reading u32",
            ));
        }
        let mut arr = [0u8; 4];
        arr.copy_from_slice(&buf[*pos..*pos + 4]);
        *pos += 4;
        Ok(u32::from_be_bytes(arr))
    }
}

impl TryFrom<u8> for AofOp {
    type Error = io::Error;
    fn try_from(v: u8) -> io::Result<Self> {
        match v {
            1 => Ok(AofOp::Set),
            2 => Ok(AofOp::Del),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown AOF op: {}", v),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;

    use super::*;

    /// This test checks the addition of SET and DEL operations to the AOF log
    /// and their subsequent replay.
    #[test]
    fn test_append_and_replay_set_and_del() -> io::Result<()> {
        // Create a temporary file for the test
        let temp_file = NamedTempFile::new()?;
        let path = temp_file.path();

        {
            let mut log = AofLog::open(path)?;
            log.append_set(b"kin", b"dzadza")?;
            log.append_del(b"kin")?;
        }

        {
            let mut log = AofLog::open(path)?;
            let mut seq = Vec::new();
            log.replay(|op, key, val| seq.push((op, key, val)))?;

            assert_eq!(seq.len(), 2);
            assert_eq!(seq[0].0, AofOp::Set);
            assert_eq!(&seq[0].1, b"kin");
            assert_eq!(seq[0].2.as_deref(), Some(b"dzadza".as_ref()));
            assert_eq!(seq[1].0, AofOp::Del);
            assert_eq!(&seq[1].1, b"kin");
            assert!(seq[1].2.is_none());
        }

        Ok(())
    }

    /// This test checks the addition of multiple SET operations to the AOF log
    /// and their subsequent replay.
    #[test]
    fn test_append_multiple_set() -> io::Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path();

        {
            let mut log = AofLog::open(path)?;
            log.append_set(b"k1", b"v1")?;
            log.append_set(b"k2", b"v2")?;
            log.append_set(b"k3", b"v3")?;
        }

        {
            let mut log = AofLog::open(path)?;
            let mut seq = Vec::new();
            log.replay(|op, key, val| seq.push((op, key, val)))?;
            assert_eq!(seq.len(), 3);
            for (i, (k, v)) in [("k1", "v1"), ("k2", "v2"), ("k3", "v3")]
                .iter()
                .enumerate()
            {
                assert_eq!(seq[i].0, AofOp::Set);
                assert_eq!(&seq[i].1, k.as_bytes());
                assert_eq!(seq[i].2.as_deref(), Some(v.as_bytes()));
            }
        }

        Ok(())
    }
}
