use crate::log_info;
use anyhow::Result;
use serde_json::{Value, json};

use super::SendTg;

impl SendTg {
    pub(super) fn send_message(
        &mut self,
        chat_id: &str,
        message: &str,
        silent: bool,
        reply_markup: Option<&Value>,
        thread_id: Option<i64>,
    ) -> Result<()> {
        self.send_chat_action(chat_id, "typing", thread_id);

        let mut payload = json!({
            "chat_id": chat_id,
            "text": message.replace("\\n", "\n"),
            "parse_mode": "HTML",
            "disable_notification": silent,
        });

        if let Some(id) = thread_id {
            payload["message_thread_id"] = json!(id);
        }

        if let Some(markup) = reply_markup {
            payload["reply_markup"] = markup.clone();
        }

        let url = format!("{}{}/sendMessage", self.api_url, self.bot_token);
        let response = self.client.post(&url).json(&payload).send();

        match self.handle_response("Failed to send message:", response) {
            Ok(_) => {
                let target = self.target_label(thread_id);
                log_info!("Message sent to {}: {}", target, message);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
}
