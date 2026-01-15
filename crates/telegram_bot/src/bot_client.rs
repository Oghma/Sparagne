use async_trait::async_trait;
use teloxide::{
    payloads::{EditMessageTextSetters, SendMessageSetters},
    requests::{Requester, ResponseResult},
    types::{ChatId, InlineKeyboardMarkup, MessageId},
};

#[async_trait]
pub(crate) trait BotClient: Send + Sync {
    async fn send_message(
        &self,
        chat_id: ChatId,
        text: &str,
        kb: Option<InlineKeyboardMarkup>,
    ) -> ResponseResult<MessageId>;

    async fn edit_message_text(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        text: &str,
        kb: InlineKeyboardMarkup,
    ) -> ResponseResult<()>;
}

#[async_trait]
impl BotClient for teloxide::Bot {
    async fn send_message(
        &self,
        chat_id: ChatId,
        text: &str,
        kb: Option<InlineKeyboardMarkup>,
    ) -> ResponseResult<MessageId> {
        let request = Requester::send_message(self, chat_id, text);
        let request = if let Some(kb) = kb {
            request.reply_markup(kb)
        } else {
            request
        };
        let message = request.await?;
        Ok(message.id)
    }

    async fn edit_message_text(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        text: &str,
        kb: InlineKeyboardMarkup,
    ) -> ResponseResult<()> {
        Requester::edit_message_text(self, chat_id, message_id, text)
            .reply_markup(kb)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod mock;
