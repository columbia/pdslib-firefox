use std::collections::{HashMap, HashSet};

use super::traits::{Event, EventStorage, RelevantEventSelector};

/// A struct that holds relevant events for a set of epochs.
///
/// Can be constructed either from an `EventStorage`, or directly from a
/// mapping of relevant events per epoch.
#[derive(Debug, Clone)]
pub struct RelevantEvents<E: Event> {
    pub events_per_epoch: HashMap<E::EpochId, Vec<E>>,
}

impl<E: Event> RelevantEvents<E> {
    /// Fetches and filters relevant events from the given event storage,
    /// for the specified epochs.
    pub fn from_event_storage<ES>(
        event_storage: &mut ES,
        epoch_ids: &[E::EpochId],
        selector: &impl RelevantEventSelector<Event = E>,
    ) -> Result<Self, ES::Error>
    where
        ES: EventStorage<Event = E>,
    {
        let mut events_per_epoch = HashMap::new();

        for epoch_id in epoch_ids {
            // fetch all events at that epoch from storage
            let events = event_storage
                .events_for_epoch(epoch_id)?
                // filter relevant events using the selector
                .filter(|event| selector.is_relevant_event(event))
                .collect();

            // store the events in the map
            events_per_epoch.insert(*epoch_id, events);
        }

        let this = Self::from_mapping(events_per_epoch);
        Ok(this)
    }

    /// Constructs a `RelevantEvents` instance directly from a mapping of
    /// epochs, to relevant events for each of those epochs.
    pub fn from_mapping(events_per_epoch: HashMap<E::EpochId, Vec<E>>) -> Self {
        Self { events_per_epoch }
    }

    /// Get the relevant events for a specific epoch.
    pub fn for_epoch(&self, epoch_id: &E::EpochId) -> &[E] {
        self.events_per_epoch
            .get(epoch_id)
            .map(|events| events.as_slice())
            .unwrap_or_default()
    }

    /// Get the set of unique source URIs that have at least one relevant event
    /// in the given epoch.
    pub fn sources_for_epoch(&self, epoch_id: &E::EpochId) -> HashSet<&E::Uri> {
        let events_for_epoch = self.for_epoch(epoch_id);

        // collect unique source URIs for the given epoch
        events_for_epoch
            .iter()
            .map(|event| &event.event_uris().source_uri)
            .collect::<HashSet<&E::Uri>>()
    }

    /// Drop and forget the given epoch and all its events.
    pub fn drop_epoch(&mut self, epoch_id: &E::EpochId) {
        self.events_per_epoch.remove(epoch_id);
    }
}
