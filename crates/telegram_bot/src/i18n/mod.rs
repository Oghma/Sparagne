#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Locale {
    It,
    En,
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
    HomeBtnList,
    HomeBtnStats,
    HomeBtnSettings,
    HomeBtnCommands,
    SettingsTitle,
    SettingsBtnWallet,
    SettingsBtnFlow,
    CommandsTitle,
    CommandsBody,
    PickerWalletTitle,
    PickerFlowTitle,
    WizardTitleExpense,
    WizardTitleIncome,
    WizardTitleRefund,
    WizardBodySimple,
    WizardBtnInput,
    WizardBtnWallet,
    WizardBtnFlow,
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
    TxKindExpense,
    TxKindIncome,
    TxKindRefund,
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
    } else if code.starts_with("en") {
        Locale::En
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
            "👋 {display_name}\n\n🏦 {vault}\n👛 Portafoglio: {wallet}\n🎯 Budget: {flow}"
        }
        (Locale::It, TextKey::HomeBtnExpense) => "Uscita",
        (Locale::It, TextKey::HomeBtnIncome) => "Entrata",
        (Locale::It, TextKey::HomeBtnList) => "Ultime",
        (Locale::It, TextKey::HomeBtnStats) => "Stats",
        (Locale::It, TextKey::HomeBtnSettings) => "Impostazioni",
        (Locale::It, TextKey::HomeBtnCommands) => "Comandi",
        (Locale::It, TextKey::SettingsTitle) => "⚙️ Impostazioni",
        (Locale::It, TextKey::SettingsBtnWallet) => "Cambia Portafoglio",
        (Locale::It, TextKey::SettingsBtnFlow) => "Cambia Budget",
        (Locale::It, TextKey::CommandsTitle) => "📋 Comandi disponibili",
        (Locale::It, TextKey::CommandsBody) => {
            "/home - Torna alla home\n/help - Mostra aiuto\n/categories - Lista categorie\n/members - Gestisci membri vault\n\n📝 Quick add (scrivi direttamente):\n12.50 bar caffè\n+1000 stipendio\nr 5.20 amazon\n\n#tag opzionale: 12.50 #food caffè"
        }
        (Locale::It, TextKey::PickerWalletTitle) => "Scegli il portafoglio:",
        (Locale::It, TextKey::PickerFlowTitle) => "Scegli il budget:",
        (Locale::It, TextKey::WizardTitleExpense) => "Nuova uscita",
        (Locale::It, TextKey::WizardTitleIncome) => "Nuova entrata",
        (Locale::It, TextKey::WizardTitleRefund) => "Nuovo rimborso/storno",
        (Locale::It, TextKey::WizardBodySimple) => {
            "👛 Portafoglio: {wallet}\n🎯 Budget: {flow}\n\n💡 Scrivi: importo [#categoria] [nota]\nEs: 12.50 #food caffè"
        }
        (Locale::It, TextKey::WizardBtnInput) => "Inserisci",
        (Locale::It, TextKey::WizardBtnWallet) => "Wallet",
        (Locale::It, TextKey::WizardBtnFlow) => "Flow",
        (Locale::It, TextKey::WizardBtnHome) => "Home",
        (Locale::It, TextKey::ListHeader) => "Ultime voci:",
        (Locale::It, TextKey::ListPrev) => "Prev",
        (Locale::It, TextKey::ListNext) => "Next",
        (Locale::It, TextKey::ListToggleVoided) => "Mostra voided: {state}",
        (Locale::It, TextKey::ListStateOn) => "On",
        (Locale::It, TextKey::ListStateOff) => "Off",
        (Locale::It, TextKey::ListBtnHome) => "Home",
        (Locale::It, TextKey::DetailHeader) => {
            "📋 Dettaglio\n\n📌 Tipo: {kind}\n📅 Data: {when}\n💶 Importo: {amount}\n🏷 Categoria: {category}\n📝 Nota: {note}\n❌ Annullato: {voided}"
        }
        (Locale::It, TextKey::DetailYes) => "si",
        (Locale::It, TextKey::DetailNo) => "no",
        (Locale::It, TextKey::DetailBtnVoid) => "Annulla",
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
        (Locale::It, TextKey::TxKindExpense) => "Uscita",
        (Locale::It, TextKey::TxKindIncome) => "Entrata",
        (Locale::It, TextKey::TxKindRefund) => "Rimborso",
        (Locale::It, TextKey::TxKindTransferWallet) => "Trasf. portafoglio",
        (Locale::It, TextKey::TxKindTransferFlow) => "Trasf. budget",
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

        // English translations
        (Locale::En, TextKey::WelcomeTemplate) => {
            "Welcome, {display_name}!\n\nYou can now add entries on the fly by writing:\n\n12.50 coffee shop\n+1000 salary\nr 5.20 amazon\n\nSet defaults (wallet/budget) using the buttons."
        }
        (Locale::En, TextKey::HelpText) => {
            "Examples:\n\n12.50 coffee shop\n-12.50 coffee shop\n+1000 salary\nr 5.20 amazon\n\nOptional #tag (max 1): 12.50 coffee #food\n\nCommands:\n/home\n/categories\n/merge_category <from> -> <to>\n/merge_category confirm <from> -> <to>\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>\n/flow_members <budget>\n/flow_members add <budget> <username> <owner|editor|viewer>\n/flow_members remove <budget> <username>\n/vault_delete\n/vault_delete confirm"
        }
        (Locale::En, TextKey::MergeCategoryHelp) => {
            "Usage:\n/merge_category <from> -> <to>\n/merge_category confirm <from> -> <to>"
        }
        (Locale::En, TextKey::MembersHelp) => {
            "Usage:\n/members\n/members add <username> <owner|editor|viewer>\n/members remove <username>"
        }
        (Locale::En, TextKey::FlowMembersHelp) => {
            "Usage:\n/flow_members <budget>\n/flow_members add <budget> <username> <owner|editor|viewer>\n/flow_members remove <budget> <username>\n\nNote: budget name can contain spaces."
        }
        (Locale::En, TextKey::UnsetValue) => "Not set",
        (Locale::En, TextKey::UnallocatedFlow) => "Unallocated",
        (Locale::En, TextKey::HomeSummary) => {
            "👋 {display_name}\n\n🏦 {vault}\n👛 Wallet: {wallet}\n🎯 Budget: {flow}"
        }
        (Locale::En, TextKey::HomeBtnExpense) => "Expense",
        (Locale::En, TextKey::HomeBtnIncome) => "Income",
        (Locale::En, TextKey::HomeBtnList) => "History",
        (Locale::En, TextKey::HomeBtnStats) => "Stats",
        (Locale::En, TextKey::HomeBtnSettings) => "Settings",
        (Locale::En, TextKey::HomeBtnCommands) => "Commands",
        (Locale::En, TextKey::SettingsTitle) => "⚙️ Settings",
        (Locale::En, TextKey::SettingsBtnWallet) => "Change Wallet",
        (Locale::En, TextKey::SettingsBtnFlow) => "Change Budget",
        (Locale::En, TextKey::CommandsTitle) => "📋 Available commands",
        (Locale::En, TextKey::CommandsBody) => {
            "/home - Go to home\n/help - Show help\n/categories - List categories\n/members - Manage vault members\n\n📝 Quick add (type directly):\n12.50 coffee shop\n+1000 salary\nr 5.20 amazon\n\nOptional #tag: 12.50 #food coffee"
        }
        (Locale::En, TextKey::PickerWalletTitle) => "Choose wallet:",
        (Locale::En, TextKey::PickerFlowTitle) => "Choose budget:",
        (Locale::En, TextKey::WizardTitleExpense) => "New expense",
        (Locale::En, TextKey::WizardTitleIncome) => "New income",
        (Locale::En, TextKey::WizardTitleRefund) => "New refund",
        (Locale::En, TextKey::WizardBodySimple) => {
            "👛 Wallet: {wallet}\n🎯 Budget: {flow}\n\n💡 Type: amount [#category] [note]\nEx: 12.50 #food coffee"
        }
        (Locale::En, TextKey::WizardBtnInput) => "Enter",
        (Locale::En, TextKey::WizardBtnWallet) => "Wallet",
        (Locale::En, TextKey::WizardBtnFlow) => "Budget",
        (Locale::En, TextKey::WizardBtnHome) => "Home",
        (Locale::En, TextKey::ListHeader) => "Recent entries:",
        (Locale::En, TextKey::ListPrev) => "Prev",
        (Locale::En, TextKey::ListNext) => "Next",
        (Locale::En, TextKey::ListToggleVoided) => "Show voided: {state}",
        (Locale::En, TextKey::ListStateOn) => "On",
        (Locale::En, TextKey::ListStateOff) => "Off",
        (Locale::En, TextKey::ListBtnHome) => "Home",
        (Locale::En, TextKey::DetailHeader) => {
            "📋 Detail\n\n📌 Type: {kind}\n📅 Date: {when}\n💶 Amount: {amount}\n🏷 Category: {category}\n📝 Note: {note}\n❌ Voided: {voided}"
        }
        (Locale::En, TextKey::DetailYes) => "yes",
        (Locale::En, TextKey::DetailNo) => "no",
        (Locale::En, TextKey::DetailBtnVoid) => "Void",
        (Locale::En, TextKey::DetailBtnEdit) => "Edit",
        (Locale::En, TextKey::DetailBtnRepeat) => "Repeat",
        (Locale::En, TextKey::DetailBtnBack) => "Back",
        (Locale::En, TextKey::EditMenuTitle) => "What do you want to edit?",
        (Locale::En, TextKey::EditMenuAmount) => "Amount",
        (Locale::En, TextKey::EditMenuNote) => "Note",
        (Locale::En, TextKey::StatsSummary) => {
            "Stats\n\nBalance: {balance}\nTotal income: {income}\nTotal expenses: {expenses}"
        }
        (Locale::En, TextKey::StatsBtnHome) => "Home",
        (Locale::En, TextKey::TxVoidedSuffix) => " • void",
        (Locale::En, TextKey::TxKindExpense) => "Expense",
        (Locale::En, TextKey::TxKindIncome) => "Income",
        (Locale::En, TextKey::TxKindRefund) => "Refund",
        (Locale::En, TextKey::TxKindTransferWallet) => "Wallet transfer",
        (Locale::En, TextKey::TxKindTransferFlow) => "Budget transfer",
        (Locale::En, TextKey::WizardPromptExpense) => {
            "Send an expense, e.g.:\n\n12.50 coffee shop\n12.50 coffee #food\n\n(or type directly in chat without using the wizard)"
        }
        (Locale::En, TextKey::WizardPromptIncome) => {
            "Send an income, e.g.:\n\n1000 salary\n+1000 #salary paycheck\n\n(or type directly in chat without using the wizard)"
        }
        (Locale::En, TextKey::WizardPromptRefund) => {
            "Send a refund, e.g.:\n\nr 5.20 amazon\nr 5.20 #shopping amazon\n\n(or type directly in chat without using the wizard)"
        }
        (Locale::En, TextKey::WizardErrorEmpty) => "Empty text.",
        (Locale::En, TextKey::WizardErrorExpensePlus) => {
            "Selected: expense. Remove the '+' (e.g.: 12.50 coffee)."
        }
        (Locale::En, TextKey::WizardErrorExpenseRefund) => {
            "Selected: expense. For refund use the \"Refund\" button."
        }
        (Locale::En, TextKey::WizardErrorIncomeRefund) => {
            "Selected: income. Remove 'r' (e.g.: 1000 salary)."
        }
        (Locale::En, TextKey::UnknownUser) => "Cannot identify user.",
        (Locale::En, TextKey::PairingRequired) => "To pair: /start <code>",
        (Locale::En, TextKey::PairingPrompt) => "Enter the pairing code:",
        (Locale::En, TextKey::PreferencesSaveError) => "Error saving preferences.",
        (Locale::En, TextKey::DefaultWalletMissing) => "Please set a default wallet first.",
        (Locale::En, TextKey::TooManyTags) => "Too many tags: max 1.",
        (Locale::En, TextKey::InvalidAmountExample) => "Invalid amount (e.g.: 10 or 10.50).",
        (Locale::En, TextKey::InvalidAmountExampleShort) => "Invalid amount (e.g.: 10 or 10.50)",
        (Locale::En, TextKey::InvalidAmount) => "Invalid amount.",
        (Locale::En, TextKey::InvalidAmountPositive) => "Invalid amount (must be > 0).",
        (Locale::En, TextKey::TransactionVoided) => "✅ Entry voided.",
        (Locale::En, TextKey::EditAmountPrompt) => "Send the new amount (e.g.: 10.50):",
        (Locale::En, TextKey::EditNotePrompt) => "Send the new note (empty to remove):",
        (Locale::En, TextKey::EditAmountUpdated) => "✅ Amount updated.",
        (Locale::En, TextKey::EditNoteUpdated) => "✅ Note updated.",
        (Locale::En, TextKey::QuickAddSaved) => "✅ Saved: {amount}",
        (Locale::En, TextKey::AlreadySaved) => "✅ Already saved.",
        (Locale::En, TextKey::RepeatSuccess) => "✅ Repeated.",
        (Locale::En, TextKey::RepeatUnsupported) => "Repeat not supported for this type.",
        (Locale::En, TextKey::RepeatNoWallet) => "Transaction without wallet: cannot repeat.",
        (Locale::En, TextKey::QuickAddUndo) => "Undo",
        (Locale::En, TextKey::ApiNetworkError) => {
            "Connection problems with the server. Try again later!"
        }
        (Locale::En, TextKey::ApiMembershipLastOwner) => {
            "You cannot remove the last owner of the budget."
        }
        (Locale::En, TextKey::ApiMembershipOwnerImmutable) => {
            "You cannot change the vault owner's role."
        }
        (Locale::En, TextKey::ApiMembershipOwnerRemoveForbidden) => {
            "You cannot remove the vault owner."
        }
        (Locale::En, TextKey::ApiUnauthorized) => "Unauthorized. Use /start for pairing.",
        (Locale::En, TextKey::ApiForbidden) => "Operation not allowed.",
        (Locale::En, TextKey::ApiNotFound) => "Resource not found. Try resetting defaults.",
        (Locale::En, TextKey::ApiConflict) => "Duplicate request (already saved).",
        (Locale::En, TextKey::ApiBadRequestUserNotFound) => {
            "Invalid pairing code (or you're using a different database)."
        }
        (Locale::En, TextKey::ApiServerError) => "Server error.",
        (Locale::En, TextKey::MembersVaultTitle) => "Vault members",
        (Locale::En, TextKey::MembersFlowTitle) => "Budget \"{flow}\" members",
        (Locale::En, TextKey::MemberSaved) => "✅ Member saved: {username} ({role})",
        (Locale::En, TextKey::MemberRemoved) => "✅ Member removed: {username}",
        (Locale::En, TextKey::VaultDeleted) => "✅ Vault deleted.",
        (Locale::En, TextKey::CategorySourceMissing) => "Source category not found: {name}",
        (Locale::En, TextKey::CategoryDestinationMissing) => {
            "Destination category not found: {name}"
        }
        (Locale::En, TextKey::CategoryListHint) => "Use /categories to see the list.",
        (Locale::En, TextKey::MergeConfirmPrompt) => {
            "Ok, I can merge \"{from}\" -> \"{into}\".\nConfirm with:\n/merge_category confirm {from} -> {into}"
        }
        (Locale::En, TextKey::MergeCompleted) => "Merge completed: \"{from}\" -> \"{into}\".",
        (Locale::En, TextKey::CategoryListEmpty) => {
            "No categories. Add a transaction with #category to start."
        }
        (Locale::En, TextKey::CategoryListHeader) => "Categories:",
        (Locale::En, TextKey::CategoryFlagSystem) => " [system]",
        (Locale::En, TextKey::CategoryFlagArchived) => " [archived]",
        (Locale::En, TextKey::MembersEmpty) => "No members.",
        (Locale::En, TextKey::FlowNotFoundNone) => {
            "Budget \"{name}\" not found. No shareable budgets."
        }
        (Locale::En, TextKey::FlowNotFound) => "Budget \"{name}\" not found.",
        (Locale::En, TextKey::FlowAvailableHeader) => "Available budgets:",
        (Locale::En, TextKey::MergeConflictHeader) => {
            "Merge not possible: \"{from}\" -> \"{into}\"."
        }
        (Locale::En, TextKey::MergeConflictListHeader) => "Conflicts:",
        (Locale::En, TextKey::MergeConflictSame) => "Categories are identical.",
        (Locale::En, TextKey::MergeConflictSourceSystem) => {
            "Category \"{name}\" is a system category."
        }
        (Locale::En, TextKey::MergeConflictTargetArchived) => "Category \"{name}\" is archived.",
        (Locale::En, TextKey::MergeConflictAlias) => "Conflicting alias: {value}",
        (Locale::En, TextKey::MergeConflictName) => "Conflicting name: {value}",
        (Locale::En, TextKey::MergeConflictFallback) => "Conflict: {kind} ({value})",
        (Locale::En, TextKey::VaultDeleteHelp) => {
            "Usage:\n/vault_delete confirm\n\nWarning: deletes the Main vault and all data."
        }
        (Locale::En, TextKey::RoleOwner) => "owner",
        (Locale::En, TextKey::RoleEditor) => "editor",
        (Locale::En, TextKey::RoleViewer) => "viewer",
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
        assert_eq!(resolve_locale(Some("fr-FR")), default_locale());
        assert_eq!(resolve_locale(Some("de")), default_locale());
    }

    #[test]
    fn resolve_locale_accepts_english_prefix() {
        assert_eq!(resolve_locale(Some("en")), Locale::En);
        assert_eq!(resolve_locale(Some("en-US")), Locale::En);
        assert_eq!(resolve_locale(Some("en-GB")), Locale::En);
    }

    #[test]
    fn resolve_locale_accepts_italian_prefix() {
        assert_eq!(resolve_locale(Some("it")), Locale::It);
        assert_eq!(resolve_locale(Some("it-IT")), Locale::It);
    }
}
