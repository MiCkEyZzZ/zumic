use bytes::Bytes;
use serde_json::json;
use zumic::pubsub::{
    decode_command, decode_message, encode_message, encode_publish, Message, MessagePayload,
    PubSubCommand,
};

// Этот тест симулирует roundtrip pubsub через wire-level команды и фреймы
#[test]
fn test_full_pubsub_roundtrip() {
    // 1. Клиент формирует команду PUBLISH с JSON payload
    let payload = MessagePayload::Json(json!({
        "event": "user_login",
        "user_id": 42,
        "details": { "ip": "127.0.0.1" }
    }));

    let channel = "events";
    let wire_payload = encode_publish(channel, &payload).expect("encode publish");

    // 2. Сервер декодирует wire payload → PubSubCommand
    let mut data = wire_payload.clone();
    let cmd = decode_command(&mut data)
        .expect("decode command")
        .expect("some cmd");

    match cmd {
        PubSubCommand::Publish(ch, pl) => {
            assert_eq!(ch, channel);
            match pl {
                MessagePayload::Json(ref v) => assert_eq!(v["user_id"], 42),
                _ => panic!("Payload is not JSON"),
            }
        }
        _ => panic!("Unexpected command"),
    }

    // 3. Сервер проводит публикацию — создаёт Message, кодирует его обратно (wire)
    let msg = Message::with_payload(channel, payload);
    let wire_msg = encode_message(&msg).expect("encode message");
    let mut wire_msg_data = wire_msg.clone();

    // 4. Клиент принимает байты, декодирует обратно в Message
    let decoded = decode_message(&mut wire_msg_data)
        .expect("decode message")
        .expect("got message");
    assert_eq!(decoded.channel.as_ref(), channel);
    match decoded.payload {
        MessagePayload::Json(ref v) => assert_eq!(v["event"], "user_login"),
        _ => panic!("Wrong payload variant"),
    }
}

// Тест на работу с бинарным Serialized payload
#[test]
fn test_serialized_binary_payload_integration() {
    let payload_bytes = vec![1, 2, 3, 4, 255, 0, 128];
    let payload = MessagePayload::Serialized {
        data: Bytes::from(payload_bytes.clone()),
        content_type: "application/zsp-custom".to_string(),
    };
    let channel = "bin";

    // Формируем publish, декодируем команду, создаём Message, обратно кодируем и
    // декодируем
    let wire_cmd = encode_publish(channel, &payload).expect("encode publish");
    let mut data = wire_cmd;
    let cmd = decode_command(&mut data)
        .expect("decode cmd")
        .expect("got cmd");
    match cmd {
        PubSubCommand::Publish(ch, pl) => {
            assert_eq!(ch, channel);
            match pl {
                MessagePayload::Serialized { data, content_type } => {
                    assert_eq!(content_type, "application/zsp-custom");
                    assert_eq!(data.as_ref(), payload_bytes.as_slice());
                }
                _ => panic!("Should be serialized payload"),
            }
        }
        _ => panic!("Unexpected command"),
    }

    // Симулируем обратную отправку сообщения в сеть (Wire)
    let msg = Message::with_payload(channel, payload);
    let wire_msg = encode_message(&msg).expect("encode message");
    let mut wire_msg_data = wire_msg;
    let decoded_msg = decode_message(&mut wire_msg_data)
        .expect("decode msg")
        .expect("got msg");
    match decoded_msg.payload {
        MessagePayload::Serialized { data, content_type } => {
            assert_eq!(content_type, "application/zsp-custom");
            assert_eq!(data.as_ref(), payload_bytes.as_slice());
        }
        _ => panic!("Wrong payload variant!"),
    }
}
