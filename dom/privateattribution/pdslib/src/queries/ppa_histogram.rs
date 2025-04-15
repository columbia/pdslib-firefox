use std::{
    collections::{HashMap, HashSet},
    vec,
};

use crate::{
    events::{
        hashmap_event_storage::VecEpochEvents, ppa_event::PpaEvent,
        traits::RelevantEventSelector,
    },
    queries::{
        histogram::{HistogramReport, HistogramRequest},
        traits::ReportRequestUris,
    },
};

pub struct PpaRelevantEventSelector {
    pub report_request_uris: ReportRequestUris<String>,
    pub is_matching_event: Box<dyn Fn(u64) -> bool>,
    pub bucket_intermediary_mapping: HashMap<usize, String>,
}

impl std::fmt::Debug for PpaRelevantEventSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PpaRelevantEventSelector")
            .field("report_request_uris", &self.report_request_uris)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct PpaHistogramConfig {
    pub start_epoch: usize,
    pub end_epoch: usize,
    pub report_global_sensitivity: f64,
    pub query_global_sensitivity: f64,
    pub requested_epsilon: f64,
    pub histogram_size: usize,
}

#[derive(Debug, Clone)]
pub enum AttributionLogic {
    LastTouch,
}

impl RelevantEventSelector for PpaRelevantEventSelector {
    type Event = PpaEvent;

    fn is_relevant_event(&self, event: &PpaEvent) -> bool {
        // Condition 1: Event's source URI should be in the allowed list by the
        // report request source URIs.
        let source_match = self
            .report_request_uris
            .source_uris
            .contains(&event.uris.source_uri);

        // Condition 2: Every querier URI from the report must be in the event’s
        // querier URIs. TODO: We might change Condition 2 eventually
        // when we support split reports, where one querier is
        // authorized but not others.
        let querier_match = self
            .report_request_uris
            .querier_uris
            .iter()
            .all(|uri| event.uris.querier_uris.contains(uri));

        // Condition 3: The report’s trigger URI should be allowed by the event
        // trigger URIs.
        let trigger_match = event
            .uris
            .trigger_uris
            .contains(&self.report_request_uris.trigger_uri);

        source_match
            && querier_match
            && trigger_match
            && (self.is_matching_event)(event.filter_data)
    }
}

#[derive(Debug)]
pub struct PpaHistogramRequest {
    start_epoch: usize,
    end_epoch: usize,
    report_global_sensitivity: f64,
    query_global_sensitivity: f64,
    requested_epsilon: f64,
    histogram_size: usize,
    filters: PpaRelevantEventSelector,
    logic: AttributionLogic,
}

impl PpaHistogramRequest {
    /// Constructs a new `PpaHistogramRequest`, validating that:
    /// - `requested_epsilon` is > 0.
    /// - `report_global_sensitivity` and `query_global_sensitivity` are
    ///   non-negative.
    pub fn new(
        config: PpaHistogramConfig,
        filters: PpaRelevantEventSelector,
    ) -> Result<Self, &'static str> {
        if config.requested_epsilon <= 0.0 {
            return Err("requested_epsilon must be greater than 0");
        }
        if config.report_global_sensitivity < 0.0
            || config.query_global_sensitivity < 0.0
        {
            return Err("sensitivity values must be non-negative");
        }
        if config.histogram_size == 0 {
            return Err("histogram_size must be greater than 0");
        }
        Ok(Self {
            start_epoch: config.start_epoch,
            end_epoch: config.end_epoch,
            report_global_sensitivity: config.report_global_sensitivity,
            query_global_sensitivity: config.query_global_sensitivity,
            requested_epsilon: config.requested_epsilon,
            histogram_size: config.histogram_size,
            filters,
            logic: AttributionLogic::LastTouch,
        })
    }

    pub fn get_bucket_intermediary_mapping(
        &self,
    ) -> &HashMap<usize, String> {
        &self.filters.bucket_intermediary_mapping
    }

    // Helper to check if a bucket is for a specific intermediary
    pub fn is_bucket_for_intermediary(
        &self,
        bucket_key: usize,
        intermediary_uri: &str,
    ) -> bool {
        match self
            .filters
            .bucket_intermediary_mapping
            .get(&bucket_key)
        {
            Some(intermediary) => intermediary == intermediary_uri,
            None => false,
        }
    }
}

