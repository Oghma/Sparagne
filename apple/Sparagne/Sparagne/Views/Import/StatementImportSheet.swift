import SwiftUI
import SparagneCore
import UniformTypeIdentifiers

/// Importing a bank or card statement (`core/src/statement/mod.rs`), in one
/// sheet with four pages: choose the file, map its columns and pick where
/// it goes, review every row, read the report.
///
/// The model (`StatementImportModel`) holds the choices and talks to the
/// core; this view only lays the pages out in the ledger's palette. Problems
/// show on the sheet itself, above the buttons: the window's alert would
/// wait until the sheet is gone.
struct StatementImportSheet: View {
    let store: AppStore
    @State private var model: StatementImportModel
    @State private var choosingFile = false
    @Environment(\.dismiss) private var dismiss

    /// What the file panel offers: CSV, TSV and plain text, which is what
    /// banks call their exports.
    private static let fileTypes: [UTType] = [.commaSeparatedText, .tabSeparatedText, .plainText, .text]

    /// `model` is for previews, which open the sheet on a file already read.
    init(store: AppStore, model: StatementImportModel? = nil) {
        self.store = store
        _model = State(initialValue: model ?? StatementImportModel(store: store))
    }

    var body: some View {
        VStack(spacing: 0) {
            StepStrip(step: model.step, fileName: model.fileName)
            Hairline()
            if store.isReadOnly {
                readOnly
            } else {
                page
            }
            Hairline()
            footer
        }
        .frame(width: ImportMetrics.sheetWidth, height: ImportMetrics.sheetHeight)
        .background(Ink.bg)
        .preferredColorScheme(.dark)
        .fileImporter(isPresented: $choosingFile, allowedContentTypes: Self.fileTypes) { result in
            switch result {
            case .success(let url): Task { await model.open(url) }
            case .failure(let error): model.problem = StatementImportProblem(error)
            }
        }
        // Every change to the mapping or the target re-reads the file. The
        // pause lets a value being typed settle; a change during it cancels
        // this run and starts the next.
        .task(id: model.previewInput) {
            guard (try? await Task.sleep(for: .milliseconds(200))) != nil else { return }
            await model.refreshPreview()
        }
    }

    // MARK: - Pages

    @ViewBuilder
    private var page: some View {
        switch model.step {
        case .file:
            filePage
        case .mapping:
            ScrollView { StatementMappingEditor(model: model) }
                .scrollBounceBehavior(.basedOnSize)
        case .review:
            StatementReview(model: model)
        case .report:
            if let report = model.report {
                ScrollView { StatementReportView(report: report) }
                    .scrollBounceBehavior(.basedOnSize)
            }
        }
    }

