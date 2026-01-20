use api_types::transaction::{TransactionKind, TransactionView};
use engine::{Currency as EngineCurrency, Money};
use teloxide::{prelude::*, types::InputFile};

use crate::{
    ConfigParameters,
    bot_client::BotClient,
    i18n::{self, TextKey},
    use_cases::shared,
};

/// Handles the /export command - generates and sends a CSV file with all
/// transactions.
pub(crate) async fn handle_export(
    bot: &dyn BotClient,
    chat_id: ChatId,
    user_id: u64,
    cfg: &ConfigParameters,
    locale: i18n::Locale,
) -> ResponseResult<()> {
    // Notify user that export is being generated
    bot.send_message(chat_id, i18n::t(locale, TextKey::ExportGenerating), None)
        .await?;

    // Get vault snapshot for currency info
    let prefs = cfg.prefs.get_or_default(user_id).await;
    let vault_ref = shared::vault_ref_from_prefs(&prefs);
    let snapshot = match cfg.api.vault_snapshot(user_id, &vault_ref).await {
        Ok(s) => s,
        Err(err) => {
            shared::send_api_error(bot, chat_id, locale, err).await?;
            return Ok(());
        }
    };

    let currency = shared::engine_currency(snapshot.currency);
    let wallet_id = prefs
        .default_wallet_id
        .filter(|id| snapshot.wallets.iter().any(|w| w.id == *id));

    // Fetch all transactions (paginated)
    let mut all_transactions: Vec<TransactionView> = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let list = match cfg
            .api
            .transactions_list(
                user_id,
                &api_types::transaction::TransactionList {
                    vault_id: snapshot.id.clone(),
                    flow_id: None,
                    wallet_id,
                    limit: Some(100), // Fetch in batches of 100
                    cursor,
                    from: None,
                    to: None,
                    kinds: None,
                    include_voided: Some(true), // Include all for export
                    include_transfers: Some(true),
                },
            )
            .await
        {
            Ok(v) => v,
            Err(err) => {
                shared::send_api_error(bot, chat_id, locale, err).await?;
                return Ok(());
            }
        };

        all_transactions.extend(list.transactions);

        match list.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    if all_transactions.is_empty() {
        bot.send_message(chat_id, i18n::t(locale, TextKey::ExportEmpty), None)
            .await?;
        return Ok(());
    }

    // Generate CSV
    let csv_content = generate_csv(locale, &all_transactions, currency);

    // Create filename with date
    let now = chrono::Local::now();
    let filename = format!("sparagne_export_{}.csv", now.format("%Y%m%d_%H%M%S"));

    // Send as document
    let input_file = InputFile::memory(csv_content.into_bytes()).file_name(filename);
    bot.send_document(chat_id, input_file, i18n::t(locale, TextKey::ExportReady))
        .await?;

    Ok(())
}

/// Generates CSV content from transactions.
fn generate_csv(
    locale: i18n::Locale,
    transactions: &[TransactionView],
    currency: EngineCurrency,
) -> String {
    let mut csv = String::from("data,tipo,importo,categoria,nota,annullata\n");

    for tx in transactions {
        let date = tx.occurred_at.format("%Y-%m-%d");
        let kind_str = match tx.kind {
            TransactionKind::Expense => {
                if locale == i18n::Locale::It {
                    "spesa"
                } else {
                    "expense"
                }
            }
            TransactionKind::Income => {
                if locale == i18n::Locale::It {
                    "entrata"
                } else {
                    "income"
                }
            }
            TransactionKind::Refund => {
                if locale == i18n::Locale::It {
                    "rimborso"
                } else {
                    "refund"
                }
            }
            TransactionKind::TransferWallet => {
                if locale == i18n::Locale::It {
                    "trasf_wallet"
                } else {
                    "transfer_wallet"
                }
            }
            TransactionKind::TransferFlow => {
                if locale == i18n::Locale::It {
                    "trasf_budget"
                } else {
                    "transfer_budget"
                }
            }
        };

        let amount = Money::new(tx.amount_minor).format(currency);
        let category = tx.category.as_deref().unwrap_or("");
        // Escape commas and quotes in note
        let note = tx
            .note
            .as_deref()
            .unwrap_or("")
            .replace('"', "\"\"")
            .replace(',', ";");
        let voided = if tx.voided {
            if locale == i18n::Locale::It {
                "sì"
            } else {
                "yes"
            }
        } else {
            "no"
        };

        csv.push_str(&format!(
            "{date},{kind_str},{amount},\"{category}\",\"{note}\",{voided}\n"
        ));
    }

    csv
}
