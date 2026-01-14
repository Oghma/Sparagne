#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Locale {
    It,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextKey {
    WelcomeTemplate,
    HelpText,
    MergeCategoryHelp,
    MembersHelp,
    FlowMembersHelp,
    UnsetValue,
    UnallocatedFlow,
    HomeSummary,
    HomeBtnExpense,
    HomeBtnIncome,
    HomeBtnRefund,
    HomeBtnList,
    HomeBtnStats,
    HomeBtnWalletDefault,
    HomeBtnFlowDefault,
    PickerWalletTitle,
    PickerFlowTitle,
    WizardTitleExpense,
    WizardTitleIncome,
    WizardTitleRefund,
    WizardBody,
    WizardBtnInput,
    WizardBtnWallet,
    WizardBtnFlow,
    WizardBtnCategoryNone,
    WizardBtnCategoryReset,
    WizardBtnRecents,
    WizardBtnHome,
    ListHeader,
    ListPrev,
    ListNext,
    ListToggleVoided,
    ListStateOn,
    ListStateOff,
    ListBtnHome,
    DetailHeader,
    DetailYes,
    DetailNo,
    DetailLegs,
    DetailBtnVoid,
    DetailBtnEdit,
    DetailBtnRepeat,
    DetailBtnBack,
    EditMenuTitle,
    EditMenuAmount,
    EditMenuNote,
    StatsSummary,
    StatsBtnHome,
    TxVoidedSuffix,
    TxKindTransferWallet,
    TxKindTransferFlow,
    WizardPromptExpense,
    WizardPromptIncome,
    WizardPromptRefund,
    WizardErrorEmpty,
    WizardErrorExpensePlus,
    WizardErrorExpenseRefund,
    WizardErrorIncomeRefund,
    UnknownUser,
    PairingRequired,
    PairingPrompt,
    PreferencesSaveError,
    DefaultWalletMissing,
    TooManyTags,
    InvalidAmountExample,
    InvalidAmountExampleShort,
    InvalidAmount,
    InvalidAmountPositive,
    TransactionVoided,
    EditAmountPrompt,
    EditNotePrompt,
    EditAmountUpdated,
    EditNoteUpdated,
    QuickAddSaved,
    AlreadySaved,
    RepeatSuccess,
    RepeatUnsupported,
    RepeatNoWallet,
    QuickAddUndo,
    ApiNetworkError,
    ApiMembershipLastOwner,
    ApiMembershipOwnerImmutable,
    ApiMembershipOwnerRemoveForbidden,
    ApiUnauthorized,
    ApiForbidden,
    ApiNotFound,
    ApiConflict,
    ApiBadRequestUserNotFound,
    ApiServerError,
    MembersVaultTitle,
    MembersFlowTitle,
    MemberSaved,
    MemberRemoved,
    VaultDeleted,
    CategorySourceMissing,
    CategoryDestinationMissing,
    CategoryListHint,
    MergeConfirmPrompt,
    MergeCompleted,
    CategoryListEmpty,
    CategoryListHeader,
    CategoryFlagSystem,
    CategoryFlagArchived,
    MembersEmpty,
    FlowNotFoundNone,
    FlowNotFound,
    FlowAvailableHeader,
    MergeConflictHeader,
    MergeConflictListHeader,
    MergeConflictSame,
    MergeConflictSourceSystem,
    MergeConflictTargetArchived,
    MergeConflictAlias,
    MergeConflictName,
    MergeConflictFallback,
    VaultDeleteHelp,
    RoleOwner,
    RoleEditor,
    RoleViewer,
}

pub(crate) fn default_locale() -> Locale {
    Locale::It
}

pub(crate) fn resolve_locale(telegram_language: Option<&str>) -> Locale {
    let Some(code) = telegram_language else {
        return default_locale();
    };
    if code.starts_with("it") {
        Locale::It
    } else {
        default_locale()
    }
}

