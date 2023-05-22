use std::{collections::HashMap, io::Write};

use bytes::{Buf, Bytes};

use crate::{pb, RQError, RQResult};
use flate2::write::GzDecoder;
use prost::Message;

use super::{ForwardMessage, ForwardNode, MessageNode};

pub fn find_file_name(source: &str) -> Option<&str> {
    if let Some(xml_style) = source.split_once("m_fileName=\"") {
        return xml_style.1.split_once("\"").map(|(file_name, _)| file_name);
    }
    if let Some(json_style) = source.rsplit_once(r#"\"filename\":\""#) {
        return json_style
            .1
            .split_once(r#"\""#)
            .map(|(file_name, _)| file_name);
    }
    None
}

impl super::super::super::Engine {
    pub fn decode_multi_msg_apply_down_resp(
        &self,
        payload: Bytes,
    ) -> RQResult<pb::multimsg::MultiMsgApplyDownRsp> {
        pb::multimsg::MultiRspBody::decode(&*payload)?
            .multimsg_applydown_rsp
            .pop()
            .ok_or(RQError::EmptyField("multimsg_applydown_rsp"))
    }

    pub fn decode_multi_msg_transmit(
        &self,
        mut payload: Bytes,
        msg_key: &[u8],
    ) -> RQResult<HashMap<String, pb::msg::PbMultiMsgItem>> {
        if payload.get_u8() != 40 {
            Err(RQError::Decode(format!(
                "Unexpected body data: {payload:?}"
            )))?
        }
        let msg_head_len = payload.get_i32();
        let data_len = payload.get_i32();
        payload.advance(msg_head_len as usize); // Skip IM msg head
        let decrypted = crate::crypto::qqtea_decrypt(&payload[0..data_len as usize], msg_key);
        let compressed = pb::longmsg::LongRspBody::decode(&*decrypted)?
            .msg_down_rsp
            .pop()
            .ok_or(RQError::EmptyField("msg_down_rsp"))?
            .msg_content;
        let mut decoder = GzDecoder::new(vec![]);
        decoder.write_all(&compressed)?;
        Ok(
            pb::msg::PbMultiMsgTransmit::decode(Bytes::from(decoder.finish()?))?
                .pb_item_list
                .into_iter()
                .map(|item| (item.file_name().to_owned(), item))
                .collect(),
        )
    }

    pub fn decode_forward_message(
        &self,
        file_name: &str,
        items: &HashMap<String, pb::msg::PbMultiMsgItem>,
    ) -> RQResult<Vec<ForwardMessage>> {
        let item = match items.get(file_name) {
            Some(item) => item.clone(),
            None => Err(RQError::Other(format!(
                "{file_name} does not exist in MultiMsgItem map"
            )))?,
        };
        let msgs = item.buffer.unwrap_or_default().msg;
        let mut nodes: Vec<ForwardMessage> = Vec::with_capacity(msgs.len());
        'iter_main: for msg in msgs.into_iter() {
            let head = msg.head.unwrap_or_default();
            let sender_name: String;
            if let Some(ref group_info) = head.group_info
            && head.msg_type() == 82 {
                sender_name = String::from_utf8(group_info.group_card().into())?;
            } else {
                sender_name = head.from_nick().to_string();
            }
            let elements = msg
                .body
                .unwrap_or_default()
                .rich_text
                .unwrap_or_default()
                .elems;
            for elem in elements.iter().filter_map(|e| e.elem.as_ref()) {
                let mut file_src: Option<String> = None;
                if let crate::pb::msg::elem::Elem::RichMsg(ref elem) = elem
                && elem.service_id() == 35 {
                    let rich = crate::msg::elem::RichMsg::from(elem.clone());
                    file_src = Some(rich.template1);
                }
                else if let crate::pb::msg::elem::Elem::LightApp(ref elem) = elem
                && let light_app = crate::msg::elem::LightApp::from(elem.clone())
                && light_app.content.contains(r#"{"app":"com.tencent.multimsg""#) {
                    let light_app = crate::msg::elem::LightApp::from(elem.clone());
                    file_src = Some(light_app.content);
                }
                if let Some(file_src) = file_src {
                    let file_name = find_file_name(&file_src)
                        .ok_or_else(|| RQError::EmptyField("file_name"))?;
                    let sub_nodes = self.decode_forward_message(file_name, items)?;
                    nodes.push(ForwardMessage::Forward(ForwardNode {
                        sender_id: head.from_uin(),
                        time: head.msg_time(),
                        sender_name,
                        nodes: sub_nodes,
                    }));
                    continue 'iter_main;
                }
            }
            nodes.push(ForwardMessage::Message(MessageNode {
                sender_id: head.from_uin(),
                time: head.msg_time(),
                sender_name,
                elements: elements.into(),
            }))
        }
        Ok(nodes)
    }

    pub fn decode_multi_msg_apply_up_resp(
        &self,
        payload: Bytes,
    ) -> RQResult<pb::multimsg::MultiMsgApplyUpRsp> {
        pb::multimsg::MultiRspBody::decode(&*payload)?
            .multimsg_applyup_rsp
            .pop()
            .ok_or(RQError::EmptyField("multimsg_applyup_rsp"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn find_filename_xml() {
        let content = r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
        <msg serviceID="35" templateID="1" action="viewMultiMsg" brief="testqGnJ1R..."
            m_resid="(size=1)DBD2AB20196EEB631C95DEF40E20C709"
            m_fileName="160023" sourceMsgId="0" url=""
            flag="3" adverSign="0" multiMsgFlag="1">
            <item layout="1">
                <title>testqGnJ1R...</title>
                <hr hidden="false" style="0"/>
                <summary>点击查看完整消息</summary>
            </item>
            <source name="聊天记录" icon="" action="" appid="-1"/>
        </msg>"#;

        assert_eq!(super::find_file_name(content), Some("160023"));
    }

    #[test]
    fn find_filename_json() {
        let content = r#"{"app":"com.tencent.multimsg","desc":"[聊天记录]","view":"contact","ver":"0.0.0.5","prompt":"[聊天记录]","meta":{"detail":{"news":[{"text":" 轩:别太荒谬[图片][图片][图片][图片][图片] "},{"text":" 斩首的夜:？？？ "},{"text":" 斩首的夜:[图片] "},{"text":" 相生万物:[图片] "}],"uniseq":"7220266786072788669","resid":"HEioyxxucuHjhqd0kWWm0T7EKvui6gxFGg4cLFGbzWghQkhk2Kn1SsISdQ\/1SmXm","summary":" 查看转发消息  ","source":" 群聊的聊天记录 "}},"config":{"round":1,"forward":1,"autosize":1,"type":"normal","width":300},"extra":"{\"tsum\":4,\"filename\":\"7220266786072788669\"}"}"#;

        assert_eq!(super::find_file_name(content), Some("7220266786072788669"));
    }
}