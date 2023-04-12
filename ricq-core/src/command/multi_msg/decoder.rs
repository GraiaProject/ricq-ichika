use std::{collections::HashMap, io::Write};

use bytes::{Buf, Bytes};

use crate::{pb, RQError, RQResult};
use flate2::write::GzDecoder;
use prost::Message;

use super::{ForwardMessage, ForwardNode, MessageNode};

fn find_res_id(template: &str) -> Option<&str> {
    template
        .rsplit_once("m_resid=\"")
        .and_then(|v| v.1.split_once("\""))
        .map(|v| v.0)
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
        name: &str,
        items: &HashMap<String, pb::msg::PbMultiMsgItem>,
    ) -> RQResult<Vec<ForwardMessage>> {
        let item = match items.get(name) {
            Some(item) => item.clone(),
            None => Err(RQError::Other(format!(
                "{name} does not exist in MultiMsgItem map"
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
                if let crate::pb::msg::elem::Elem::RichMsg(ref elem) = elem
                && elem.service_id() == 35 {
                    let rich = crate::msg::elem::RichMsg::from(elem.clone());
                    let res_id = find_res_id(&rich.template1)
                        .ok_or_else(|| RQError::EmptyField("m_resid"))?;
                    let sub_nodes = self.decode_forward_message(res_id, items)?;
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
