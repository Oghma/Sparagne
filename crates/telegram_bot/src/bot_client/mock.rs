use std::sync::Mutex;

use async_trait::async_trait;
use teloxide::{
    requests::ResponseResult,
    types::{ChatId, InlineKeyboardMarkup, MessageId},
};

use super::BotClient;

#[derive(Clone, Debug)]
pub(crate) struct SentMessage {
    pub(crate) chat_id: ChatId,
    pub(crate) text: String,
    pub(crate) has_kb: bool,
    pub(crate) message_id: MessageId,
}

#[derive(Clone, Debug)]
pub(crate) struct EditedMessage {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    pub(crate) text: String,
    pub(crate) has_kb: bool,
}

#[derive(Default)]
pub(crate) struct MockBot {
    sent: Mutex<Vec<SentMessage>>,
    edited: Mutex<Vec<EditedMessage>>,
    next_id: Mutex<i32>,
}

impl MockBot {
    pub(crate) fn new() -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
            edited: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
        }
    }

    pub(crate) fn last_sent(&self) -> Option<SentMessage> {
        self.sent.lock().expect("mock bot lock").last().cloned()
    }

    pub(crate) fn last_edited(&self) -> Option<EditedMessage> {
        self.edited.lock().expect("mock bot lock").last().cloned()
    }
}

#[async_trait]
impl BotClient for MockBot {
    async fn send_message(
        &self,
        chat_id: ChatId,
        text: &str,
        kb: Option<InlineKeyboardMarkup>,
    ) -> ResponseResult<MessageId> {
        let mut next_id = self.next_id.lock().expect("mock bot lock");
        let message_id = MessageId(*next_id);
        *next_id += 1;

        self.sent.lock().expect("mock bot lock").push(SentMessage {
            chat_id,
            text: text.to_string(),
            has_kb: kb.is_some(),
            message_id,
        });

        Ok(message_id)
    }

    async fn edit_message_text(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        text: &str,
        kb: InlineKeyboardMarkup,
    ) -> ResponseResult<()> {
        self.edited
            .lock()
            .expect("mock bot lock")
            .push(EditedMessage {
                chat_id,
                message_id,
                text: text.to_string(),
                has_kb: !kb.inline_keyboard.is_empty(),
            });
        Ok(())
    }
}
