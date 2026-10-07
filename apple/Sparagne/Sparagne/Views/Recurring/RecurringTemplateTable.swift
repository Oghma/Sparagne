import SwiftUI
import SparagneCore

/// What the Ricorrenze tab's inspector shows: a saved template, or a new one
/// being written.
enum RecurringSelection: Hashable {
    case template(Uuid)
    case new
}

/// Column geometry of "Modelli", shared by the heading and the rows. The
/// three text columns take what is left, down to their minimums, which fit
/// the window's narrowest width beside the inspector.
private enum TemplateColumn {
    /// Wide enough for "Enabled", which is longer than the canvas's
    /// "Attiva".
    static let enabled: CGFloat = 60
    static let descriptionMinimum: CGFloat = 120
    static let amount: CGFloat = 96
    static let cadenceMinimum: CGFloat = 150
    static let next: CGFloat = 100
    static let wallet: CGFloat = 84
    static let envelope: CGFloat = 92
    static let categoryMinimum: CGFloat = 90
    static let padding: CGFloat = 8
}

/// "Modelli" (`docs/v2/UI.md` §2.5): every template as a 26-point row,
/// archived ones at the bottom, dimmed, with Ripristina. A click selects a
/// row and the inspector edits it; the switch pauses or resumes it in place;
/// the last row starts a new one.
struct RecurringTemplateTable: View {
    let store: AppStore
    let today: NaiveDate
    /// Prossima per template, from the tab (`AppStore.nextRecurring`): a row
    /// only reads it, since a hover draws every row again.
    let next: [Uuid: RecurringNext]
    /// What the inspector shows, as the tab resolved it.
    let selection: RecurringSelection?
    let select: (RecurringSelection) -> Void

    @State private var hovered: RecurringSelection?

    var body: some View {
        let names = NameBook(snapshot: store.snapshot)
        Panel(padding: 0) {
            VStack(alignment: .leading, spacing: 0) {
                RecurringCardHeader(title: String(localized: "Templates")) {
                    Text(String(localized: "click a row to edit it"))
                        .font(Face.ui(11.5, .medium))
                        .foregroundStyle(Ink.text3)
                }
                .padding(.horizontal, 12)
                .padding(.top, 10)
                header
                ForEach(store.recurringTemplatesInTableOrder, id: \.id) { template in
                    row(template, names: names)
                }
                // A viewer reads the templates; nothing to add.
                if store.canWrite { newRow }
            }
        }
    }

    // MARK: - Heading

