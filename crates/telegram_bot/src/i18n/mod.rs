#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Locale {
    It,
    En,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextKey {
    // Welcome & Help
    WelcomeTemplate,
    HelpText,

    // Common
    UnsetValue,
    UnallocatedFlow,

    // Home
    HomeSummary,
    HomeBtnExpense,
    HomeBtnIncome,
    HomeBtnHistory,
    HomeBtnStats,

    // Pickers
    PickerWalletTitle,
    PickerFlowTitle,

    // Wizard
    WizardTitleExpense,
    WizardTitleIncome,
    WizardBodySimple,
    WizardBtnInput,
    WizardBtnWallet,
    WizardBtnFlow,
    WizardBtnHome,
    WizardPromptExpense,
    WizardPromptIncome,
    WizardErrorEmpty,

    // List
    ListHeader,
    ListPrev,
    ListNext,
    ListToggleVoided,
    ListStateOn,
    ListStateOff,
    ListBtnHome,

    // Detail
    DetailHeader,
    DetailBtnVoid,
    DetailBtnEdit,
    DetailBtnBack,
    EditMenuTitle,
    EditMenuAmount,
    EditMenuNote,

    // Stats
    StatsSummary,
    StatsCategoryHeader,
    StatsNoData,
    StatsBtnHome,

    // Transaction types
    TxVoidedSuffix,
    TxKindExpense,
    TxKindIncome,
    TxKindRefund,
    TxKindTransferWallet,
    TxKindTransferFlow,

    // Errors and prompts
    UnknownUser,
    PairingRequired,
    PairingPrompt,
    PreferencesSaveError,
    VaultPickerTitle,
    VaultPickerEmpty,
    VaultSetConfirmation,
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
    QuickAddUndo,

    // API errors
    ApiNetworkError,
    ApiUnauthorized,
    ApiForbidden,
    ApiNotFound,
    ApiConflict,
    ApiBadRequestUserNotFound,
    ApiServerError,

    // Categories
    CategoryListEmpty,
    CategoryListHeader,

    // Onboarding
    PairingSuccess,
    WelcomeFirstTime,
    ConceptsExplanation,
    QuickStartGuide,

    // Contextual Help
    HelpTextHome,
    HelpTextWizard,
    HelpTextList,
    HelpTextStats,
    HelpFooter,
    HomeBtnHelp,

    // Feedback
    ErrorRecoveryHint,

    // Navigation
    ListPageNumber,
    NavBreadcrumbList,
    NavBreadcrumbDetail,
    NavBreadcrumbWizard,
    NavBreadcrumbStats,

    // Void Confirmation
    VoidConfirmTitle,
    VoidConfirmBody,
    VoidConfirmYes,
    VoidConfirmNo,

    // Repeat Transaction
    DetailBtnRepeat,
    RepeatSuccess,

    // Export
    ExportGenerating,
    ExportReady,
    ExportEmpty,

    // Filters
    ListBtnFilter,
    FilterTitle,
    FilterKindAll,
    FilterKindExpense,
    FilterKindIncome,
    FilterActiveIndicator,
    FilterClear,
    FilterBtnBack,

    // Templates
    TemplateListTitle,
    TemplateEmpty,
    TemplateBtnUse,
    TemplateBtnDelete,
    TemplateBtnCreate,
    TemplateBtnHome,
    TemplateCreatePrompt,
    TemplateCreated,
    TemplateUsed,
    TemplateDeleted,
    TemplateMaxReached,
    TemplateInvalid,
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
        // ============ ITALIAN ============
        (Locale::It, TextKey::WelcomeTemplate) => {
            "Benvenuto, {display_name}!\n\nPuoi inserire voci al volo scrivendo:\n\n12.50 bar caffè\n+1000 stipendio\n\n#tag opzionale: 12.50 #food caffè"
        }
        (Locale::It, TextKey::HelpText) => {
            "Sintassi quick add:\n\n12.50 bar caffè → Spesa\n+1000 stipendio → Entrata\n\n#tag opzionale (max 1):\n12.50 #food caffè\n\nComandi:\n/home - Torna alla home\n/help - Mostra aiuto\n/categories - Lista categorie\n/template - Gestisci template\n/export - Esporta CSV\n/vault - Cambia vault"
        }
        (Locale::It, TextKey::UnsetValue) => "Non impostato",
        (Locale::It, TextKey::UnallocatedFlow) => "Non allocato",
        (Locale::It, TextKey::HomeSummary) => {
            "👋 Ciao {display_name}!\n\n🏦 {vault}\n👛 Wallet: {wallet}\n🎯 Budget: {flow}\n💰 Saldo: {balance}"
        }
        (Locale::It, TextKey::HomeBtnExpense) => "Spesa",
        (Locale::It, TextKey::HomeBtnIncome) => "Entrata",
        (Locale::It, TextKey::HomeBtnHistory) => "Cronologia",
        (Locale::It, TextKey::HomeBtnStats) => "Stats",
        (Locale::It, TextKey::PickerWalletTitle) => "Seleziona wallet:",
        (Locale::It, TextKey::PickerFlowTitle) => "Seleziona budget:",
        (Locale::It, TextKey::WizardTitleExpense) => "Nuova Spesa",
        (Locale::It, TextKey::WizardTitleIncome) => "Nuova Entrata",
        (Locale::It, TextKey::WizardBodySimple) => {
            "👛 Wallet: {wallet}\n🎯 Budget: {flow}\n\nInserisci: importo [#categoria] [nota]\nEs: 12.50 #cibo caffè"
        }
        (Locale::It, TextKey::WizardBtnInput) => "Inserisci",
        (Locale::It, TextKey::WizardBtnWallet) => "Wallet",
        (Locale::It, TextKey::WizardBtnFlow) => "Budget",
        (Locale::It, TextKey::WizardBtnHome) => "Home",
        (Locale::It, TextKey::WizardPromptExpense) => {
            "Invia una spesa:\n\n12.50 caffè\n12.50 #cibo caffè"
        }
        (Locale::It, TextKey::WizardPromptIncome) => {
            "Invia un'entrata:\n\n+1000 stipendio\n+1000 #stipendio mensile"
        }
        (Locale::It, TextKey::WizardErrorEmpty) => "Testo vuoto.",
        (Locale::It, TextKey::ListHeader) => "Ultime transazioni:",
        (Locale::It, TextKey::ListPrev) => "Prec",
        (Locale::It, TextKey::ListNext) => "Succ",
        (Locale::It, TextKey::ListToggleVoided) => "Mostra annullate: {state}",
        (Locale::It, TextKey::ListStateOn) => "Sì",
        (Locale::It, TextKey::ListStateOff) => "No",
        (Locale::It, TextKey::ListBtnHome) => "Home",
        (Locale::It, TextKey::DetailHeader) => {
            "📋 Dettaglio\n\n📌 Tipo: {kind}\n📅 Data: {when}\n💶 Importo: {amount}\n🏷 Categoria: {category}\n📝 Nota: {note}"
        }
        (Locale::It, TextKey::DetailBtnVoid) => "Annulla",
        (Locale::It, TextKey::DetailBtnEdit) => "Modifica",
        (Locale::It, TextKey::DetailBtnBack) => "Indietro",
        (Locale::It, TextKey::EditMenuTitle) => "Cosa vuoi modificare?",
        (Locale::It, TextKey::EditMenuAmount) => "Importo",
        (Locale::It, TextKey::EditMenuNote) => "Nota",
        (Locale::It, TextKey::StatsSummary) => {
            "📊 Stats - {month}\n\n💰 Saldo: {balance}\n📈 Entrate: {income}\n📉 Uscite: {expenses}"
        }
        (Locale::It, TextKey::StatsCategoryHeader) => "\nPer categoria:",
        (Locale::It, TextKey::StatsNoData) => "Nessuna transazione questo mese.",
        (Locale::It, TextKey::StatsBtnHome) => "Home",
        (Locale::It, TextKey::TxVoidedSuffix) => " • annullata",
        (Locale::It, TextKey::TxKindExpense) => "Spesa",
        (Locale::It, TextKey::TxKindIncome) => "Entrata",
        (Locale::It, TextKey::TxKindRefund) => "Rimborso",
        (Locale::It, TextKey::TxKindTransferWallet) => "Trasf. wallet",
        (Locale::It, TextKey::TxKindTransferFlow) => "Trasf. budget",
        (Locale::It, TextKey::UnknownUser) => "Impossibile identificare l'utente.",
        (Locale::It, TextKey::PairingRequired) => "Per fare pairing: /start <codice>",
        (Locale::It, TextKey::PairingPrompt) => "Inserisci il codice di pairing:",
        (Locale::It, TextKey::PreferencesSaveError) => "Errore nel salvataggio delle preferenze.",
        (Locale::It, TextKey::VaultPickerTitle) => "Scegli un vault:",
        (Locale::It, TextKey::VaultPickerEmpty) => "Nessun vault disponibile.",
        (Locale::It, TextKey::VaultSetConfirmation) => "✅ Vault attivo: {vault}",
        (Locale::It, TextKey::DefaultWalletMissing) => "Imposta prima un wallet di default.",
        (Locale::It, TextKey::TooManyTags) => "Troppi tag: massimo 1.",
        (Locale::It, TextKey::InvalidAmountExample) => "Importo non valido (es: 10 o 10.50).",
        (Locale::It, TextKey::InvalidAmountExampleShort) => "Importo non valido (es: 10 o 10.50)",
        (Locale::It, TextKey::InvalidAmount) => "Importo non valido.",
        (Locale::It, TextKey::InvalidAmountPositive) => "Importo non valido (deve essere > 0).",
        (Locale::It, TextKey::TransactionVoided) => "✅ Transazione annullata.",
        (Locale::It, TextKey::EditAmountPrompt) => "Invia il nuovo importo (es: 10.50):",
        (Locale::It, TextKey::EditNotePrompt) => "Invia la nuova nota (vuoto per rimuovere):",
        (Locale::It, TextKey::EditAmountUpdated) => "✅ Importo aggiornato.",
        (Locale::It, TextKey::EditNoteUpdated) => "✅ Nota aggiornata.",
        (Locale::It, TextKey::QuickAddSaved) => "✅ Salvato: {amount}",
        (Locale::It, TextKey::AlreadySaved) => "✅ Già salvato.",
        (Locale::It, TextKey::QuickAddUndo) => "Annulla",
        (Locale::It, TextKey::ApiNetworkError) => "Problemi di connessione. Riprova più tardi!",
        (Locale::It, TextKey::ApiUnauthorized) => {
            "Non autorizzato. Usa /start per fare il pairing."
        }
        (Locale::It, TextKey::ApiForbidden) => "Operazione non permessa.",
        (Locale::It, TextKey::ApiNotFound) => "Risorsa non trovata. Prova a reimpostare i default.",
        (Locale::It, TextKey::ApiConflict) => "Richiesta duplicata (già salvata).",
        (Locale::It, TextKey::ApiBadRequestUserNotFound) => "Codice di pairing non valido.",
        (Locale::It, TextKey::ApiServerError) => "Errore server.",
        (Locale::It, TextKey::CategoryListEmpty) => {
            "Nessuna categoria. Aggiungi una transazione con #categoria per iniziare."
        }
        (Locale::It, TextKey::CategoryListHeader) => "Categorie:",
        (Locale::It, TextKey::PairingSuccess) => {
            "✅ Pairing completato!\n\nOra puoi iniziare a tracciare le tue spese."
        }
        (Locale::It, TextKey::WelcomeFirstTime) => {
            "🎉 Benvenuto su Sparagne, {display_name}!\n\n📱 Il tuo tracker di spese personale."
        }
        (Locale::It, TextKey::ConceptsExplanation) => {
            "💡 Concetti base:\n\n👛 Wallet - Dove tieni i soldi (carta, contanti...)\n🎯 Budget - Come organizzi le spese (cibo, trasporti...)\n🏷 Categoria - Tag per classificare (#cibo, #bar...)"
        }
        (Locale::It, TextKey::QuickStartGuide) => {
            "🚀 Per iniziare:\n\n• Scrivi: 12.50 caffè\n• Oppure: +1000 stipendio\n• Usa i pulsanti qui sotto"
        }
        (Locale::It, TextKey::HelpTextHome) => {
            "📚 Aiuto - Home\n\nDa qui puoi:\n• Aggiungere spese/entrate\n• Vedere la cronologia\n• Consultare le statistiche\n\n💡 Tip: Scrivi direttamente importo e nota!\nEs: 12.50 caffè"
        }
        (Locale::It, TextKey::HelpTextWizard) => {
            "📚 Aiuto - Inserimento\n\nFormato: importo [#categoria] [nota]\n\nEsempi:\n• 12.50 caffè\n• 12.50 #cibo caffè al bar\n• +500 stipendio"
        }
        (Locale::It, TextKey::HelpTextList) => {
            "📚 Aiuto - Cronologia\n\nQui vedi le ultime transazioni.\n\n• Tocca una voce per i dettagli\n• Puoi modificare o annullare\n• Usa i pulsanti per navigare"
        }
        (Locale::It, TextKey::HelpTextStats) => {
            "📚 Aiuto - Statistiche\n\nRiepilogo del mese corrente:\n• Entrate e uscite totali\n• Saldo netto\n• Spese per categoria"
        }
        (Locale::It, TextKey::HelpFooter) => {
            "\n\n📋 Comandi:\n/home - Torna alla home\n/help - Questo aiuto\n/categories - Lista categorie\n/template - Gestisci template\n/export - Esporta CSV\n/vault - Cambia vault"
        }
        (Locale::It, TextKey::HomeBtnHelp) => "Aiuto",
        (Locale::It, TextKey::ErrorRecoveryHint) => "\n\n💡 Prova: /home per tornare alla home",
        (Locale::It, TextKey::ListPageNumber) => "Pagina {page}",
        (Locale::It, TextKey::NavBreadcrumbList) => "🏠 › 📜 Cronologia",
        (Locale::It, TextKey::NavBreadcrumbDetail) => "🏠 › 📜 › 📋 Dettaglio",
        (Locale::It, TextKey::NavBreadcrumbWizard) => "🏠 › ✏️ {type}",
        (Locale::It, TextKey::NavBreadcrumbStats) => "🏠 › 📊 Statistiche",
        (Locale::It, TextKey::VoidConfirmTitle) => "⚠️ Conferma annullamento",
        (Locale::It, TextKey::VoidConfirmBody) => {
            "Vuoi annullare questa transazione?\n\n{amount} • {note}"
        }
        (Locale::It, TextKey::VoidConfirmYes) => "Sì, annulla",
        (Locale::It, TextKey::VoidConfirmNo) => "No, torna indietro",
        (Locale::It, TextKey::DetailBtnRepeat) => "Ripeti",
        (Locale::It, TextKey::RepeatSuccess) => "✅ Transazione ripetuta",
        (Locale::It, TextKey::ExportGenerating) => "⏳ Generazione export in corso...",
        (Locale::It, TextKey::ExportReady) => "📁 Ecco il tuo export",
        (Locale::It, TextKey::ExportEmpty) => "Nessuna transazione da esportare.",
        (Locale::It, TextKey::ListBtnFilter) => "Filtri",
        (Locale::It, TextKey::FilterTitle) => "🔍 Filtri",
        (Locale::It, TextKey::FilterKindAll) => "Tutti",
        (Locale::It, TextKey::FilterKindExpense) => "Solo spese",
        (Locale::It, TextKey::FilterKindIncome) => "Solo entrate",
        (Locale::It, TextKey::FilterActiveIndicator) => "🔍 Filtro attivo",
        (Locale::It, TextKey::FilterClear) => "Rimuovi filtri",
        (Locale::It, TextKey::FilterBtnBack) => "Indietro",
        (Locale::It, TextKey::TemplateListTitle) => "📋 Template salvati",
        (Locale::It, TextKey::TemplateEmpty) => {
            "Nessun template salvato.\n\nI template ti permettono di salvare transazioni frequenti per riutilizzarle velocemente."
        }
        (Locale::It, TextKey::TemplateBtnUse) => "Usa",
        (Locale::It, TextKey::TemplateBtnDelete) => "Elimina",
        (Locale::It, TextKey::TemplateBtnCreate) => "Nuovo template",
        (Locale::It, TextKey::TemplateBtnHome) => "Home",
        (Locale::It, TextKey::TemplateCreatePrompt) => {
            "Invia il template nel formato:\n\nnome | importo [#categoria] [nota]\n\nEs: Caffè | 1.50 #bar caffè"
        }
        (Locale::It, TextKey::TemplateCreated) => "✅ Template salvato",
        (Locale::It, TextKey::TemplateUsed) => "✅ Transazione creata da template",
        (Locale::It, TextKey::TemplateDeleted) => "✅ Template eliminato",
        (Locale::It, TextKey::TemplateMaxReached) => {
            "Hai raggiunto il limite massimo di template (10)."
        }
        (Locale::It, TextKey::TemplateInvalid) => {
            "Formato non valido. Usa: nome | importo [#categoria] [nota]\nEs: Caffè | 1.50 #bar caffè"
        }

        // ============ ENGLISH ============
        (Locale::En, TextKey::WelcomeTemplate) => {
            "Welcome, {display_name}!\n\nYou can add entries on the fly:\n\n12.50 coffee\n+1000 salary\n\nOptional #tag: 12.50 #food coffee"
        }
        (Locale::En, TextKey::HelpText) => {
            "Quick add syntax:\n\n12.50 coffee → Expense\n+1000 salary → Income\n\nOptional #tag (max 1):\n12.50 #food coffee\n\nCommands:\n/home - Go to home\n/help - Show help\n/categories - List categories\n/template - Manage templates\n/export - Export CSV\n/vault - Switch vault"
        }
        (Locale::En, TextKey::UnsetValue) => "Not set",
        (Locale::En, TextKey::UnallocatedFlow) => "Unallocated",
        (Locale::En, TextKey::HomeSummary) => {
            "👋 Hi {display_name}!\n\n🏦 {vault}\n👛 Wallet: {wallet}\n🎯 Budget: {flow}\n💰 Balance: {balance}"
        }
        (Locale::En, TextKey::HomeBtnExpense) => "Expense",
        (Locale::En, TextKey::HomeBtnIncome) => "Income",
        (Locale::En, TextKey::HomeBtnHistory) => "History",
        (Locale::En, TextKey::HomeBtnStats) => "Stats",
        (Locale::En, TextKey::PickerWalletTitle) => "Select wallet:",
        (Locale::En, TextKey::PickerFlowTitle) => "Select budget:",
        (Locale::En, TextKey::WizardTitleExpense) => "New Expense",
        (Locale::En, TextKey::WizardTitleIncome) => "New Income",
        (Locale::En, TextKey::WizardBodySimple) => {
            "👛 Wallet: {wallet}\n🎯 Budget: {flow}\n\nEnter: amount [#category] [note]\nEx: 12.50 #food coffee"
        }
        (Locale::En, TextKey::WizardBtnInput) => "Enter",
        (Locale::En, TextKey::WizardBtnWallet) => "Wallet",
        (Locale::En, TextKey::WizardBtnFlow) => "Budget",
        (Locale::En, TextKey::WizardBtnHome) => "Home",
        (Locale::En, TextKey::WizardPromptExpense) => {
            "Send an expense:\n\n12.50 coffee\n12.50 #food coffee"
        }
        (Locale::En, TextKey::WizardPromptIncome) => {
            "Send an income:\n\n+1000 salary\n+1000 #salary monthly"
        }
        (Locale::En, TextKey::WizardErrorEmpty) => "Empty text.",
        (Locale::En, TextKey::ListHeader) => "Recent transactions:",
        (Locale::En, TextKey::ListPrev) => "Prev",
        (Locale::En, TextKey::ListNext) => "Next",
        (Locale::En, TextKey::ListToggleVoided) => "Show voided: {state}",
        (Locale::En, TextKey::ListStateOn) => "Yes",
        (Locale::En, TextKey::ListStateOff) => "No",
        (Locale::En, TextKey::ListBtnHome) => "Home",
        (Locale::En, TextKey::DetailHeader) => {
            "📋 Detail\n\n📌 Type: {kind}\n📅 Date: {when}\n💶 Amount: {amount}\n🏷 Category: {category}\n📝 Note: {note}"
        }
        (Locale::En, TextKey::DetailBtnVoid) => "Void",
        (Locale::En, TextKey::DetailBtnEdit) => "Edit",
        (Locale::En, TextKey::DetailBtnBack) => "Back",
        (Locale::En, TextKey::EditMenuTitle) => "What do you want to edit?",
        (Locale::En, TextKey::EditMenuAmount) => "Amount",
        (Locale::En, TextKey::EditMenuNote) => "Note",
        (Locale::En, TextKey::StatsSummary) => {
            "📊 Stats - {month}\n\n💰 Balance: {balance}\n📈 Income: {income}\n📉 Expenses: {expenses}"
        }
        (Locale::En, TextKey::StatsCategoryHeader) => "\nBy category:",
        (Locale::En, TextKey::StatsNoData) => "No transactions this month.",
        (Locale::En, TextKey::StatsBtnHome) => "Home",
        (Locale::En, TextKey::TxVoidedSuffix) => " • voided",
        (Locale::En, TextKey::TxKindExpense) => "Expense",
        (Locale::En, TextKey::TxKindIncome) => "Income",
        (Locale::En, TextKey::TxKindRefund) => "Refund",
        (Locale::En, TextKey::TxKindTransferWallet) => "Wallet transfer",
        (Locale::En, TextKey::TxKindTransferFlow) => "Budget transfer",
        (Locale::En, TextKey::UnknownUser) => "Cannot identify user.",
        (Locale::En, TextKey::PairingRequired) => "To pair: /start <code>",
        (Locale::En, TextKey::PairingPrompt) => "Enter the pairing code:",
        (Locale::En, TextKey::PreferencesSaveError) => "Error saving preferences.",
        (Locale::En, TextKey::VaultPickerTitle) => "Choose a vault:",
        (Locale::En, TextKey::VaultPickerEmpty) => "No vaults available.",
        (Locale::En, TextKey::VaultSetConfirmation) => "✅ Active vault: {vault}",
        (Locale::En, TextKey::DefaultWalletMissing) => "Please set a default wallet first.",
        (Locale::En, TextKey::TooManyTags) => "Too many tags: max 1.",
        (Locale::En, TextKey::InvalidAmountExample) => "Invalid amount (e.g.: 10 or 10.50).",
        (Locale::En, TextKey::InvalidAmountExampleShort) => "Invalid amount (e.g.: 10 or 10.50)",
        (Locale::En, TextKey::InvalidAmount) => "Invalid amount.",
        (Locale::En, TextKey::InvalidAmountPositive) => "Invalid amount (must be > 0).",
        (Locale::En, TextKey::TransactionVoided) => "✅ Transaction voided.",
        (Locale::En, TextKey::EditAmountPrompt) => "Send the new amount (e.g.: 10.50):",
        (Locale::En, TextKey::EditNotePrompt) => "Send the new note (empty to remove):",
        (Locale::En, TextKey::EditAmountUpdated) => "✅ Amount updated.",
        (Locale::En, TextKey::EditNoteUpdated) => "✅ Note updated.",
        (Locale::En, TextKey::QuickAddSaved) => "✅ Saved: {amount}",
        (Locale::En, TextKey::AlreadySaved) => "✅ Already saved.",
        (Locale::En, TextKey::QuickAddUndo) => "Undo",
        (Locale::En, TextKey::ApiNetworkError) => "Connection problems. Try again later!",
        (Locale::En, TextKey::ApiUnauthorized) => "Unauthorized. Use /start for pairing.",
        (Locale::En, TextKey::ApiForbidden) => "Operation not allowed.",
        (Locale::En, TextKey::ApiNotFound) => "Resource not found. Try resetting defaults.",
        (Locale::En, TextKey::ApiConflict) => "Duplicate request (already saved).",
        (Locale::En, TextKey::ApiBadRequestUserNotFound) => "Invalid pairing code.",
        (Locale::En, TextKey::ApiServerError) => "Server error.",
        (Locale::En, TextKey::CategoryListEmpty) => {
            "No categories. Add a transaction with #category to start."
        }
        (Locale::En, TextKey::CategoryListHeader) => "Categories:",
        (Locale::En, TextKey::PairingSuccess) => {
            "✅ Pairing complete!\n\nYou can now start tracking your expenses."
        }
        (Locale::En, TextKey::WelcomeFirstTime) => {
            "🎉 Welcome to Sparagne, {display_name}!\n\n📱 Your personal expense tracker."
        }
        (Locale::En, TextKey::ConceptsExplanation) => {
            "💡 Basic concepts:\n\n👛 Wallet - Where you keep money (card, cash...)\n🎯 Budget - How you organize expenses (food, transport...)\n🏷 Category - Tags to classify (#food, #bar...)"
        }
        (Locale::En, TextKey::QuickStartGuide) => {
            "🚀 To get started:\n\n• Type: 12.50 coffee\n• Or: +1000 salary\n• Use the buttons below"
        }
        (Locale::En, TextKey::HelpTextHome) => {
            "📚 Help - Home\n\nFrom here you can:\n• Add expenses/income\n• View transaction history\n• Check statistics\n\n💡 Tip: Type amount and note directly!\nEx: 12.50 coffee"
        }
        (Locale::En, TextKey::HelpTextWizard) => {
            "📚 Help - Entry\n\nFormat: amount [#category] [note]\n\nExamples:\n• 12.50 coffee\n• 12.50 #food coffee at bar\n• +500 salary"
        }
        (Locale::En, TextKey::HelpTextList) => {
            "📚 Help - History\n\nHere you see recent transactions.\n\n• Tap an entry for details\n• You can edit or void\n• Use buttons to navigate"
        }
        (Locale::En, TextKey::HelpTextStats) => {
            "📚 Help - Statistics\n\nCurrent month summary:\n• Total income and expenses\n• Net balance\n• Expenses by category"
        }
        (Locale::En, TextKey::HelpFooter) => {
            "\n\n📋 Commands:\n/home - Back to home\n/help - This help\n/categories - List categories\n/template - Manage templates\n/export - Export CSV\n/vault - Switch vault"
        }
        (Locale::En, TextKey::HomeBtnHelp) => "Help",
        (Locale::En, TextKey::ErrorRecoveryHint) => "\n\n💡 Try: /home to go back home",
        (Locale::En, TextKey::ListPageNumber) => "Page {page}",
        (Locale::En, TextKey::NavBreadcrumbList) => "🏠 › 📜 History",
        (Locale::En, TextKey::NavBreadcrumbDetail) => "🏠 › 📜 › 📋 Detail",
        (Locale::En, TextKey::NavBreadcrumbWizard) => "🏠 › ✏️ {type}",
        (Locale::En, TextKey::NavBreadcrumbStats) => "🏠 › 📊 Statistics",
        (Locale::En, TextKey::VoidConfirmTitle) => "⚠️ Confirm void",
        (Locale::En, TextKey::VoidConfirmBody) => {
            "Do you want to void this transaction?\n\n{amount} • {note}"
        }
        (Locale::En, TextKey::VoidConfirmYes) => "Yes, void it",
        (Locale::En, TextKey::VoidConfirmNo) => "No, go back",
        (Locale::En, TextKey::DetailBtnRepeat) => "Repeat",
        (Locale::En, TextKey::RepeatSuccess) => "✅ Transaction repeated",
        (Locale::En, TextKey::ExportGenerating) => "⏳ Generating export...",
        (Locale::En, TextKey::ExportReady) => "📁 Here's your export",
        (Locale::En, TextKey::ExportEmpty) => "No transactions to export.",
        (Locale::En, TextKey::ListBtnFilter) => "Filters",
        (Locale::En, TextKey::FilterTitle) => "🔍 Filters",
        (Locale::En, TextKey::FilterKindAll) => "All",
        (Locale::En, TextKey::FilterKindExpense) => "Expenses only",
        (Locale::En, TextKey::FilterKindIncome) => "Income only",
        (Locale::En, TextKey::FilterActiveIndicator) => "🔍 Filter active",
        (Locale::En, TextKey::FilterClear) => "Clear filters",
        (Locale::En, TextKey::FilterBtnBack) => "Back",
        (Locale::En, TextKey::TemplateListTitle) => "📋 Saved templates",
        (Locale::En, TextKey::TemplateEmpty) => {
            "No saved templates.\n\nTemplates let you save frequent transactions for quick reuse."
        }
        (Locale::En, TextKey::TemplateBtnUse) => "Use",
        (Locale::En, TextKey::TemplateBtnDelete) => "Delete",
        (Locale::En, TextKey::TemplateBtnCreate) => "New template",
        (Locale::En, TextKey::TemplateBtnHome) => "Home",
        (Locale::En, TextKey::TemplateCreatePrompt) => {
            "Send the template in format:\n\nname | amount [#category] [note]\n\nEx: Coffee | 1.50 #bar coffee"
        }
        (Locale::En, TextKey::TemplateCreated) => "✅ Template saved",
        (Locale::En, TextKey::TemplateUsed) => "✅ Transaction created from template",
        (Locale::En, TextKey::TemplateDeleted) => "✅ Template deleted",
        (Locale::En, TextKey::TemplateMaxReached) => {
            "You've reached the maximum number of templates (10)."
        }
        (Locale::En, TextKey::TemplateInvalid) => {
            "Invalid format. Use: name | amount [#category] [note]\nEx: Coffee | 1.50 #bar coffee"
        }
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
