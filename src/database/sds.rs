use std::cmp::Ordering;
use std::convert::TryFrom;
use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::str::{from_utf8, Utf8Error};

#[derive(Debug, Clone)]
enum Repr {
    /// Короткая строка, размещённая прямо в стеке.
    Inline { len: u8, buf: [u8; Sds::INLINE_CAP] },
    /// Длинная строка, размещённая в куче.
    Heap { buf: Vec<u8>, len: usize },
}

#[derive(Debug, Clone)]
pub struct Sds(Repr);

impl Sds {
    /// Максимальный размер строки, при котором используется стек.
    pub const INLINE_CAP: usize = 22;

    #[inline(always)]
    pub fn from_vec(vec: Vec<u8>) -> Self {
        let len = vec.len();
        if len <= Self::INLINE_CAP {
            let mut buf = [0u8; Self::INLINE_CAP];
            buf[..len].copy_from_slice(&vec);
            Sds(Repr::Inline {
                len: len as u8,
                buf,
            })
        } else {
            Sds(Repr::Heap { buf: vec, len })
        }
    }

    pub fn from_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        if bytes.len() <= Self::INLINE_CAP {
            let mut buf = [0u8; Self::INLINE_CAP];
            buf[..bytes.len()].copy_from_slice(bytes);
            Sds(Repr::Inline {
                len: bytes.len() as u8,
                buf,
            })
        } else {
            let vec = bytes.to_vec();
            let len = vec.len();
            Sds(Repr::Heap { buf: vec, len })
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match &self.0 {
            Repr::Inline { len, buf } => &buf[..*len as usize],
            Repr::Heap { buf, len } => &buf[..*len],
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match &mut self.0 {
            Repr::Inline { len, buf } => &mut buf[..*len as usize],
            Repr::Heap { buf, len } => &mut buf[..*len],
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        match &self.0 {
            Repr::Inline { len, .. } => *len as usize,
            Repr::Heap { len, .. } => *len,
        }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        match &self.0 {
            Repr::Inline { .. } => Self::INLINE_CAP,
            Repr::Heap { buf, .. } => buf.capacity(),
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        match &mut self.0 {
            Repr::Inline { len, buf } => {
                let cur_len = *len as usize;
                if cur_len + additional <= Self::INLINE_CAP {
                    return; // Уже влезает
                }
                let mut vec = Vec::with_capacity((cur_len + additional).next_power_of_two());
                vec.extend_from_slice(&buf[..cur_len]);
                self.0 = Repr::Heap {
                    len: cur_len,
                    buf: vec,
                };
            }
            Repr::Heap { buf, .. } => buf.reserve(additional),
        }
    }

    pub fn clear(&mut self) {
        match &mut self.0 {
            Repr::Inline { len, .. } => *len = 0,
            Repr::Heap { len, .. } => *len = 0,
        }
    }

    pub fn push(&mut self, byte: u8) {
        match &mut self.0 {
            Repr::Inline { len, buf } => {
                let cur_len = *len as usize;
                if cur_len < Self::INLINE_CAP {
                    buf[cur_len] = byte;
                    *len += 1;
                } else {
                    let mut vec = Vec::with_capacity((cur_len + 1).next_power_of_two());
                    vec.extend_from_slice(&buf[..cur_len]);
                    vec.push(byte);
                    self.0 = Repr::Heap {
                        len: vec.len(),
                        buf: vec,
                    };
                }
            }
            Repr::Heap { buf, len } => {
                if *len < buf.len() {
                    buf[*len] = byte;
                } else {
                    buf.push(byte);
                }
                *len += 1;
            }
        }
    }

    pub fn append(&mut self, other: &[u8]) {
        let total = self.len() + other.len();
        match &mut self.0 {
            Repr::Inline { len, buf } => {
                let cur_len = *len as usize;
                if total <= Self::INLINE_CAP {
                    buf[cur_len..total].copy_from_slice(other);
                    *len = total as u8;
                } else {
                    let mut vec = Vec::with_capacity(total.next_power_of_two());
                    vec.extend_from_slice(&buf[..cur_len]);
                    vec.extend_from_slice(other);
                    self.0 = Repr::Heap {
                        len: vec.len(),
                        buf: vec,
                    };
                }
            }
            Repr::Heap { buf, len } => {
                let cur_len = *len;
                let needed = cur_len + other.len();

                if buf.capacity() < needed {
                    buf.reserve((needed - buf.len()).next_power_of_two());
                }

                if buf.len() < needed {
                    buf.extend_from_slice(other);
                } else {
                    buf[cur_len..needed].copy_from_slice(other);
                }

                *len = needed;
            }
        }
    }

    pub fn truncate(&mut self, new_len: usize) {
        match &mut self.0 {
            Repr::Inline { len, .. } => {
                *len = new_len.min(*len as usize) as u8;
            }
            Repr::Heap { len, .. } => {
                *len = new_len.min(*len);
            }
        }
        self.inline_downgrade();
    }

    pub fn slice_range(&self, start: usize, end: usize) -> Self {
        assert!(start <= end && end <= self.len(), "invalid slice range");
        let slice = &self.as_slice()[start..end];

        if slice.len() <= Self::INLINE_CAP {
            let mut buf = [0u8; Self::INLINE_CAP];
            buf[..slice.len()].copy_from_slice(slice);
            Sds(Repr::Inline {
                len: slice.len() as u8,
                buf,
            })
        } else {
            let mut vec = Vec::with_capacity(slice.len());
            vec.extend_from_slice(slice);
            let len = vec.len();
            Sds(Repr::Heap { buf: vec, len })
        }
    }

    fn inline_downgrade(&mut self) {
        if let Repr::Heap { buf, len } = &self.0 {
            if *len <= Self::INLINE_CAP {
                let mut inline_buf = [0u8; Self::INLINE_CAP];
                inline_buf[..*len].copy_from_slice(&buf[..*len]);
                self.0 = Repr::Inline {
                    len: *len as u8,
                    buf: inline_buf,
                }
            }
        }
    }

    pub fn as_str(&self) -> Result<&str, Utf8Error> {
        from_utf8(self.as_slice())
    }
}

impl Deref for Sds {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for Sds {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl Display for Sds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.as_str() {
            Ok(s) => write!(f, "{s}"),
            Err(_) => write!(f, "{:?}", self.as_slice()),
        }
    }
}

impl Hash for Sds {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl PartialEq for Sds {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Sds {}

impl PartialOrd for Sds {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Sds {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl TryFrom<Sds> for String {
    type Error = Utf8Error;
    fn try_from(value: Sds) -> Result<Self, Self::Error> {
        value.as_str().map(|s| s.to_string())
    }
}