impl HistogramRequest for PpaHistogramRequest {
    type EpochId = usize;
    type EpochEvents = VecEpochEvents<PpaEvent>;
    type Event = PpaEvent;
    type BucketKey = usize;
    type RelevantEventSelector = PpaRelevantEventSelector;

    fn epochs_ids(&self) -> Vec<Self::EpochId> {
        (self.start_epoch..=self.end_epoch).rev().collect()
    }

    fn query_global_sensitivity(&self) -> f64 {
        self.query_global_sensitivity
    }

    fn requested_epsilon(&self) -> f64 {
        self.requested_epsilon
    }

    fn laplace_noise_scale(&self) -> f64 {
        self.query_global_sensitivity / self.requested_epsilon
    }

    fn report_global_sensitivity(&self) -> f64 {
        self.report_global_sensitivity
    }

    fn relevant_event_selector(&self) -> &Self::RelevantEventSelector {
        &self.filters
    }

    fn bucket_key(&self, event: &PpaEvent) -> Self::BucketKey {
        // Bucket key validation.
        if event.histogram_index >= self.histogram_size {
            log::warn!(
                "Invalid bucket key {}: exceeds histogram size {}. Event id: {}",
                event.histogram_index,
                self.histogram_size,
                event.id
            );
        }

        event.histogram_index
    }

    fn event_values<'a>(
        &self,
        relevant_events_per_epoch: &'a HashMap<
            Self::EpochId,
            Self::EpochEvents,
        >,
    ) -> Vec<(&'a Self::Event, f64)> {
        let mut event_values = vec![];

        match self.logic {
            AttributionLogic::LastTouch => {
                for relevant_events in relevant_events_per_epoch.values() {
                    if let Some(last_impression) = relevant_events.last() {
                        if last_impression.histogram_index < self.histogram_size
                        {
                            event_values.push((
                                last_impression,
                                self.report_global_sensitivity,
                            ));
                        } else {
                            // Log error for dropped events
                            log::error!(
                                "Dropping event with id {} due to invalid bucket key {}",
                                last_impression.id,
                                last_impression.histogram_index
                            );
                        }
                    }
                }
            } // Other attribution logic not supported yet.
        }

        event_values
    }

    fn report_uris(&self) -> ReportRequestUris<String> {
        self.filters.report_request_uris.clone()
    }

    fn get_bucket_intermediary_mapping(
        &self,
    ) -> Option<&HashMap<usize, String>> {
        Some(&self.filters.bucket_intermediary_mapping)
    }

    fn filter_report_for_intermediary(
        &self,
        report: &HistogramReport<Self::BucketKey>,
        intermediary_uri: &str,
        _relevant_events_per_epoch: &HashMap<Self::EpochId, Self::EpochEvents>,
    ) -> Option<HistogramReport<Self::BucketKey>> {
        // Collect all usize keys whose value matches intermediary_uri
        let intermediary_buckets: HashSet<usize> = self
            .filters
            .bucket_intermediary_mapping
            .iter()
            .filter_map(|(bucket_id, uri)| {
                if uri == intermediary_uri {
                    Some(*bucket_id)
                } else {
                    None
                }
            })
            .collect();

        // If none matched, return None; otherwise, filter and return Some(...)
        if intermediary_buckets.is_empty() {
            None
        } else {
            let filtered_bins = filter_histogram_for_intermediary(
                &report.bin_values,
                &intermediary_buckets,
            );
            Some(HistogramReport {
                bin_values: filtered_bins,
            })
        }
    }
}

// Utility function to filter histogram
pub fn filter_histogram_for_intermediary<BK: std::hash::Hash + Eq + Clone>(
    full_histogram: &HashMap<BK, f64>,
    intermediary_buckets: &HashSet<BK>,
) -> HashMap<BK, f64> {
    full_histogram
        .iter()
        .filter_map(|(key, value)| {
            if intermediary_buckets.contains(key) {
                Some((key.clone(), *value))
            } else {
                None
            }
        })
        .collect()
}
