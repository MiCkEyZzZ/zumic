use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    fmt::{self, Display},
    ops::Deref,
    str::from_utf8,
    sync::Arc,
};

use mlua::{Lua, Result, UserData, UserDataMethods};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Value {
    Str(ArcBytes),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArcBytes(Arc<Bytes>);

impl ArcBytes {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn from_vec(vec: Vec<u8>) -> Self {
        Self(Arc::new(Bytes::from(vec)))
    }
    pub fn from_str(s: &str) -> Self {
        Self(Arc::new(Bytes::copy_from_slice(s.as_bytes())))
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }
}

impl Serialize for ArcBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0[..])
    }
}

impl<'de> Deserialize<'de> for ArcBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        Ok(ArcBytes(Arc::new(Bytes::from(bytes))))
    }
}

impl Deref for ArcBytes {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0[..]
    }
}

impl AsRef<[u8]> for ArcBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl Display for ArcBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match from_utf8(&self.0) {
            Ok(s) => write!(f, "{}", s),
            Err(_) => write!(f, "<invalid utf-8>"),
        }
    }
}

impl From<&str> for ArcBytes {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

impl From<Vec<u8>> for ArcBytes {
    fn from(vec: Vec<u8>) -> Self {
        Self::from_vec(vec)
    }
}

impl UserData for ArcBytes {
    fn add_methods(methods: &mut UserDataMethods<Self>) {
        methods.add_method("len", |_, this, ()| Ok(this.len()));
        methods.add_method("as_slice", |_, this, ()| Ok(this.as_slice()));
        methods.add_method("to_vec", |_, this, ()| Ok(this.to_vec()));
        methods.add_method("as_str", |_, this, ()| {
            Ok(this.as_str().unwrap_or("<invalid utf-8>"))
        });
    }
}
