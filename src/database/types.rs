use serde::{Deserialize, Serialize};

use super::arcbytes::ArcBytes;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Value {
    Str(ArcBytes),
}