    private var filePage: some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "doc.text")
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(Ink.dim)
            Text(String(localized: "Choose a CSV file exported by your bank or card. Nothing is imported before you have reviewed every row."))
                .font(Face.row)
                .foregroundStyle(Ink.text)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
            Button(String(localized: "Choose File…")) { choosingFile = true }
                .keyboardShortcut(.defaultAction)
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var readOnly: some View {
        VStack(spacing: 8) {
            Spacer()
            Text(String(localized: "This vault is shared with you to read, not to change."))
                .font(Face.row)
                .foregroundStyle(Ink.text)
            Text(String(localized: "Nothing can be imported into it."))
                .font(Face.row)
                .foregroundStyle(Ink.dim)
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: - Footer

    private var footer: some View {
        HStack(spacing: 10) {
            status
            Spacer(minLength: 12)
            buttons
        }
        .padding(.horizontal, Metrics.gutter)
        .frame(height: 52)
    }

    /// What stands between the user and the next page, or where the file
    /// stands.
    @ViewBuilder
    private var status: some View {
        if let problem = model.problem {
            ProblemLine(problem: problem)
        } else if store.isReadOnly {
            EmptyView()
        } else {
            switch model.step {
            case .file:
                EmptyView()
            case .mapping:
                mappingStatus
            case .review:
                if model.isImporting {
                    ProgressView().controlSize(.small)
                } else if let problem = model.previewProblem {
                    ProblemLine(problem: problem)
                }
            case .report:
                EmptyView()
            }
        }
    }

    @ViewBuilder
    private var mappingStatus: some View {
        if !model.mappingIsComplete {
            HintLine(text: String(localized: "Choose the date and amount columns"))
        } else if model.walletId == nil {
            HintLine(text: String(localized: "Choose the wallet the statement belongs to"))
        } else if let problem = model.previewProblem {
            ProblemLine(problem: problem)
        } else if model.preview != nil {
            StatementCountsStrip(counts: model.counts)
        } else {
            ProgressView().controlSize(.small)
        }
    }

    @ViewBuilder
    private var buttons: some View {
        if store.isReadOnly {
            Button(String(localized: "Done")) { dismiss() }
                .keyboardShortcut(.defaultAction)
        } else {
            switch model.step {
            case .file:
                Button(String(localized: "Cancel"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            case .mapping:
                Button(String(localized: "Cancel"), role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(String(localized: "Choose Another File…")) { choosingFile = true }
                Button(String(localized: "Preview")) { model.step = .review }
                    .keyboardShortcut(.defaultAction)
                    .disabled(model.preview == nil || model.previewProblem != nil)
            case .review:
                Button(String(localized: "Back")) { model.step = .mapping }
                    .keyboardShortcut(.cancelAction)
                    .disabled(model.isImporting)
                Button(String(localized: "Import \(model.counts.new) rows")) {
                    Task { await model.runImport() }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(model.counts.new == 0 || model.isImporting || model.isPreviewing || model.preview == nil)
            case .report:
                Button(String(localized: "Done")) { dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
        }
    }
}

// MARK: - Chrome

/// Widths and heights the import sheet's pages agree on.
enum ImportMetrics {
    static let sheetWidth: CGFloat = 880
    static let sheetHeight: CGFloat = 640
    /// Horizontal padding inside a table cell: tighter than the ledger's, the
    /// review has more columns in less width.
    static let cellPadding: CGFloat = 6
}

/// `FILE › COLUMNS › PREVIEW › REPORT`, the current page in the accent, and
/// the file's name on the right.
private struct StepStrip: View {
    let step: StatementImportModel.Step
    let fileName: String?

    private static let steps: [(StatementImportModel.Step, String)] = [
        (.file, String(localized: "File")),
        (.mapping, String(localized: "Columns")),
        (.review, String(localized: "Preview")),
        (.report, String(localized: "Report")),
    ]

    var body: some View {
        HStack(spacing: 8) {
            SectionLabel(text: String(localized: "Import Statement"), tint: Ink.text)
            Spacer().frame(width: 12)
            ForEach(Array(Self.steps.enumerated()), id: \.offset) { index, entry in
                if index > 0 {
                    Text(verbatim: "\u{203A}").font(Face.label).foregroundStyle(Ink.dim)
                }
                SectionLabel(text: entry.1, tint: entry.0 == step ? Ink.accent : Ink.dim)
            }
            Spacer()
            if let fileName {
                Text(fileName)
                    .font(Face.footnote)
                    .foregroundStyle(Ink.dim)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .padding(.horizontal, Metrics.gutter)
        .frame(height: 36)
    }
}

/// A problem in the caution ink: the headline, then the core's detail.
struct ProblemLine: View {
    let problem: StatementImportProblem

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "exclamationmark.triangle")
            Text(problem.headline)
            if !problem.detail.isEmpty {
                Text(problem.detail).foregroundStyle(Ink.dim)
            }
        }
        .font(Face.footnote)
        .foregroundStyle(Ink.warning)
        .lineLimit(2)
        .help(problem.detail)
    }
}

/// What the user still has to choose, in the dim ink.
private struct HintLine: View {
    let text: String

    var body: some View {
        Text(text)
            .font(Face.footnote)
            .foregroundStyle(Ink.dim)
    }
}
