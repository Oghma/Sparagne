use super::TextKey;

/// Returns the Italian translation for a text key.
#[must_use]
pub fn get(key: TextKey) -> &'static str {
    match key {
        // Sections
        TextKey::SectionHome => "Home",
        TextKey::SectionTransactions => "Transazioni",
        TextKey::SectionWallets => "Portafogli",
        TextKey::SectionFlows => "Flussi",
        TextKey::SectionCategories => "Categorie",
        TextKey::SectionMembers => "Membri",
        TextKey::SectionVault => "Vault",
        TextKey::SectionPreferences => "Preferenze",
        TextKey::SectionAccounts => "Conti",
        TextKey::SectionAnalytics => "Statistiche",
        TextKey::SectionSettings => "Impostazioni",

        // Actions
        TextKey::ActionSave => "Salva",

        // Hints - Footer Navigation
        TextKey::HintHome => "home",
        TextKey::HintTransactions => "txn",
        TextKey::HintAccounts => "conti",
        TextKey::HintAnalytics => "statistiche",
        TextKey::HintSettings => "impostazioni",

        // Hints - Footer Actions
        TextKey::HintQuickAdd => "aggiungi",
        TextKey::HintHelp => "aiuto",
        TextKey::HintCreate => "crea",
        TextKey::HintEdit => "modifica",
        TextKey::HintDelete => "elimina",
        TextKey::HintSave => "salva",
        TextKey::HintCancel => "annulla",
        TextKey::HintBack => "indietro",
        TextKey::HintRefresh => "aggiorna",
        TextKey::HintAdd => "aggiungi",
        TextKey::HintToggle => "seleziona",
        TextKey::HintCategorize => "categorizza",
        TextKey::HintExit => "esci",

        // Help Overlay
        TextKey::HelpTitle => "Scorciatoie da Tastiera",
        TextKey::HelpCloseHelp => "chiudi aiuto",
        TextKey::HelpGlobal => "Globale",
        TextKey::HelpNavigation => "Navigazione",
        TextKey::HelpCommonActions => "Azioni Comuni",
        TextKey::HelpQuickAddTxn => "Aggiungi transazione veloce",
        TextKey::HelpNewTxnModal => "Nuova transazione (modale)",
        TextKey::HelpCommandPalette => "Palette comandi",
        TextKey::HelpSearch => "Cerca",
        TextKey::HelpShowHelp => "Mostra/nascondi aiuto",
        TextKey::HelpNextSubTab => "Tab successivo",
        TextKey::HelpPrevSubTab => "Tab precedente",
        TextKey::HelpNavigateList => "Naviga lista",
        TextKey::HelpOpenDetails => "Apri dettagli",
        TextKey::HelpBackClose => "Indietro / Chiudi",
        TextKey::HelpEditSelected => "Modifica selezionato",
        TextKey::HelpDeleteSelected => "Elimina selezionato",
        TextKey::HelpNavigateFeed => "Naviga feed",
        TextKey::HelpGoToTransactions => "Vai a Transazioni",
        TextKey::HelpGoToAccounts => "Vai a Conti",
        TextKey::HelpGoToAnalytics => "Vai a Statistiche",
        TextKey::HelpGoToSettings => "Vai a Impostazioni",
        TextKey::HelpNewIncome => "Nuova entrata",
        TextKey::HelpNewRefund => "Nuovo rimborso",
        TextKey::HelpNewTransfer => "Nuovo trasferimento",
        TextKey::HelpToggleFilters => "Mostra/nascondi filtri",
        TextKey::HelpGroupTxns => "Raggruppa transazioni",
        TextKey::HelpScopeWallet => "Filtra per portafoglio",
        TextKey::HelpScopeFlow => "Filtra per flusso",
        TextKey::HelpClearFilters => "Rimuovi filtri",
        TextKey::HelpDeleteTxn => "Elimina transazione",
        TextKey::HelpUndoDelete => "Annulla eliminazione (se visibile)",
        TextKey::HelpToggleVoided => "Mostra/nascondi annullate",
        TextKey::HelpNextPrevPage => "Pagina succ./prec.",
        TextKey::HelpVisualMode => "Modalita' Visuale",
        TextKey::HelpToggleVisual => "Attiva modalita' visuale",
        TextKey::HelpSelectTxn => "Seleziona transazione",
        TextKey::HelpExitVisual => "Esci modalita' visuale",
        TextKey::HelpDetailView => "Vista Dettaglio",
        TextKey::HelpEditTxn => "Modifica transazione",
        TextKey::HelpRepeatTxn => "Ripeti transazione",
        TextKey::HelpVoidTxn => "Annulla transazione",
        TextKey::HelpForm => "Modulo",
        TextKey::HelpNextField => "Campo successivo",
        TextKey::HelpChangeValue => "Cambia valore",
        TextKey::HelpFilters => "Filtri",
        TextKey::HelpToggleType => "Cambia tipo",
        TextKey::HelpToggleScope => "Cambia ambito",
        TextKey::HelpApply => "Applica",
        TextKey::HelpJumpSubTab => "Vai a sub-tab",
        TextKey::HelpSourcesWallets => "Fonti (Portafogli)",
        TextKey::HelpCreateWallet => "Crea portafoglio",
        TextKey::HelpRenameWallet => "Rinomina portafoglio",
        TextKey::HelpDeleteArchive => "Elimina (archivia)",
        TextKey::HelpViewDetails => "Visualizza dettagli",
        TextKey::HelpEnvelopesFlows => "Buste (Flussi)",
        TextKey::HelpCreateEnvelope => "Crea busta",
        TextKey::HelpRenameEnvelope => "Rinomina busta",
        TextKey::HelpChangeMode => "Cambia modalita'",
        TextKey::HelpRefreshData => "Aggiorna dati",
        TextKey::HelpSwitchView => "Cambia vista",
        TextKey::HelpSwitchPanel => "Cambia pannello",
        TextKey::HelpCashSpendWorth => "Cassa/Spese/Patrimonio",
        TextKey::HelpChangePeriod => "Cambia periodo",
        TextKey::HelpCreateCategory => "Crea categoria",
        TextKey::HelpRenameCategory => "Rinomina categoria",
        TextKey::HelpManageAliases => "Gestisci alias",
        TextKey::HelpMergeCategories => "Unisci categorie",
        TextKey::HelpAliases => "Alias",
        TextKey::HelpSwitchFocus => "Cambia focus",
        TextKey::HelpDeleteAlias => "Elimina alias",
        TextKey::HelpAddSave => "Aggiungi/Salva",
        TextKey::HelpVault => "Vault",
        TextKey::HelpCreateVault => "Crea vault",
        TextKey::HelpSelectVault => "Seleziona vault",
        TextKey::HelpMembers => "Membri",
        TextKey::HelpAddMember => "Aggiungi membro",
        TextKey::HelpEditMember => "Modifica membro",
        TextKey::HelpRemoveMember => "Rimuovi membro",
        TextKey::HelpVaultMembers => "Membri vault",
        TextKey::HelpFlowSharing => "Condivisione flusso",
        TextKey::HelpChangeFlow => "Cambia flusso",
        TextKey::HelpChangeRole => "Cambia ruolo",

        // Status bar
        TextKey::StatusOnline => "online",
        TextKey::StatusOffline => "offline",

        // Success Messages
        TextKey::SuccessTransactionSaved => "Transazione salvata.",
        TextKey::SuccessTransactionUpdated => "Transazione aggiornata.",
        TextKey::SuccessTransactionVoided => "Transazione annullata.",
        TextKey::SuccessTransactionRepeated => "Transazione ripetuta.",
        TextKey::SuccessTransferWalletSaved => "Transfer wallet salvato.",
        TextKey::SuccessTransferWalletUpdated => "Transfer wallet aggiornato.",
        TextKey::SuccessTransferFlowSaved => "Transfer flow salvato.",
        TextKey::SuccessTransferFlowUpdated => "Transfer flow aggiornato.",
        TextKey::SuccessWalletCreated => "Wallet creato.",
        TextKey::SuccessWalletUpdated => "Wallet aggiornato.",
        TextKey::SuccessWalletRestored => "Wallet ripristinato.",
        TextKey::SuccessFlowCreated => "Flow creato.",
        TextKey::SuccessFlowUpdated => "Flow aggiornato.",
        TextKey::SuccessFlowRestored => "Flow ripristinato.",
        TextKey::SuccessCategoryCreated => "Categoria creata.",
        TextKey::SuccessCategoryUpdated => "Categoria aggiornata.",
        TextKey::SuccessAliasCreated => "Alias creato.",
        TextKey::SuccessAliasDeleted => "Alias eliminato.",
        TextKey::SuccessVaultCreated => "Vault creato.",
        TextKey::SuccessVaultSelected => "Vault selected.",
        TextKey::SuccessDefaultsSaved => "Default salvati.",
        TextKey::SuccessMergeCompleted => "Merge completato.",
        TextKey::SuccessMergePreviewOk => "Preview ok. Premi Enter per confermare.",
        TextKey::SuccessDeletedItem => "Eliminato \"{label}\" ({amount})",
        TextKey::SuccessDeletedMultiple => "Eliminate {count} transazioni",
        TextKey::SuccessCategorizedTransactions => {
            "Categorizzate {count} transazioni come #{category}"
        }
        TextKey::SuccessDeletedWallet => "Eliminato \"{name}\"",
        TextKey::SuccessDeletedFlow => "Eliminato \"{name}\"",

        // Error Messages
        TextKey::ErrorSaving => "Errore salvataggio.",
        TextKey::ErrorUpdating => "Errore aggiornamento.",
        TextKey::ErrorVoiding => "Errore durante l'annullamento.",
        TextKey::ErrorRepeating => "Errore durante la ripetizione.",
        TextKey::ErrorConnection => "Errore connessione",
        TextKey::ErrorTransferWallet => "Errore transfer wallet.",
        TextKey::ErrorTransferFlow => "Errore transfer flow.",
        TextKey::ErrorCreateWallet => "Errore creazione wallet.",
        TextKey::ErrorUpdateWallet => "Errore aggiornamento wallet.",
        TextKey::ErrorArchiveWallet => "Errore archivio wallet.",
        TextKey::ErrorRestoreWallet => "Errore ripristino wallet.",
        TextKey::ErrorCreateFlow => "Errore creazione flow.",
        TextKey::ErrorUpdateFlow => "Errore aggiornamento flow.",
        TextKey::ErrorArchiveFlow => "Errore archiviazione flow.",
        TextKey::ErrorRestoreFlow => "Errore ripristino flow.",
        TextKey::ErrorCreateCategory => "Errore creazione categoria.",
        TextKey::ErrorUpdateCategory => "Errore aggiornamento categoria.",
        TextKey::ErrorArchiveCategory => "Errore archivio categoria.",
        TextKey::ErrorCreateAlias => "Errore creazione alias.",
        TextKey::ErrorDeleteAlias => "Errore eliminazione alias.",
        TextKey::ErrorCreateVault => "Errore creazione vault.",
        TextKey::ErrorDeleteVault => "Errore eliminazione vault.",
        TextKey::ErrorSaveDefaults => "Errore salvataggio default.",
        TextKey::ErrorInvalidCredentials => "Credenziali errate o pairing mancante.",
        TextKey::ErrorMembershipLastOwner => "Non puoi rimuovere l'ultimo owner del flow.",
        TextKey::ErrorMembershipOwnerImmutable => {
            "Non puoi cambiare il ruolo dell'owner del vault."
        }
        TextKey::ErrorMembershipOwnerRemoveForbidden => "Non puoi rimuovere l'owner del vault.",
        TextKey::ErrorOperationForbidden => "Operazione non permessa.",
        TextKey::ErrorResourceNotFound => "Risorsa non trovata.",
        TextKey::ErrorConflict => "Conflitto: {message}",
        TextKey::ErrorValidation => "Errore di validazione: {message}",
        TextKey::ErrorValidationAmbiguousVault => {
            "Nome vault ambiguo. Usa \"Main (owner)\" o un vault id."
        }
        TextKey::ErrorBadRequest => "Richiesta non valida: {message}",
        TextKey::ErrorServerError => "Errore server: {message}",
        TextKey::ErrorServerUnreachable => "Server non raggiungibile: {error}",

        // State Messages
        TextKey::StateSnapshotUnavailable => "Snapshot non disponibile.",
        TextKey::StateVaultUnavailable => "Vault non disponibile.",
        TextKey::StateUserUnavailable => "Utente non disponibile.",
        TextKey::StateNoWalletAvailable => "Nessun wallet disponibile.",
        TextKey::StateUnallocatedMissing => "Flow Unallocated mancante.",
        TextKey::StateCannotDetermineWalletTransfer => {
            "Impossibile determinare i wallet del transfer."
        }
        TextKey::StateCannotDetermineFlowTransfer => "Impossibile determinare i flow del transfer.",

        // UI Labels
        TextKey::UiOther => "Other",
        TextKey::UiError => "Error",
        TextKey::UiFailedToDeleteVault => "Failed to delete vault.",
        TextKey::UiNoDetailAvailable => "Nessun dettaglio disponibile.",
        TextKey::UiNoElement => "Nessun elemento.",
        TextKey::UiNoRecentCategories => "Nessuna categoria recente.",
        TextKey::UiRecentCategories => "Categorie recenti",

        // Prompts and Hints
        TextKey::PromptEnterName => "Inserisci un nome.",
        TextKey::PromptEnterUsername => "Inserisci un username.",
        TextKey::PromptEnterAlias => "Inserisci un alias.",
        TextKey::PromptEnterCap => "Inserisci un cap.",
        TextKey::PromptFillAllFields => "Compila tutti i campi.",
        TextKey::PromptUseDedicatedTransferForm => "Usa il form transfer dedicato.",
        TextKey::PromptAtLeastTwoCategories => "Serve almeno 2 categorie per unire.",
        TextKey::PromptSourceCategoryInvalid => "Categoria sorgente non valida.",
        TextKey::PromptDestinationCategoryInvalid => "Categoria destinazione non valida.",
        TextKey::PromptCheckConflicts => "Merge non valido. Controlla i conflitti.",
        TextKey::PromptNoShareableFlows => "Nessun flow condivisibile.",
        TextKey::PromptNoMemberSelected => "Nessun membro selezionato.",
        TextKey::PromptNoAliasSelected => "Nessun alias selezionato.",
        TextKey::PromptNoCategorySelected => "Nessuna categoria selezionata.",
        TextKey::PromptEnterCategory => "Inserisci una categoria.",

        // Quick Add / Parsing Errors
        TextKey::QuickAddEnterAmount => "Inserisci un importo.",
        TextKey::QuickAddAmountMissing => "Importo mancante.",
        TextKey::QuickAddAmountInvalid => "Importo non valido.",
        TextKey::QuickAddAmountMustBePositive => "Importo deve essere > 0.",
        TextKey::QuickAddTooManyCategories => "Troppi tag: massimo 1.",
        TextKey::QuickAddTooManyWallets => "Troppi wallet: massimo 1.",
        TextKey::QuickAddTooManyEnvelopes => "Troppi envelope: massimo 1.",
        TextKey::QuickAddSpecifyTwoWallets => "Specifica esattamente 2 wallet (@from @to).",
        TextKey::QuickAddSpecifyTwoFlows => "Specifica esattamente 2 flow (>from >to).",
        TextKey::QuickAddWalletNotFound => "Wallet non trovato: @{query}",
        TextKey::QuickAddEnvelopeNotFound => "Envelope non trovato: >{query}",
        TextKey::QuickAddFlowNotFound => "Flow non trovato: >{query}",
        TextKey::QuickAddWalletsMustBeDifferent => "I due wallet devono essere diversi.",
        TextKey::QuickAddFlowsMustBeDifferent => "I due flow devono essere diversi.",

        // Validation Errors
        TextKey::ValidationRequired => "Campo obbligatorio",
        TextKey::ValidationAmountRequired => "Importo obbligatorio",
        TextKey::ValidationAmountInvalid => "Importo non valido",
        TextKey::ValidationAmountPositive => "L'importo deve essere positivo",
        TextKey::ValidationDateRequired => "Data obbligatoria",
        TextKey::ValidationDateInvalid => "Formato data non valido (YYYY-MM-DD HH:MM)",
        TextKey::ValidationDateInvalidTimezone => "Fuso orario non valido",
        TextKey::ValidationLengthMin => "Minimo {min} caratteri",
        TextKey::ValidationLengthMax => "Massimo {max} caratteri",
        TextKey::ValidationTransferSameSource => "Scegli due wallet diversi.",
        TextKey::ValidationTransferSameDestination => "Scegli due flow diversi.",
        TextKey::ValidationTransferSameElements => "Scegli due elementi diversi.",
        TextKey::ValidationTransferMinimumTwo => "Servono almeno 2 elementi.",
        TextKey::ValidationWalletInvalid => "Wallet non valido.",
        TextKey::ValidationFlowInvalid => "Flow non valido.",
        TextKey::ValidationCategoryInvalid => "Categoria non valida.",
        TextKey::ValidationOpeningBalanceInvalid => "Saldo iniziale non valido.",
        TextKey::ValidationCapInvalid => "Cap non valido.",
        TextKey::ValidationCapMustBePositive => "Il cap deve essere > 0.",
        TextKey::ValidationOpeningBalanceNonNegative => "L'allocazione iniziale deve essere >= 0.",
        TextKey::ValidationNoWalletSelected => "Nessun wallet selezionato.",
        TextKey::ValidationNoFlowSelected => "Nessun flow selezionato.",
        TextKey::ValidationNoTransactionToVoid => "Nessuna transazione da annullare.",
        TextKey::ValidationUnallocatedCannotRename => "Unallocated non puo' essere rinominato.",
        TextKey::ValidationUnallocatedCannotArchive => "Unallocated non puo' essere archiviato.",
        TextKey::ValidationAlreadyArchived => "Gia' archiviato.",
        TextKey::ValidationSystemCategoryImmutable => "Le categorie di sistema non si modificano.",
        TextKey::ValidationNoTransactionSelected => "Nessuna transazione selezionata.",
        TextKey::ValidationNoWalletAvailable => "Nessun wallet disponibile.",
        TextKey::ValidationNoFlowAvailable => "Nessun flow disponibile.",
        TextKey::ValidationNoElementAvailable => "Nessun elemento disponibile.",
        TextKey::ValidationTransactionVoided => "Transazione annullata: modifica non disponibile.",
        TextKey::ValidationTransactionInvalid => "Transazione non valida.",
        TextKey::ValidationTransferWalletInvalid => "Transfer wallet non valido.",
        TextKey::ValidationTransferFlowInvalid => "Transfer flow non valido.",
        TextKey::ValidationWalletArchived => "Wallet archiviato: modifica non disponibile.",
        TextKey::ValidationFlowArchived => "Flow archiviato: modifica non disponibile.",
        TextKey::ValidationSnapshotUnavailable => "Snapshot non disponibile.",

        // Home screen stat cards
        TextKey::HomeCardIncome => "Entrate",
        TextKey::HomeCardExpenses => "Spese",
        TextKey::HomeCardThisMonth => "questo mese",

        // Dialog titles and messages
        TextKey::DialogUnsavedChangesTitle => "Modifiche non salvate",
        TextKey::DialogUnsavedChangesMessage => "Hai modifiche non salvate. Scartarle?",
        TextKey::DialogSave => "Salva",
        TextKey::DialogDiscard => "Scarta",
        TextKey::DialogDelete => "Elimina",
        TextKey::DialogDeleteVaultTitle => "Elimina Vault",
        TextKey::DialogDeleteVaultWarning => "Questa azione non puo' essere annullata.",
        TextKey::DialogDeleteTransactionsTitle => "Elimina Transazioni",
        TextKey::DialogDeleteTransactionTitle => "Elimina Transazione",
        TextKey::DialogDeleteUndoHint => "Puoi annullare questa azione per 5 secondi.",
        TextKey::DialogDeleteWalletTitle => "Elimina Wallet",
        TextKey::DialogDeleteWalletHint => {
            "Il wallet sara' nascosto ma potra' essere ripristinato."
        }
        TextKey::DialogDeleteFlowTitle => "Elimina Flow",
        TextKey::DialogDeleteFlowHint => "Il flow sara' nascosto ma potra' essere ripristinato.",
        TextKey::DialogDeleteCategoryTitle => "Elimina Categoria",
        TextKey::DialogDeleteCategoryHint => {
            "La categoria sara' nascosta ma potra' essere ripristinata."
        }
        TextKey::DialogThisVault => "questo vault",
        TextKey::DialogTransaction => "Transazione",
        TextKey::DialogConnectionErrorTitle => "Errore Connessione",
        TextKey::DialogConnectionErrorMessage => "Impossibile connettersi al server.",

        // Form field labels
        TextKey::FormAmount => "Importo",
        TextKey::FormWallet => "Wallet",
        TextKey::FormFlow => "Flow",
        TextKey::FormCategory => "Categoria",
        TextKey::FormNote => "Nota",
        TextKey::FormWhen => "Quando",

        // Form field helpers
        TextKey::FormHelperAmount => "Importo numerico (obbligatorio)",
        TextKey::FormHelperWallet => "Wallet sorgente/destinazione",
        TextKey::FormHelperFlow => "Busta/allocazione budget",
        TextKey::FormHelperCategory => "Tag per statistiche",
        TextKey::FormHelperNote => "Descrizione opzionale",
        TextKey::FormHelperWhen => "Data e ora (default: adesso)",

        // Form titles
        TextKey::FormEditIncome => "Modifica Entrata",
        TextKey::FormNewIncome => "Nuova Entrata",
        TextKey::FormEditExpense => "Modifica Spesa",
        TextKey::FormNewExpense => "Nuova Spesa",
        TextKey::FormEditRefund => "Modifica Rimborso",
        TextKey::FormNewRefund => "Nuovo Rimborso",
        TextKey::FormEditTransaction => "Modifica Transazione",
        TextKey::FormNewTransaction => "Nuova Transazione",

        // Form footer hints
        TextKey::FormHintSave => " Salva  ",
        TextKey::FormHintCancel => " Annulla  ",
        TextKey::FormHintNextField => " Campo succ.  ",
        TextKey::FormHintCycleChoices => " Cambia scelta",

        // Transaction list / common labels
        TextKey::TxnScopeAll => "Tutti",
        TextKey::TxnUncategorized => "Senza categoria",
        TextKey::TxnNoWallet => "Nessun wallet",
        TextKey::TxnNoEnvelope => "Nessuna busta",
        TextKey::TxnRecentsPrefix => "Recenti: ",
        TextKey::TxnRecentsCategories => "Categorie: ",
        TextKey::TxnRecentsWallet => "Wallet: ",
        TextKey::TxnRecentsFlow => "Flow: ",

        // Quick add
        TextKey::QuickAddTitle => " Quick Add ",
        TextKey::QuickAddPlaceholder => "Premi [n] per aggiungere una transazione...",
        TextKey::QuickAddToday => "Oggi",
        TextKey::QuickAddSyntaxHint => {
            "Sintassi: [+]importo nota [#cat] [@wallet] [>busta]  |  + entrata, r rimborso"
        }
        TextKey::QuickAddSyntaxShort => "[+]importo nota [#categoria] [@wallet] [>busta]",
        TextKey::QuickAddExamples => "   Esempi: ",
        TextKey::QuickAddEnvelopeSuggestions => "Suggerimenti busta: ",
        TextKey::QuickAddCycle => " cambia",

        // Stats labels
        TextKey::StatsTitle => "Statistiche",
        TextKey::StatsNoData => "Nessun dato. Premi ",
        TextKey::StatsRefreshHint => " per aggiornare.",
        TextKey::StatsMonthSummary => "Riepilogo Mese",
        TextKey::StatsMoM => "MoM",
        TextKey::StatsInc => "Ent",
        TextKey::StatsExp => "Spe",
        TextKey::StatsNa => "n/d",
        TextKey::StatsIncome => "Entrate",
        TextKey::StatsExpenses => "Spese",
        TextKey::StatsExpenseOverIncome => "Spese/Entrate",
        TextKey::StatsNoIncomeToCompare => "Nessuna entrata da confrontare",
        TextKey::StatsNet => "Netto",
        TextKey::StatsBalance => "Saldo",
        TextKey::StatsCategoryBreakdown => "Ripartizione Categorie",
        TextKey::StatsNoCategoryData => "Nessun dato di spesa per le categorie",
        TextKey::StatsDistribution => "Distribuzione",
        TextKey::StatsBalanceTrend => "Trend Saldo (30gg)",
        TextKey::StatsMonthlyTrend => "Trend Mensile",
        TextKey::StatsMonthlyTrendNoData => {
            "Trend mensile non disponibile. Premi 'r' per aggiornare."
        }
        TextKey::StatsFinancialTrends => "Trend Finanziari (6 mesi)",
        TextKey::StatsNetSavings => "Risparmio",
        TextKey::StatsTotalIncome => "Entrate Totali",
        TextKey::StatsTotalExpenses => "Spese Totali",
        TextKey::StatsNetBalance => "Saldo Netto",
        TextKey::StatsThisMonth => "Questo mese",
        TextKey::StatsExpenseTrend => "Trend Spese (6m)",
        TextKey::StatsNoExpenseData => "Nessun dato di trend spese. Premi 'r' per aggiornare.",
        TextKey::StatsTabCashFlow => "1 Flusso di cassa",
        TextKey::StatsTabSpending => "2 Spese",
        TextKey::StatsTabNetWorth => "3 Patrimonio",

        // Category screen labels
        TextKey::CatTitle => " Categorie ",
        TextKey::CatNoCategoriesYet => "Nessuna categoria ancora",
        TextKey::CatCreateFirst => " per creare la prima categoria",
        TextKey::CatRenameTitle => "Rinomina Categoria",
        TextKey::CatCurrentLabel => "Corrente:",
        TextKey::CatNewNameLabel => "Nuovo nome:",
        TextKey::CatAliasesFor => " Alias per {} ",
        TextKey::CatNoAliasesForCategory => "Nessun alias per questa categoria",
        TextKey::CatTypeToAddAlias => "Scrivi nell'input sotto per aggiungerne uno",
        TextKey::CatNewAlias => "  Nuovo alias: ",
        TextKey::CatSwitchFocus => " cambia focus  ",
        TextKey::CatAliasesTitle => " Alias ",
        TextKey::CatNoCategorySelected => "Nessuna categoria selezionata",
        TextKey::CatCategoryLabel => "  Categoria: ",
        TextKey::CatPressToLoadAliases => "  Premi [l] per caricare gli alias",
        TextKey::CatNoAliases => "  Nessun alias",
        TextKey::CatMore => "    ... +{} altri",
        TextKey::CatNewCategoryTitle => " Nuova Categoria ",
        TextKey::CatMergeCategoriesTitle => " Unisci Categorie ",
        TextKey::CatMergeLabel => "  Unisci: ",
        TextKey::CatMergePreviewOkMerge => "Preview OK. Premi [Enter] per unire.",
        TextKey::CatMergePreviewOkConfirm => "Preview OK. Premi [Enter] per confermare.",
        TextKey::CatMergeConflicts => "  Conflitti:",
        TextKey::CatMergeSelectTarget => {
            "  Seleziona la destinazione e premi [Enter] per l'anteprima"
        }
        TextKey::CatMergePreviewAction => " preview/unisci  ",
        TextKey::CatConflictSameCategory => "Non puoi unire una categoria con se stessa",
        TextKey::CatConflictSourceSystem => "Categoria di sistema: {value}",
        TextKey::CatConflictTargetArchived => "Destinazione archiviata: {value}",
        TextKey::CatConflictAliasConflict => "Conflitto alias: {value}",
        TextKey::CatConflictNameConflict => "Conflitto nome: {value}",
        TextKey::CatConflictGeneric => "Conflitto: {kind} ({value})",

        // Category list badges
        TextKey::CatBadgeSystem => "[system]",
        TextKey::CatBadgeArchived => "[archiviato]",
        TextKey::CatBadgeFrom => "[DA]",
        TextKey::CatBadgeTo => "[A]",

        // Category form hints
        TextKey::CatHintCreate => " crea  ",
        TextKey::CatHintRename => " rinomina  ",
        TextKey::CatHintAliases => " alias  ",
        TextKey::CatHintMerge => " unisci",

        // Transaction header / grouping
        TextKey::TxnGroupDate => "Data",
        TextKey::TxnGroupCategory => "Categoria",
        TextKey::TxnGroupWallet => "Wallet",
        TextKey::TxnGroupEnvelope => "Busta",
        TextKey::TxnHeaderFiltersOff => "Filtri [off]",
        TextKey::TxnHeaderSearch => "Cerca: ",

        // Transaction detail
        TextKey::TxnDetailTitle => "Dettaglio Transazione",
        TextKey::TxnDetailKind => "Tipo",
        TextKey::TxnDetailVoided => "Annullata",
        TextKey::TxnDetailVoidedYes => "SI'",
        TextKey::TxnDetailVoidedNo => "NO",
        TextKey::TxnDetailWhen => "Quando",
        TextKey::TxnDetailAmount => "Importo",
        TextKey::TxnDetailCategory => "Categoria",
        TextKey::TxnDetailNote => "Nota",
        TextKey::TxnDetailLegsTitle => "Movimenti",
        TextKey::TxnDetailLegWallet => "Wallet",
        TextKey::TxnDetailLegFlow => "Flow",

        // Vault view / defaults
        TextKey::VaultDefaultName => "Main",
        TextKey::VaultQuickDefaults => "Quick Defaults",
        TextKey::VaultDefaultWallet => "Wallet Predefinito",
        TextKey::VaultDefaultFlow => "Flow Predefinito",
        TextKey::VaultIdLabel => "ID",
        TextKey::VaultCurrencyLabel => "Valuta",

        // Loading & empty states
        TextKey::LoadingGeneric => "Caricamento...",
        TextKey::LoadingVaultData => "Recupero dati del vault",
        TextKey::SearchLabel => "Cerca: ",
        TextKey::SearchNoResults => "Nessun risultato per ",
        TextKey::SearchClearHint => "[Esc] per cancellare",
        TextKey::SearchClearShort => "[Esc] cancella",

        // Date labels
        TextKey::DateToday => "Oggi",
        TextKey::DateYesterday => "Ieri",

        // Entity list shared
        TextKey::EntityArchivedOn => "Archiviati: S\u{00ec}",
        TextKey::EntityBadgeArchived => "[archiviato]",
        TextKey::EntityBadgeDefault => "[predefinito]",

        // Wallet screen
        TextKey::WalletTitle => " Wallet ",
        TextKey::WalletDetailTitle => "Dettaglio Wallet",
        TextKey::WalletNotFound => "Wallet non trovato",
        TextKey::WalletSelectPrompt => "Seleziona un wallet per i dettagli",
        TextKey::WalletNoTransactions => "Nessuna transazione per questo wallet",
        TextKey::WalletWelcomeTitle => "\u{1f4b0} Benvenuto!",
        TextKey::WalletWelcomeDesc1 => "Crea il tuo primo wallet per iniziare",
        TextKey::WalletWelcomeDesc2 => "a tracciare le tue finanze.",
        TextKey::WalletHintQuickCreate => " Creazione rapida  ",
        TextKey::WalletHintCreateDetails => " Crea con dettagli",
        TextKey::FormTitleRenameWallet => "Rinomina Wallet",
        TextKey::FormTitleNewWallet => "Nuovo Wallet",

        // Flow screen
        TextKey::FlowTitle => " Budget e Obiettivi ",
        TextKey::FlowDetailTitle => "Dettaglio Flow",
        TextKey::FlowNotFound => "Flow non trovato",
        TextKey::FlowSelectPrompt => "Seleziona un flow per i dettagli",
        TextKey::FlowNoTransactions => "Nessuna transazione per questo flow",
        TextKey::FlowWelcomeTitle => "\u{1f4e6} Budget con Buste",
        TextKey::FlowWelcomeDesc1 => "Crea buste per organizzare e monitorare",
        TextKey::FlowWelcomeDesc2 => "le spese per categoria o obiettivo.",
        TextKey::FlowHintQuickCreate => " Creazione rapida  ",
        TextKey::FlowHintCreateCap => " Crea con limite",
        TextKey::FormTitleRenameFlow => "Rinomina Flow",
        TextKey::FormTitleNewFlow => "Nuovo Budget/Obiettivo",

        // Home screen
        TextKey::HomeActivityFeed => "Attivit\u{00e0} Recente",
        TextKey::HomeNetWorth => "Patrimonio Netto",
        TextKey::HomeQuickBalances => "Saldi",
        TextKey::HomeNoDataYet => "Nessun dato",
        TextKey::HomeAddFirstTxn => "[n] per aggiungere la prima transazione",
        TextKey::HomeWallets => "Wallet",
        TextKey::HomeBudgets => "Budget",
        TextKey::HomeNoActivityYet => "Nessuna attivit\u{00e0}",

        // Settings
        TextKey::SettingsCardTitle => "Impostazioni",
        TextKey::PreferencesTitle => "Preferenze",

        // Members
        TextKey::MembersVaultTitle => "Membri del Vault",
        TextKey::MembersEditTitle => "Modifica Membro",
        TextKey::MembersAddTitle => "Aggiungi Membro",

        // Vault
        TextKey::VaultCreateTitle => "Crea Vault",

        // Transfers & pickers
        TextKey::TransferWalletTitle => "Transfer Wallet",
        TextKey::TransferFlowTitle => "Transfer Flow",
        TextKey::TransferEditWalletTitle => "Modifica Transfer Wallet",
        TextKey::TransferEditFlowTitle => "Modifica Transfer Flow",
        TextKey::TransferTypeTitle => "Tipo di Transfer",
        TextKey::TransferFrom => "Da",
        TextKey::TransferTo => "A",
        TextKey::TransferAvailable => "Disponibili",
        TextKey::TransferBadgeFrom => " [da]",
        TextKey::TransferBadgeTo => " [a]",
        TextKey::TransferFormHints => {
            "Tab: avanti \u{2022} \u{2191}/\u{2193}: cambia \u{2022} Invio: salva \u{2022} Esc: annulla"
        }

        // Filters
        TextKey::FilterTitle => " Filtri ",
        TextKey::FilterFrom => "Da",
        TextKey::FilterTo => "A",
        TextKey::FilterTransactionTypes => "Tipi di Transazione ",
        TextKey::FilterToggleHint => "(premi per alternare)",
        TextKey::FilterKindIncome => "Entrata",
        TextKey::FilterKindExpense => "Spesa",
        TextKey::FilterKindRefund => "Rimborso",
        TextKey::FilterKindWalletTransfer => "Transfer Wallet",
        TextKey::FilterKindFlowTransfer => "Transfer Flow",

        // Pickers
        TextKey::PickerAllWallets => "Tutti i wallet",
        TextKey::PickerAllFlows => "Tutti i flow",
        TextKey::PickerSelectWallet => "Seleziona wallet",
        TextKey::PickerSelectFlow => "Seleziona flow",
        TextKey::PickerBadgeUnallocated => " [Non allocato]",
        TextKey::PickerSuffixArchived => " (archiviato)",

        // Grouping dialog
        TextKey::GroupingTitle => " Raggruppa Transazioni ",
        TextKey::GroupingDate => "Data",
        TextKey::GroupingCategory => "Categoria",
        TextKey::GroupingWallet => "Wallet",
        TextKey::GroupingEnvelope => "Busta",
        TextKey::GroupingCurrent => "corrente",

        // Transactions list
        TextKey::TxnNoTransactionsYet => "Nessuna transazione. Premi ",
        TextKey::TxnAddOneHint => " per aggiungerne una.",
        TextKey::TxnSearchEditClearHint => "Ctrl+F modifica \u{2022} Esc cancella",

        // Scope labels (transaction common)
        TextKey::ScopeFlowLabel => "Flow: ",
        TextKey::ScopeFlowUnknown => "Flow: ?",
        TextKey::ScopeWalletLabel => "Wallet: ",
        TextKey::ScopeWalletUnknown => "Wallet: ?",

        // Shell / Status
        TextKey::ShellVaultLabel => "Vault",
        TextKey::ShellUserLabel => "Utente",
        TextKey::ShellVaultFallback => "Principale",

        // Error dialog
        TextKey::ErrorTechnicalDetails => "Dettagli tecnici:",

        // Flow form
        TextKey::FormLabelAllowNegative => "Consenti negativo",
        TextKey::FlowBadgeAllowNegative => "[neg. ok]",

        // Recurring
        TextKey::RecurringTitle => "Ricorrenti",
        TextKey::RecurringEmpty => "Nessun modello ricorrente",
        TextKey::RecurringPending => "In attesa di conferma",
        TextKey::RecurringFreqDaily => "Giornaliero",
        TextKey::RecurringFreqWeekly => "Settimanale",
        TextKey::RecurringFreqMonthly => "Mensile",
        TextKey::RecurringFreqYearly => "Annuale",
        TextKey::RecurringFormTitle => "Nuovo Ricorrente",
        TextKey::RecurringFormEditTitle => "Modifica Ricorrente",
        TextKey::RecurringFormKind => "Tipo",
        TextKey::RecurringFormAmount => "Importo",
        TextKey::RecurringFormFrequency => "Frequenza",
        TextKey::RecurringFormDay => "Giorno del periodo",
        TextKey::RecurringFormStartDate => "Data inizio",
        TextKey::RecurringFormEndDate => "Data fine",
        TextKey::RecurringKindIncome => "Entrata",
        TextKey::RecurringKindExpense => "Spesa",
        TextKey::RecurringEnabled => "on",
        TextKey::RecurringDisabled => "off",
        TextKey::RecurringCreated => "Modello ricorrente creato.",
        TextKey::RecurringUpdated => "Modello ricorrente aggiornato.",
        TextKey::RecurringArchived => "Modello ricorrente archiviato.",
        TextKey::RecurringExecuted => "Transazione ricorrente eseguita.",

        // General UI
        TextKey::UiNone => "Nessuno",
        TextKey::UiUndoApplied => "Annullamento applicato.",
        TextKey::UiYes => "S\u{00ec}",
        TextKey::UiNo => "No",
    }
}
