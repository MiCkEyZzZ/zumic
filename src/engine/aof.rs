use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, Write},
    path::Path,
};

pub struct AofLog {
    file: File,
}

impl AofLog {
    /// Open (or create) AOF file.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)?;
        // Move to the brginning for replay
        file.seek(io::SeekFrom::Start(0))?;
        Ok(AofLog { file })
    }

    /// Add SET entry
    pub fn append_set(&mut self, key: &[u8], value: &[u8]) -> io::Result<()> {
        self.file.write_all(&[1])?;
        self.file.write_all(&(key.len() as u32).to_be_bytes())?;
        self.file.write_all(key)?;
        self.file.write_all(&(value.len() as u32).to_be_bytes())?;
        self.file.write_all(value)?;
        self.file.flush()?;
        Ok(())
    }

    /// Add DEL entry
    pub fn append_del(&mut self, key: &[u8]) -> io::Result<()> {
        self.file.write_all(&[2])?;
        self.file.write_all(&(key.len() as u32).to_be_bytes())?;
        self.file.write_all(key)?;
        self.file.flush()?;
        Ok(())
    }

    /// Read the entire log and replay operations in the callback
    pub fn replay<F>(&mut self, mut f: F) -> io::Result<()>
    where
        F: FnMut(u8, Vec<u8>, Option<Vec<u8>>),
    {
        self.file.seek(io::SeekFrom::Start(0))?;
        let mut buf = Vec::new();
        self.file.read_to_end(&mut buf)?;
        let mut pos = 0;
        while pos < buf.len() {
            let op = buf[pos];
            pos += 1;
            let key_len = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;
            let key = buf[pos..pos + key_len].to_vec();
            pos += key_len;
            if op == 1 {
                let val_len = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
                pos += 4;
                let val = buf[pos..pos + val_len].to_vec();
                pos += val_len;
                f(op, key, Some(val));
            } else {
                f(op, key, None);
            }
        }
        Ok(())
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
        let path = temp_file.path().to_path_buf();

        // Open the log
        {
            let mut log = AofLog::open(&path)?;

            // Add operations
            log.append_set(b"kin", b"dzadza")?;
            log.append_del(b"kin")?;
        }

        // Reopen the log and check the replay
        {
            let mut log = AofLog::open(&path)?;
            let mut entries = Vec::new();

            log.replay(|op, key, val| {
                entries.push((op, key, val));
            })?;

            // Check the entries
            assert_eq!(entries.len(), 2);

            // Check SET operation
            assert_eq!(entries[0].0, 1);
            assert_eq!(entries[0].1, b"kin");
            assert_eq!(entries[0].2.as_deref(), Some(b"dzadza".as_ref()));

            // Check DEL operation
            assert_eq!(entries[1].0, 2);
            assert_eq!(entries[1].1, b"kin");
            assert_eq!(entries[1].2, None);
        }

        Ok(())
    }

    /// This test checks the addition of multiple SET operations to the AOF log
    /// and their subsequent replay.
    #[test]
    fn test_append_multiple_set() -> io::Result<()> {
        let temp_file = NamedTempFile::new()?;
        let path = temp_file.path().to_path_buf();

        {
            let mut log = AofLog::open(&path)?;
            log.append_set(b"k1", b"v1")?;
            log.append_set(b"k2", b"v2")?;
            log.append_set(b"k3", b"v3")?;
        }

        {
            let mut log = AofLog::open(&path)?;
            let mut entries = Vec::new();

            log.replay(|op, key, val| {
                entries.push((op, key, val));
            })?;

            // Verify that 3 operations have been replayed
            assert_eq!(entries.len(), 3);
            for (i, (k, v)) in [("k1", "v1"), ("k2", "v2"), ("k3", "v3")]
                .iter()
                .enumerate()
            {
                assert_eq!(entries[i].0, 1); // Check if operation is SET (1)
                assert_eq!(entries[i].1, k.as_bytes());
                assert_eq!(entries[i].2.as_deref(), Some(v.as_bytes()));
            }
        }

        Ok(())
    }
}