pub(crate) fn t(locale: Locale, key: TextKey) -> &'static str {
    match (locale, key) {
        (Locale::It, TextKey::WelcomeTemplate) => {
            "Benvenuto, {display_name}!\n\nOra puoi inserire voci al volo scrivendo ad esempio:\n\n12.50 bar caff\u{e8}\n+1000 stipendio\nr 5.20 amazon\n\nImposta i default (wallet/flow) usando i bottoni."
        }
        (Locale::It, TextKey::HelpText) => {
            "Esempi:\n\n12.50 bar caff\u{e8}\n-12.50 bar caff\u{e8}\n+1000 stipendio\nr 5.20 amazon\n\n#tag opzionale (max 1): 12.50 bar #food caff\u{e8}\n\nComandi:\n/home\n/categories\n/merge_category <da> -> <a>\n/merge_category confirm <da> -> <a>\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>\n/flow_members <flow>\n/flow_members add <flow> <username> <owner|editor|viewer>\n/flow_members remove <flow> <username>\n/vault_delete\n/vault_delete confirm"
        }
        (Locale::It, TextKey::MergeCategoryHelp) => {
            "Uso:\n/merge_category <da> -> <a>\n/merge_category confirm <da> -> <a>"
        }
        (Locale::It, TextKey::MembersHelp) => {
            "Uso:\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>"
        }
        (Locale::It, TextKey::FlowMembersHelp) => {
            "Uso:\n/flow_members <flow>\n/flow_members add <flow> <username> <owner|editor|viewer>\n/flow_members remove <flow> <username>\n\nNota: il flow pu\u{f2} contenere spazi."
        }
        (Locale::It, TextKey::UnsetValue) => "Non impostato",
        (Locale::It, TextKey::UnallocatedFlow) => "Non in flow",
        (Locale::It, TextKey::HomeSummary) => {
            "{display_name} \u{2022} Vault: {vault}\nWallet default: {wallet}\nFlow default: {flow_default}\nUltimo flow: {flow_last}"
        }
        (Locale::It, TextKey::HomeBtnExpense) => "Uscita",
        (Locale::It, TextKey::HomeBtnIncome) => "Entrata",
        (Locale::It, TextKey::HomeBtnRefund) => "Refund",
        (Locale::It, TextKey::HomeBtnList) => "Ultime",
        (Locale::It, TextKey::HomeBtnStats) => "Stats",
        (Locale::It, TextKey::HomeBtnWalletDefault) => "Wallet default",
        (Locale::It, TextKey::HomeBtnFlowDefault) => "Flow default",
        (Locale::It, TextKey::PickerWalletTitle) => "Scegli il wallet di default:",
        (Locale::It, TextKey::PickerFlowTitle) => "Scegli il flow (ultimo flow usato):",
        (Locale::It, TextKey::WizardTitleExpense) => "Nuova uscita",
        (Locale::It, TextKey::WizardTitleIncome) => "Nuova entrata",
        (Locale::It, TextKey::WizardTitleRefund) => "Nuovo rimborso/storno",
        (Locale::It, TextKey::WizardBody) => {
            "Wallet: {wallet}\nFlow: {flow}\nCategoria: {category}\n\nTip: puoi anche scrivere direttamente in chat (quick add)."
        }
        (Locale::It, TextKey::WizardBtnInput) => "Inserisci",
        (Locale::It, TextKey::WizardBtnWallet) => "Wallet",
        (Locale::It, TextKey::WizardBtnFlow) => "Flow",
        (Locale::It, TextKey::WizardBtnCategoryNone) => "Nessuna",
        (Locale::It, TextKey::WizardBtnCategoryReset) => "Reset",
        (Locale::It, TextKey::WizardBtnRecents) => "Recenti",
        (Locale::It, TextKey::WizardBtnHome) => "Home",
        (Locale::It, TextKey::ListHeader) => "Ultime voci:",
        (Locale::It, TextKey::ListPrev) => "Prev",
        (Locale::It, TextKey::ListNext) => "Next",
        (Locale::It, TextKey::ListToggleVoided) => "Mostra voided: {state}",
        (Locale::It, TextKey::ListStateOn) => "On",
        (Locale::It, TextKey::ListStateOff) => "Off",
        (Locale::It, TextKey::ListBtnHome) => "Home",
        (Locale::It, TextKey::DetailHeader) => {
            "Dettaglio\n\nKind: {kind}\nQuando: {when}\nImporto: {amount}\nCategoria: {category}\nNota: {note}\nVoided: {voided}"
        }
        (Locale::It, TextKey::DetailYes) => "si",
        (Locale::It, TextKey::DetailNo) => "no",
        (Locale::It, TextKey::DetailLegs) => "Legs",
        (Locale::It, TextKey::DetailBtnVoid) => "Void",
        (Locale::It, TextKey::DetailBtnEdit) => "Edit",
        (Locale::It, TextKey::DetailBtnRepeat) => "Ripeti",
        (Locale::It, TextKey::DetailBtnBack) => "Indietro",
        (Locale::It, TextKey::EditMenuTitle) => "Cosa vuoi modificare?",
        (Locale::It, TextKey::EditMenuAmount) => "Importo",
        (Locale::It, TextKey::EditMenuNote) => "Nota",
        (Locale::It, TextKey::StatsSummary) => {
            "Stats\n\nBilancio: {balance}\nTotale entrate: {income}\nTotale uscite: {expenses}"
        }
        (Locale::It, TextKey::StatsBtnHome) => "Home",
        (Locale::It, TextKey::TxVoidedSuffix) => " \u{2022} void",
        (Locale::It, TextKey::TxKindTransferWallet) => "tw",
        (Locale::It, TextKey::TxKindTransferFlow) => "tf",
        (Locale::It, TextKey::WizardPromptExpense) => {
            "Invia una uscita, es:\n\n12.50 bar caff\u{e8}\n12.50 bar #food caff\u{e8}\n\n(oppure scrivi direttamente nella chat senza usare il wizard)"
        }
        (Locale::It, TextKey::WizardPromptIncome) => {
            "Invia una entrata, es:\n\n1000 stipendio\n+1000 #salary stipendio\n\n(oppure scrivi direttamente nella chat senza usare il wizard)"
        }
        (Locale::It, TextKey::WizardPromptRefund) => {
            "Invia un rimborso/storno, es:\n\nr 5.20 amazon\nr 5.20 #shopping amazon\n\n(oppure scrivi direttamente nella chat senza usare il wizard)"
        }
        (Locale::It, TextKey::WizardErrorEmpty) => "Testo vuoto.",
        (Locale::It, TextKey::WizardErrorExpensePlus) => {
            "Selezionato: uscita. Rimuovi il '+' (es: 12.50 bar)."
        }
        (Locale::It, TextKey::WizardErrorExpenseRefund) => {
            "Selezionato: uscita. Per refund usa il bottone \"Refund\"."
        }
        (Locale::It, TextKey::WizardErrorIncomeRefund) => {
            "Selezionato: entrata. Rimuovi 'r' (es: 1000 stipendio)."
        }
        (Locale::It, TextKey::UnknownUser) => "Impossibile identificare l'utente.",
        (Locale::It, TextKey::PairingRequired) => "Per fare pairing: /start <codice>",
        (Locale::It, TextKey::PairingPrompt) => "Inserisci il codice di pairing:",
        (Locale::It, TextKey::PreferencesSaveError) => "Errore nel salvataggio delle preferenze.",
        (Locale::It, TextKey::DefaultWalletMissing) => "Imposta prima un wallet di default.",
        (Locale::It, TextKey::TooManyTags) => "Troppi tag: massimo 1.",
        (Locale::It, TextKey::InvalidAmountExample) => "Importo non valido (es: 10 o 10.50).",
        (Locale::It, TextKey::InvalidAmountExampleShort) => "Importo non valido (es: 10 o 10.50)",
        (Locale::It, TextKey::InvalidAmount) => "Importo non valido.",
        (Locale::It, TextKey::InvalidAmountPositive) => "Importo non valido (deve essere > 0).",
        (Locale::It, TextKey::TransactionVoided) => "\u{2705} Voce annullata (void).",
        (Locale::It, TextKey::EditAmountPrompt) => "Invia il nuovo importo (es: 10.50):",
        (Locale::It, TextKey::EditNotePrompt) => "Invia la nuova nota (vuoto per rimuovere):",
        (Locale::It, TextKey::EditAmountUpdated) => "\u{2705} Importo aggiornato.",
        (Locale::It, TextKey::EditNoteUpdated) => "\u{2705} Nota aggiornata.",
        (Locale::It, TextKey::QuickAddSaved) => "\u{2705} Salvato: {amount}",
        (Locale::It, TextKey::AlreadySaved) => "\u{2705} Gi\u{e0} salvato.",
        (Locale::It, TextKey::RepeatSuccess) => "\u{2705} Ripetuta.",
        (Locale::It, TextKey::RepeatUnsupported) => "Ripetizione non supportata per questo tipo.",
        (Locale::It, TextKey::RepeatNoWallet) => "Transazione senza wallet: non posso ripeterla.",
        (Locale::It, TextKey::QuickAddUndo) => "Undo",
        (Locale::It, TextKey::ApiNetworkError) => {
            "Problemi di connessione con il server. Riprova pi\u{f9} tardi!"
        }
        (Locale::It, TextKey::ApiMembershipLastOwner) => {
            "Non puoi rimuovere l'ultimo owner del flow."
        }
        (Locale::It, TextKey::ApiMembershipOwnerImmutable) => {
            "Non puoi cambiare il ruolo dell'owner del vault."
        }
        (Locale::It, TextKey::ApiMembershipOwnerRemoveForbidden) => {
            "Non puoi rimuovere l'owner del vault."
        }
        (Locale::It, TextKey::ApiUnauthorized) => {
            "Non autorizzato. Usa /start per fare il pairing."
        }
        (Locale::It, TextKey::ApiForbidden) => "Operazione non permessa.",
        (Locale::It, TextKey::ApiNotFound) => "Risorsa non trovata. Prova a reimpostare i default.",
        (Locale::It, TextKey::ApiConflict) => "Richiesta duplicata (gi\u{e0} salvata).",
        (Locale::It, TextKey::ApiBadRequestUserNotFound) => {
            "Codice di pairing non valido (o stai usando un database diverso da quello del server)."
        }
        (Locale::It, TextKey::ApiServerError) => "Errore server.",
        (Locale::It, TextKey::MembersVaultTitle) => "Membri vault",
        (Locale::It, TextKey::MembersFlowTitle) => "Membri flow \"{flow}\"",
        (Locale::It, TextKey::MemberSaved) => "\u{2705} Membro salvato: {username} ({role})",
        (Locale::It, TextKey::MemberRemoved) => "\u{2705} Membro rimosso: {username}",
        (Locale::It, TextKey::VaultDeleted) => "\u{2705} Vault eliminato.",
        (Locale::It, TextKey::CategorySourceMissing) => "Categoria sorgente non trovata: {name}",
        (Locale::It, TextKey::CategoryDestinationMissing) => {
            "Categoria destinazione non trovata: {name}"
        }
        (Locale::It, TextKey::CategoryListHint) => "Usa /categories per vedere la lista.",
        (Locale::It, TextKey::MergeConfirmPrompt) => {
            "Ok, posso unire \"{from}\" -> \"{into}\".\nConferma con:\n/merge_category confirm {from} -> {into}"
        }
        (Locale::It, TextKey::MergeCompleted) => "Unione completata: \"{from}\" -> \"{into}\".",
        (Locale::It, TextKey::CategoryListEmpty) => {
            "Nessuna categoria. Aggiungi una transazione con #categoria per iniziare."
        }
        (Locale::It, TextKey::CategoryListHeader) => "Categorie:",
        (Locale::It, TextKey::CategoryFlagSystem) => " [system]",
        (Locale::It, TextKey::CategoryFlagArchived) => " [archived]",
        (Locale::It, TextKey::MembersEmpty) => "Nessun membro.",
        (Locale::It, TextKey::FlowNotFoundNone) => {
            "Flow \"{name}\" non trovato. Nessun flow condivisibile."
        }
        (Locale::It, TextKey::FlowNotFound) => "Flow \"{name}\" non trovato.",
        (Locale::It, TextKey::FlowAvailableHeader) => "Flow disponibili:",
        (Locale::It, TextKey::MergeConflictHeader) => {
            "Merge non possibile: \"{from}\" -> \"{into}\"."
        }
        (Locale::It, TextKey::MergeConflictListHeader) => "Conflitti:",
        (Locale::It, TextKey::MergeConflictSame) => "Le categorie sono identiche.",
        (Locale::It, TextKey::MergeConflictSourceSystem) => {
            "La categoria \"{name}\" e' di sistema."
        }
        (Locale::It, TextKey::MergeConflictTargetArchived) => {
            "La categoria \"{name}\" e' archiviata."
        }
        (Locale::It, TextKey::MergeConflictAlias) => "Alias in conflitto: {value}",
        (Locale::It, TextKey::MergeConflictName) => "Nome in conflitto: {value}",
        (Locale::It, TextKey::MergeConflictFallback) => "Conflitto: {kind} ({value})",
        (Locale::It, TextKey::VaultDeleteHelp) => {
            "Uso:\n/vault_delete confirm\n\nAttenzione: elimina il vault Main e tutti i dati."
        }
        (Locale::It, TextKey::RoleOwner) => "owner",
        (Locale::It, TextKey::RoleEditor) => "editor",
        (Locale::It, TextKey::RoleViewer) => "viewer",
    }
}

pub(crate) fn format(locale: Locale, key: TextKey, pairs: &[(&str, &str)]) -> String {
    let mut text = t(locale, key).to_string();
    for (token, value) in pairs {
        let needle = format!("{{{token}}}");
        text = text.replace(&needle, value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_locale_defaults_when_missing() {
        assert_eq!(resolve_locale(None), default_locale());
    }

    #[test]
    fn resolve_locale_falls_back_for_unknown() {
        assert_eq!(resolve_locale(Some("en")), default_locale());
        assert_eq!(resolve_locale(Some("fr-FR")), default_locale());
    }

    #[test]
    fn resolve_locale_accepts_italian_prefix() {
        assert_eq!(resolve_locale(Some("it")), Locale::It);
        assert_eq!(resolve_locale(Some("it-IT")), Locale::It);
    }
}
