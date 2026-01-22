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
        TextKey::SectionStats => "Statistiche",

        // Form Labels
        TextKey::LabelAmount => "Importo",
        TextKey::LabelWallet => "Portafoglio",
        TextKey::LabelFlow => "Flusso",
        TextKey::LabelCategory => "Categoria",
        TextKey::LabelNote => "Nota",
        TextKey::LabelOccurredAt => "Data",
        TextKey::LabelFrom => "Da",
        TextKey::LabelTo => "A",
        TextKey::LabelName => "Nome",
        TextKey::LabelUsername => "Username",
        TextKey::LabelRole => "Ruolo",
        TextKey::LabelOpeningBalance => "Saldo iniziale",
        TextKey::LabelCap => "Limite",

        // Form Titles
        TextKey::TitleNewExpense => "Nuova Spesa",
        TextKey::TitleNewIncome => "Nuova Entrata",
        TextKey::TitleNewRefund => "Nuovo Rimborso",
        TextKey::TitleNewTransfer => "Nuovo Trasferimento",
        TextKey::TitleEditTransaction => "Modifica Transazione",
        TextKey::TitleNewWallet => "Nuovo Portafoglio",
        TextKey::TitleEditWallet => "Modifica Portafoglio",
        TextKey::TitleNewFlow => "Nuovo Flusso",
        TextKey::TitleEditFlow => "Modifica Flusso",
        TextKey::TitleNewCategory => "Nuova Categoria",
        TextKey::TitleEditCategory => "Modifica Categoria",
        TextKey::TitleNewMember => "Nuovo Membro",
        TextKey::TitleEditMember => "Modifica Membro",
        TextKey::TitleNewVault => "Nuovo Vault",
        TextKey::TitleVaultDefaults => "Impostazioni Vault",
        TextKey::TitleSelectVault => "Seleziona Vault",

        // Validation Errors
        TextKey::ValidationRequired => "Campo obbligatorio",
        TextKey::ValidationAmountRequired => "Importo obbligatorio",
        TextKey::ValidationAmountInvalid => "Importo non valido",
        TextKey::ValidationAmountPositive => "L'importo deve essere positivo",
        TextKey::ValidationAmountNegative => "L'importo deve essere negativo",
        TextKey::ValidationDateRequired => "Data obbligatoria",
        TextKey::ValidationDateInvalid => "Formato data non valido (YYYY-MM-DD HH:MM)",
        TextKey::ValidationDateInvalidTimezone => "Fuso orario non valido",
        TextKey::ValidationLengthMin => "Minimo {min} caratteri",
        TextKey::ValidationLengthMax => "Massimo {max} caratteri",
        TextKey::ValidationWalletRequired => "Seleziona un portafoglio",
        TextKey::ValidationFlowRequired => "Seleziona un flusso",
        TextKey::ValidationCategoryRequired => "Seleziona una categoria",
        TextKey::ValidationNameRequired => "Nome obbligatorio",
        TextKey::ValidationUsernameRequired => "Username obbligatorio",
        TextKey::ValidationRoleRequired => "Ruolo obbligatorio",
        TextKey::ValidationFromRequired => "Origine obbligatoria",
        TextKey::ValidationToRequired => "Destinazione obbligatoria",

        // Empty States
        TextKey::EmptyTransactions => "Nessuna transazione",
        TextKey::EmptyWallets => "Nessun portafoglio",
        TextKey::EmptyFlows => "Nessun flusso",
        TextKey::EmptyCategories => "Nessuna categoria",
        TextKey::EmptyMembers => "Nessun membro",
        TextKey::EmptyVaults => "Nessun vault",
        TextKey::EmptyStats => "Nessuna statistica disponibile",
        TextKey::EmptyResults => "Nessun risultato",

        // Actions
        TextKey::ActionSave => "Salva",
        TextKey::ActionCancel => "Annulla",
        TextKey::ActionCreate => "Crea",
        TextKey::ActionEdit => "Modifica",
        TextKey::ActionDelete => "Elimina",
        TextKey::ActionArchive => "Archivia",
        TextKey::ActionVoid => "Annulla",
        TextKey::ActionRefund => "Rimborsa",
        TextKey::ActionTransfer => "Trasferisci",
        TextKey::ActionConfirm => "Conferma",
        TextKey::ActionBack => "Indietro",
        TextKey::ActionRefresh => "Aggiorna",

        // Hints
        TextKey::HintPressEnter => "Premi Invio per confermare",
        TextKey::HintPressEsc => "Premi Esc per annullare",
        TextKey::HintPressTab => "Premi Tab per passare al campo successivo",
        TextKey::HintSelectWithArrows => "Usa le frecce per selezionare",
        TextKey::HintTypeToSearch => "Digita per cercare",
        TextKey::HintLoadingData => "Caricamento in corso...",
        TextKey::HintNoSelection => "Nessuna selezione",
        TextKey::HintConfirmDelete => "Premi di nuovo per confermare",

        // Success Messages
        TextKey::SuccessCreated => "Creato con successo",
        TextKey::SuccessUpdated => "Aggiornato con successo",
        TextKey::SuccessDeleted => "Eliminato con successo",
        TextKey::SuccessArchived => "Archiviato con successo",
        TextKey::SuccessVoided => "Annullato con successo",
        TextKey::SuccessRefreshed => "Dati aggiornati",

        // Error Messages
        TextKey::ErrorGeneric => "Si e' verificato un errore",
        TextKey::ErrorNetwork => "Errore di connessione",
        TextKey::ErrorNotFound => "Non trovato",
        TextKey::ErrorUnauthorized => "Non autorizzato",

        // Misc
        TextKey::MiscYes => "Si",
        TextKey::MiscNo => "No",
        TextKey::MiscAll => "Tutti",
        TextKey::MiscNone => "Nessuno",
    }
}
