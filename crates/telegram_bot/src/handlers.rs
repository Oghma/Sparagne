use teloxide::types::User;

use crate::ConfigParameters;

mod callbacks;
mod commands;
mod pending;
mod templates;

pub(crate) use callbacks::handle_callback;
pub(crate) use commands::handle_message;

fn is_allowed(cfg: &ConfigParameters, user: Option<&User>) -> bool {
    let Some(allowed) = cfg.allowed_users.as_ref() else {
        return true;
    };
    let Some(user) = user else {
        return false;
    };
    allowed.contains(&user.id)
}
