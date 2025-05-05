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
