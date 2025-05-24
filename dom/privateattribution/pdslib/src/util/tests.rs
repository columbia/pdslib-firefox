use crate::{
    budget::pure_dp_filter::PureDPBudget, events::traits::EventUris,
    pds::quotas::StaticCapacities, queries::traits::ReportRequestUris,
};

// Sample mock values to reduce boilerplate in tests.

impl<FID> StaticCapacities<FID, PureDPBudget> {
    /// Sample capacitiy values for testing.
    pub fn mock() -> Self {
        Self::new(
            PureDPBudget::from(1.0),
            PureDPBudget::from(20.0),
            PureDPBudget::from(1.5),
            PureDPBudget::from(4.0),
        )
    }
}

impl EventUris<String> {
    /// Sample URIs for testing.
    pub fn mock() -> Self {
        Self {
            source_uri: "blog.com".to_string(),
            trigger_uris: vec!["shoes.com".to_string()],
            intermediary_uris: vec![
                "search.engine".to_string(),
                "social.media".to_string(),
            ],
            querier_uris: vec![
                "shoes.com".to_string(),
                "adtech.com".to_string(),
            ],
        }
    }
}

impl ReportRequestUris<String> {
    /// Sample URIs for testing.
    pub fn mock() -> Self {
        Self {
            trigger_uri: "shoes.com".to_string(),
            source_uris: vec!["blog.com".to_string()],
            intermediary_uris: vec![
                "search.engine".to_string(),
                "social.media".to_string(),
            ],
            querier_uris: vec!["adtech.com".to_string()],
        }
    }
}
