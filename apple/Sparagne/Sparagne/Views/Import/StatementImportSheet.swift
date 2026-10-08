import SwiftUI
import SparagneCore
import UniformTypeIdentifiers

/// Importing a bank or card statement (`core/src/statement/mod.rs`), in one
/// sheet with four pages: choose the file, map its columns and pick where
/// it goes, review every row, read the report.
///
/// The model (`StatementImportModel`) holds the choices and talks to the
/// core; this view only lays the pages out, in the frame every dialog of the
/// window shares (`FormSheet`): the title, the steps, the page, and the
/// buttons in the footer band. Problems show on the sheet itself, under the
/// page: the window's alert would wait until the sheet is gone.
struct StatementImportSheet: View {
    let store: AppStore
    @State private var model: StatementImportModel
    @State private var choosingFile = false
    @Environment(\.dismiss) private var dismiss

    /// What the file panel offers: CSV, TSV and plain text, which is what
    /// banks call their exports.
    private static let fileTypes: [UTType] = [.commaSeparatedText, .tabSeparatedText, .plainText, .text]

    /// Wider and taller than the other sheets: the review has eight columns
    /// (`docs/v2/UI.md` §2.6).
    private static let size = CGSize(width: 880, height: 640)

    /// `model` is for previews, which open the sheet on a file already read.
    init(store: AppStore, model: StatementImportModel? = nil) {
        self.store = store
        _model = State(initialValue: model ?? StatementImportModel(store: store))
    }