    private var header: some View {
        HStack(spacing: 0) {
            cell(width: TemplateColumn.enabled) { Text(String(localized: "Enabled")) }
            cell(minWidth: TemplateColumn.descriptionMinimum) { Text(String(localized: "Description")) }
            cell(width: TemplateColumn.amount, alignment: .trailing) { Text(String(localized: "Amount")) }
            cell(minWidth: TemplateColumn.cadenceMinimum) { Text(String(localized: "Cadence")) }
            cell(width: TemplateColumn.next) { Text(String(localized: "Next")) }
            cell(width: TemplateColumn.wallet) { Text(String(localized: "Wallet")) }
            cell(width: TemplateColumn.envelope) { Text(String(localized: "Envelope")) }
            cell(minWidth: TemplateColumn.categoryMinimum) { Text(String(localized: "Category")) }
        }
        .font(Face.ui(11, .medium))
        .foregroundStyle(Ink.text3)
        .padding(.horizontal, 4)
        .frame(height: Metrics.headerHeight)
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.line2).frame(height: 1) }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }

    // MARK: - A template

    private func row(_ template: RecurringView, names: NameBook) -> some View {
        let key = RecurringSelection.template(template.id)
        let isSelected = selection == key
        // Archived reads as background; paused, one step back from running.
        let ink = template.archived ? Ink.text3 : template.enabled ? Ink.text : Ink.text2
        let side = template.archived ? Ink.text3 : Ink.text2
        return HStack(spacing: 0) {
            cell(width: TemplateColumn.enabled) {
                RecurringSwitch(
                    label: String(localized: "Enabled \(RecurringTitle.of(template))"),
                    isOn: template.enabled && !template.archived
                ) { on in
                    Task { await store.setRecurringEnabled(template.id, on) }
                }
                .disabled(!store.canWrite || template.archived)
            }
            cell(minWidth: TemplateColumn.descriptionMinimum) {
                Text(RecurringTitle.of(template)).foregroundStyle(ink)
            }
            cell(width: TemplateColumn.amount, alignment: .trailing) {
                Text(RecurringAmount.text(template))
                    .foregroundStyle(template.archived ? Ink.text3 : template.kind == .income ? Ink.positive : ink)
                    .accessibilityLabel("\(RecurringAmount.text(template)), \(DueRecurringText.kind(template.kind))")
            }
            cell(minWidth: TemplateColumn.cadenceMinimum) {
                Text(ScheduleFormatting.describe(template.schedule)).foregroundStyle(side)
            }
            cell(width: TemplateColumn.next) { nextCell(template, ink: ink) }
            cell(width: TemplateColumn.wallet) {
                Text(DueRecurringText.wallet(template, names: names)).foregroundStyle(side)
            }
            cell(width: TemplateColumn.envelope) {
                Text(DueRecurringText.envelope(template, names: names)).foregroundStyle(side)
            }
            cell(minWidth: TemplateColumn.categoryMinimum) {
                Text(template.category.flatMap { $0.isEmpty ? nil : $0 } ?? TransactionRow.placeholder)
                    .foregroundStyle(side)
            }
        }
        .font(Face.ui(12))
        .padding(.horizontal, 4)
        .frame(height: 26)
        .background(background(key, isSelected: isSelected))
        .overlay(alignment: .bottom) { Rectangle().fill(Ink.rowLine).frame(height: 1) }
        .contentShape(Rectangle())
        .onTapGesture { select(key) }
        .onHover { inside in hover(key, inside) }
        .accessibilityElement(children: .contain)
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
        .accessibilityAction(named: String(localized: "Edit")) { select(key) }
        .contextMenu {
            if store.canWrite {
                if template.archived {
                    Button(String(localized: "Restore")) {
                        Task { await store.restoreRecurring(template.id) }
                    }
                } else {
                    Button(String(localized: "Archive"), role: .destructive) {
                        Task { await store.archiveRecurring(template.id) }
                    }
                }
            }
        }
    }

    /// Prossima: the oldest due period in red, or the next date; for an
    /// archived template, the way back. Empty for the moment between a new
    /// template reaching the list and the tab working its date out.
    @ViewBuilder
    private func nextCell(_ template: RecurringView, ink: Color) -> some View {
        switch next[template.id] {
        case nil:
            EmptyView()
        case .due(let day):
            Text(String(localized: "\(RecurringDayText.short(day, today: today)) \u{00B7} due"))
                .foregroundStyle(Ink.negative)
        case .upcoming(let day):
            Text(RecurringDayText.short(day, today: today)).foregroundStyle(ink)
        case .finished:
            Text(String(localized: "ended")).foregroundStyle(Ink.text3)
        case .archived:
            if store.canWrite {
                Button(String(localized: "Restore")) {
                    Task { await store.restoreRecurring(template.id) }
                }
                .buttonStyle(.chrome(.ghost, small: true))
                .padding(.leading, -8)
            } else {
                Text(String(localized: "archived")).foregroundStyle(Ink.text3)
            }
        }
    }

    // MARK: - The last row

    /// "Nuova ricorrenza…": the inspector in create mode. The cells show what
    /// a new template starts from.
    private var newRow: some View {
        let key = RecurringSelection.new
        return HStack(spacing: 0) {
            cell(width: TemplateColumn.enabled, alignment: .center) {
                Image(systemName: "plus")
                    .font(.system(size: 10, weight: .bold))
                    .accessibilityHidden(true)
            }
            cell(minWidth: TemplateColumn.descriptionMinimum) {
                Text(String(localized: "New recurring entry\u{2026}"))
            }
            cell(width: TemplateColumn.amount, alignment: .trailing) { Text(LedgerMoney.bare(0)) }
            cell(minWidth: TemplateColumn.cadenceMinimum) { Text(ScheduleFormatting.every(1, .month)) }
            cell(width: TemplateColumn.next) { EmptyView() }
            cell(width: TemplateColumn.wallet) { EmptyView() }
            cell(width: TemplateColumn.envelope) { EmptyView() }
            cell(minWidth: TemplateColumn.categoryMinimum) { EmptyView() }
        }
        .font(Face.ui(12))
        .foregroundStyle(selection == key ? Ink.text2 : Ink.text3)
        .padding(.horizontal, 4)
        .frame(height: 26)
        .background(background(key, isSelected: selection == key))
        .contentShape(Rectangle())
        .onTapGesture { select(key) }
        .onHover { inside in hover(key, inside) }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "New recurring entry\u{2026}"))
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { select(key) }
    }

    // MARK: - Pieces

    private func background(_ key: RecurringSelection, isSelected: Bool) -> Color {
        if isSelected { return Ink.accent.opacity(0.14) }
        return hovered == key ? Ink.raised : .clear
    }

    private func hover(_ key: RecurringSelection, _ inside: Bool) {
        if inside {
            hovered = key
        } else if hovered == key {
            hovered = nil
        }
    }

    private func cell<Content: View>(
        width: CGFloat? = nil,
        minWidth: CGFloat? = nil,
        alignment: Alignment = .leading,
        @ViewBuilder content: () -> Content
    ) -> some View {
        content()
            .lineLimit(1)
            .truncationMode(.tail)
            .padding(.horizontal, TemplateColumn.padding)
            .frame(
                minWidth: width ?? minWidth,
                idealWidth: width,
                maxWidth: width ?? .infinity,
                alignment: alignment
            )
    }
}
