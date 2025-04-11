use std::collections::HashMap;

use super::command::Response;
use crate::{database::Value, network::zsp::frame::zsp_types::ZSPFrame};

pub fn serialize_response(response: Response) -> ZSPFrame {
    match response {
        Response::Ok => ZSPFrame::SimpleString("OK".into()),
        Response::Value(value) => value_to_frame(value), // Всё перенаправляется в helper
        Response::Error(msg) => ZSPFrame::FrameError(msg),
    }
}

fn value_to_frame(value: Value) -> ZSPFrame {
    match value {
        Value::Str(s) => ZSPFrame::BulkString(Some(s.to_vec())),
        Value::Int(i) => ZSPFrame::Integer(i),
        Value::Float(f) => ZSPFrame::Float(f),
        Value::Bool(b) => ZSPFrame::SimpleString(b.to_string()),
        Value::Null => ZSPFrame::Null,
        Value::List(list) => {
            let frames = list
                .iter()
                .map(|item| ZSPFrame::BulkString(Some(item.to_vec())))
                .collect();
            ZSPFrame::Array(Some(frames))
        }
        Value::Hash(map) => {
            let dict: HashMap<String, ZSPFrame> = map
                .into_iter()
                .map(|(k, v)| {
                    let key =
                        String::from_utf8(k.to_vec()).unwrap_or_else(|_| "<invalid utf8>".into());
                    let val = ZSPFrame::BulkString(Some(v.to_vec()));
                    (key, val)
                })
                .collect();
            ZSPFrame::Dictionary(Some(dict))
        }
        Value::ZSet { dict, .. } => {
            let pairs = dict
                .into_iter()
                .map(|(k, score)| {
                    let key =
                        String::from_utf8(k.to_vec()).unwrap_or_else(|_| "<invalid utf8>".into());
                    (key, score)
                })
                .collect();
            ZSPFrame::ZSet(pairs)
        }
        Value::Set(set) => {
            let frames = set
                .into_iter()
                .map(|item| ZSPFrame::SimpleString(item))
                .collect();
            ZSPFrame::Array(Some(frames))
        }
        Value::HyperLogLog(_) => ZSPFrame::SimpleString("HLL(NotImplemented)".into()),
        Value::SStream(_) => ZSPFrame::SimpleString("SStream(NotImplemented)".into()),
    }
}
