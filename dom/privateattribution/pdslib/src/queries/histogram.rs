use std::{collections::HashMap, fmt::Debug, hash::Hash};

use crate::{
    events::{
        relevant_events::RelevantEvents,
        traits::{EpochId, Event, Uri},
    },
    mechanisms::NormType,
    queries::traits::{QueryComputeResult, Report, ReportRequestUris},
};

#[derive(Debug, Clone)]
pub struct HistogramReport<BucketKey> {
    pub bin_values: HashMap<BucketKey, f64>,
}

/// Trait for bucket keys.
pub trait BucketKey: Debug + Hash + Eq + Clone {}

/// Default type for bucket keys.
impl BucketKey for u64 {}

/// Default histogram has no bins (null report).
impl<BK> Default for HistogramReport<BK> {
    fn default() -> Self {
        Self {
            bin_values: HashMap::new(),
        }
    }
}

impl<BK: BucketKey> Report for HistogramReport<BK> {}

/// [Experimental] Trait for generic histogram requests. Any type satisfying
/// this interface will be callable as a valid ReportRequest with the right
/// accounting. Following the formalism from https://arxiv.org/pdf/2405.16719, Thm 18.
/// Can be instantiated by PPA-style queries in particular.
pub trait HistogramRequest: Debug
where
    Self::BucketKey: Clone,
{
    type BucketKey: BucketKey;
    type HistogramEvent: Event;
    type HistogramEpochId: EpochId;
    type HistogramUri: Uri;

    /// Maximum value (sum) attributable to all the events in a single epoch,
    /// for this particular conversion. a.k.a. A^max.
    fn attributable_value(&self) -> f64;

    /// Returns the histogram bucket key (bin) for a given event.
    fn bucket_key(&self, event: &Self::HistogramEvent) -> Self::BucketKey;

    /// Attributes a value to each event in `relevant_events_per_epoch`, which
    /// will be obtained by retrieving *relevant* events from the event
    /// storage.
    ///
    /// Events with value 0 can be omitted since whey won't appear in the
    /// `compute_report` sum. Histogram buckets are padded with zeros at
    /// aggregation time.
    ///
    /// Events can point to the relevant_events_per_epoch, hence
    /// the lifetime. Returns an (ordered) vector of tuples (event, value),
    /// which will be browsed in order by `compute_histogram_report`. Ordering
    /// can depend on timestamp or other conditions (e.g. order of the vector of
    /// events for one epoch) in the implementation.
    fn event_values<'a>(
        &self,
        relevant_events: &'a RelevantEvents<Self::HistogramEvent>,
    ) -> Vec<(&'a Self::HistogramEvent, f64)>;

    fn histogram_report_uris(&self) -> ReportRequestUris<Self::HistogramUri>;

    /// Gets the querier bucket mapping for filtering histograms
    fn get_bucket_intermediary_mapping(
        &self,
    ) -> Option<&HashMap<u64, Self::HistogramUri>>;

    /// Filter a histogram for a specific querier
    fn filter_report_for_intermediary(
        &self,
        report: &HistogramReport<Self::BucketKey>,
        intermediary_uri: &Self::HistogramUri,
        _: &RelevantEvents<Self::HistogramEvent>,
    ) -> Option<HistogramReport<Self::BucketKey>>;

    /// Computes the report by attributing values to events, and then summing
    /// events by bucket.
    fn compute_histogram_report(
        &self,
        relevant_events: &RelevantEvents<Self::HistogramEvent>,
    ) -> QueryComputeResult<Self::HistogramUri, HistogramReport<Self::BucketKey>>
    {
        let mut bin_values: HashMap<Self::BucketKey, f64> = HashMap::new();

        let mut total_value: f64 = 0.0;
        let event_values = self.event_values(relevant_events);

        // The event_values function selects the relevant events and assigns
        // values according to the requested attribution logic, so we
        // should be able to aggregate values directly. For example, in the case
        // of LastTouch logic, only the last value for an event will be
        // selected, so the sum is juat that singular value.
        //
        // The order matters, since events that are attributed last might be
        // dropped by the contribution cap. `event_values` is in charge of
        // ordering the events from `relevant_events`.
        let mut report = HistogramReport {
            bin_values: HashMap::new(),
        };
        let mut early_stop = false;

        for (event, value) in event_values {
            total_value += value;
            if total_value > self.attributable_value() {
                // Return partial attribution to stay within the cap.
                early_stop = true;
                report = HistogramReport {
                    bin_values: bin_values.clone(),
                };
                break;
            }
            let bin = self.bucket_key(event);
            *bin_values.entry(bin).or_default() += value;
        }

        if !early_stop {
            report = HistogramReport { bin_values };
        }

        let mut site_to_report_mapping = HashMap::new();
        site_to_report_mapping.insert(
            self.histogram_report_uris().querier_uris[0].clone(),
            report.clone(),
        );

        for intermediary_uri in
            self.histogram_report_uris().intermediary_uris.iter()
        {
            match self.filter_report_for_intermediary(
                &report,
                intermediary_uri,
                relevant_events,
            ) {
                Some(filtered_report) => {
                    site_to_report_mapping
                        .insert(intermediary_uri.clone(), filtered_report);
                }
                None => {
                    site_to_report_mapping.insert(
                        intermediary_uri.clone(),
                        HistogramReport {
                            bin_values: HashMap::new(),
                        },
                    );
                }
            }
        }

        match self.get_bucket_intermediary_mapping() {
            Some(intermediary_mapping) => QueryComputeResult::new(
                intermediary_mapping.clone(),
                site_to_report_mapping,
            ),
            None => {
                QueryComputeResult::new(HashMap::new(), site_to_report_mapping)
            }
        }
    }

    /// Computes individual sensitivity in the single epoch case.
    fn histogram_single_epoch_individual_sensitivity(
        &self,
        report: &HistogramReport<Self::BucketKey>,
        norm_type: NormType,
    ) -> f64 {
        match norm_type {
            NormType::L1 => report.bin_values.values().sum(),
            NormType::L2 => {
                let sum_squares: f64 =
                    report.bin_values.values().map(|x| x * x).sum();
                sum_squares.sqrt()
            }
        }
    }

    /// Computes individual sensitivity in the single epoch-source case.
    fn histogram_single_epoch_source_individual_sensitivity(
        &self,
        report: &HistogramReport<Self::BucketKey>,
        norm_type: NormType,
    ) -> f64 {
        self.histogram_single_epoch_individual_sensitivity(report, norm_type)
    }

    /// Computes the global sensitivity, useful for the multi-epoch case.
    /// See https://arxiv.org/pdf/2405.16719, Thm. 18
    fn histogram_report_global_sensitivity(&self) -> f64 {
        // NOTE: if we have only one possible bin (histogram in R instead or
        // R^m), then we can remove the factor 2. But this constraint is
        // not enforceable with HashMap<BucketKey, f64>, so for
        // use-cases that have one bin we should use a custom type
        // similar to `SimpleLastTouchHistogramReport` with Option<BucketKey,
        // f64>.
        2.0 * self.attributable_value()
    }
}
