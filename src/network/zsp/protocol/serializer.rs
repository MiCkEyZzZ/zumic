use super::command::Response;
use crate::network::zsp::frame::types::ZSPFrame;

pub fn serialize_response(response: Response) -> ZSPFrame {
    match response {
        Response::Ok => ZSPFrame::SimpleString("OK".into()),
        Response::Value(Some(value)) => ZSPFrame::BulkString(Some(value)),
        Response::Value(None) => ZSPFrame::Null,
        Response::Error(msg) => ZSPFrame::FrameError(msg),
    }
}