    var body: some View {
        FormSheet(String(localized: "Import Statement"), width: Self.size.width) {
            VStack(alignment: .leading, spacing: 14) {
                StepStrip(model: model)
                Group {
                    if store.isReadOnly {
                        readOnly
                    } else {
                        page
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                status
            }
        } footer: {
            buttons
        }
        .frame(height: Self.size.height)
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

    /// What the sheet is for, in the middle of the page; the button that
    /// opens the file panel is the footer's primary, so ↩ opens it.
    private var filePage: some View {
        VStack(spacing: 12) {
            Image(systemName: "doc.text")
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(Ink.text3)
                .accessibilityHidden(true)
            Text(String(localized: "Choose a CSV file exported by your bank or card. Nothing is imported before you have reviewed every row."))
                .font(Face.ui(13))
                .foregroundStyle(Ink.text2)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var readOnly: some View {
        VStack(spacing: 6) {
            Text(String(localized: "This vault is shared with you to read, not to change."))
                .foregroundStyle(Ink.text)
            Text(String(localized: "Nothing can be imported into it."))
                .foregroundStyle(Ink.text3)
        }
        .font(Face.ui(13))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: - Under the page

    /// What went wrong, what still stands between the user and the next
    /// page, or the counts the import would make.
    @ViewBuilder
    private var status: some View {
        if let problem = model.problem {
            ProblemNote(problem: problem)
        } else if !store.isReadOnly {
            switch model.step {
            case .file, .report:
                EmptyView()
            case .mapping:
                mappingStatus
            case .review:
                reviewStatus
            }
        }
    }

    @ViewBuilder
    private var mappingStatus: some View {
        if !model.mappingIsComplete {
            FormNote(String(localized: "Choose the date and amount columns"), indented: false)
        } else if model.walletId == nil {
            FormNote(String(localized: "Choose the wallet the statement belongs to"), indented: false)
        } else if let problem = model.previewProblem {
            ProblemNote(problem: problem)
        } else if model.preview != nil {
            StatementCountsLine(counts: model.counts)
        } else {
            ProgressView().controlSize(.small)
        }
    }

    @ViewBuilder
    private var reviewStatus: some View {
        if let problem = model.previewProblem {
            ProblemNote(problem: problem)
        } else {
            HStack(spacing: 8) {
                StatementCountsLine(counts: model.counts)
                Spacer(minLength: 8)
                if model.isPreviewing || model.isImporting {
                    ProgressView().controlSize(.small)
                }
            }
        }
    }

    // MARK: - Footer

    /// Cancel (esc), the way back, and the page's next move (↩). On the
    /// review, esc goes back to the columns rather than closing the sheet,
    /// so a stray esc never throws the mapping away.
    @ViewBuilder
    private var buttons: some View {
        if store.isReadOnly {
            FormPrimaryButton(title: String(localized: "Done")) { dismiss() }
        } else {
            switch model.step {
            case .file:
                FormCancelButton { dismiss() }
                FormPrimaryButton(title: String(localized: "Choose File…")) { choosingFile = true }
            case .mapping:
                FormCancelButton { dismiss() }
                Button(String(localized: "Choose Another File…")) { choosingFile = true }
                    .buttonStyle(.chrome(.bordered))
                FormPrimaryButton(title: String(localized: "Preview")) { model.step = .review }
                    .disabled(model.preview == nil || model.previewProblem != nil)
            case .review:
                FormCancelButton(title: String(localized: "Back")) { model.step = .mapping }
                    .disabled(model.isImporting)
                FormPrimaryButton(title: String(localized: "Import \(model.counts.new) rows")) {
                    Task { await model.runImport() }
                }
                .disabled(model.counts.new == 0 || model.isImporting || model.isPreviewing || model.preview == nil)
            case .report:
                FormPrimaryButton(title: String(localized: "Done")) { dismiss() }
            }
        }
    }
}

// MARK: - Pieces

/// `1 File · 2 Columns · 3 Preview · 4 Report` under the title, over a
/// hairline: the page on screen in `text` with its number in the accent,
/// the others in `text3`, and the file's name on the right. A step the
/// model can reach (`StatementImportModel.isReachable`) is a button; one
/// ahead of the work done is plain text, so it cannot be clicked.
private struct StepStrip: View {
    let model: StatementImportModel

    private static let steps: [(StatementImportModel.Step, String)] = [
        (.file, String(localized: "File")),
        (.mapping, String(localized: "Columns")),
        (.review, String(localized: "Preview")),
        (.report, String(localized: "Report")),
    ]

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(Self.steps.enumerated()), id: \.offset) { index, entry in
                if index > 0 {
                    Text(verbatim: "\u{00B7}")
                        .foregroundStyle(Ink.text3)
                        .padding(.horizontal, 8)
                        .accessibilityHidden(true)
                }
                StepItem(
                    number: index + 1,
                    title: entry.1,
                    isCurrent: entry.0 == model.step,
                    isReachable: model.isReachable(entry.0)
                ) {
                    model.step = entry.0
                }
            }
            Spacer(minLength: 16)
            if let fileName = model.fileName {
                Text(fileName)
                    .font(Face.ui(11.5))
                    .foregroundStyle(Ink.text3)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .font(Face.ui(12, .medium))
        .padding(.bottom, 10)
        .overlay(alignment: .bottom) { Hairline() }
    }
}

/// One step of the strip. The pointer over a step that can be reached
/// lifts it to `text2`, the only hint that it is a button.
private struct StepItem: View {
    let number: Int
    let title: String
    let isCurrent: Bool
    let isReachable: Bool
    let go: () -> Void

    @State private var hovered = false

    var body: some View {
        if isReachable && !isCurrent {
            Button(action: go) { label }
                .buttonStyle(.plain)
                .onHover { hovered = $0 }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(title)
                .accessibilityAddTraits(.isButton)
        } else {
            label
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(title)
                .accessibilityAddTraits(isCurrent ? .isSelected : [])
        }
    }

    private var label: some View {
        HStack(spacing: 5) {
            Text(verbatim: String(number))
                .foregroundStyle(isCurrent ? Ink.accent : Ink.text3)
            Text(title)
                .foregroundStyle(isCurrent ? Ink.text : (hovered && isReachable ? Ink.text2 : Ink.text3))
        }
        .contentShape(Rectangle())
    }
}

/// A problem as every form sheet shows one: the headline beside the
/// caution triangle, the core's own words under it.
private struct ProblemNote: View {
    let problem: StatementImportProblem

    var body: some View {
        FormNote(
            problem.headline,
            tone: .warning,
            detail: problem.detail.isEmpty ? nil : problem.detail,
            indented: false
        )
    }
}

/// `To import 4 · Already imported 0 · Skipped 4 · Invalid 0 · Rounded 0`:
/// the preview's counts as the import will make them, drawn like the
/// window's status line (`StatusLine`), with the invalid rows in the
/// negative ink and the zeros dimmed.
private struct StatementCountsLine: View {
    let counts: StatementImportModel.Counts

    private var items: [(label: String, value: Int, tint: Color)] {
        [
            (String(localized: "To import"), counts.new, Ink.text),
            (String(localized: "Already imported"), counts.alreadyImported, Ink.text),
            (String(localized: "Skipped"), counts.skipped, Ink.text),
            (String(localized: "Invalid"), counts.invalid, Ink.negative),
            (String(localized: "Rounded"), counts.rounded, Ink.text),
        ]
    }

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                if index > 0 {
                    Text(verbatim: "\u{00B7}")
                        .padding(.horizontal, 7)
                        .accessibilityHidden(true)
                }
                Text(item.label)
                Text(verbatim: String(item.value))
                    .fontWeight(.medium)
                    .foregroundStyle(item.value == 0 ? Ink.text3 : item.tint)
                    .padding(.leading, 4)
            }
        }
        .font(Face.ui(11.5))
        .foregroundStyle(Ink.text3)
        .lineLimit(1)
        .fixedSize()
        // One sentence for VoiceOver, as the window's status line reads.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(StatusLine.spoken(items.map { StatusItem(label: $0.label, value: String($0.value)) }))
    }
}
