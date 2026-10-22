use crate::db::Database;
use crate::error::AppResult;
use crate::models::{Conversation, ConversationDraft, Message, MessageDraft};

pub(crate) fn create_conversation(
    db: &Database,
    draft: ConversationDraft,
) -> AppResult<Conversation> {
    db.create_conversation(draft)
}

pub(crate) fn bind_conversation_model(
    db: &Database,
    conversation_id: &str,
    model_config_id: Option<&str>,
) -> AppResult<()> {
    db.update_conversation_model(conversation_id, model_config_id)
}

pub(crate) fn load_conversation_history(
    db: &Database,
    conversation_id: &str,
) -> AppResult<Vec<Message>> {
    db.list_messages(conversation_id)
}

pub(crate) fn append_conversation_message(
    db: &Database,
    draft: MessageDraft,
) -> AppResult<Message> {
    db.append_message(draft)
}
