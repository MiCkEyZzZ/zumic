use super::types::ZSPFrame;

pub struct ZSPEncoder;

impl ZSPEncoder {}

impl ZSPEncoder {
    pub fn encode(frame: &ZSPFrame) -> Vec<u8> {
        match frame {
            ZSPFrame::SimpleString(s) => format!("+{}\r\n", s).into_bytes(),
            ZSPFrame::Error(s) => format!("-{}\r\n", s).into_bytes(),
            ZSPFrame::Integer(i) => format!(":{}\r\n", i).into_bytes(),
            ZSPFrame::BulkString(Some(b)) => {
                let mut out = format!("${}\r\n", b.len()).into_bytes();
                out.extend(b);
                out.extend(b"\r\n");
                out
            }
            ZSPFrame::BulkString(None) => b"$-1\r\n".to_vec(),
            ZSPFrame::Array(Some(elements)) => {
                let mut out = format!("*{}\r\n", elements.len()).into_bytes();
                for e in elements {
                    out.extend(Self::encode(e));
                }
                out
            }
            ZSPFrame::Array(None) => b"*-1\r\n".to_vec(),
        }
    }
}
