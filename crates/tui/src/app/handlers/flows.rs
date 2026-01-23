use super::super::*;

impl App {
    pub(crate) fn flows_select_next(&mut self) {
        let len = flows_visible_indices(&self.state).len();
        if len == 0 {
            return;
        }
        self.state.flows.selected = (self.state.flows.selected + 1).min(len - 1);
    }

    pub(crate) fn flows_select_prev(&mut self) {
        if flows_visible_indices(&self.state).is_empty() {
            return;
        }
        self.state.flows.selected = self.state.flows.selected.saturating_sub(1);
    }
    pub(crate) fn start_flow_create(&mut self) {
        self.reset_flow_form();
        self.state.flows.mode = FlowsMode::Create;
    }

    pub(crate) fn start_flow_rename(&mut self) {
        let Some((name, is_unallocated)) = self
            .selected_flow()
            .map(|flow| (flow.name.clone(), flow.is_unallocated))
        else {
            self.state.flows.error = Some("Nessun flow selezionato.".to_string());
            return;
        };
        if is_unallocated {
            self.state.flows.error = Some("Unallocated non si può rinominare.".to_string());
            return;
        }
        self.reset_flow_form();
        self.state.flows.form.name = name;
        self.state.flows.mode = FlowsMode::Rename;
        self.state.flows.form.focus = FlowFormField::Name;
    }
    pub(crate) fn cycle_flow_mode(&mut self) {
        self.state.flows.form.mode = match self.state.flows.form.mode {
            FlowModeChoice::Unlimited => FlowModeChoice::NetCapped,
            FlowModeChoice::NetCapped => FlowModeChoice::IncomeCapped,
            FlowModeChoice::IncomeCapped => FlowModeChoice::Unlimited,
        };
    }
    pub(crate) fn selected_flow(&self) -> Option<&api_types::vault::FlowView> {
        let indices = flows_visible_indices(&self.state);
        let index = indices.get(self.state.flows.selected).copied()?;
        self.state
            .snapshot
            .as_ref()
            .and_then(|snap| snap.flows.get(index))
    }

    pub(crate) fn select_flow_by_id(&mut self, flow_id: uuid::Uuid) {
        let Some(snapshot) = &self.state.snapshot else {
            return;
        };
        let indices = flows_visible_indices(&self.state);
        if let Some(pos) = indices.iter().position(|idx| {
            snapshot
                .flows
                .get(*idx)
                .map(|flow| flow.id == flow_id)
                .unwrap_or(false)
        }) {
            self.state.flows.selected = pos;
        }
    }
}
