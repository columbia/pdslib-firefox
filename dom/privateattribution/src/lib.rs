#![allow(warnings)]

pub mod epoch;
pub mod filter_data;

use epoch::{days_ago_to_epoch, epoch_now, timestamp_now, timestamp_to_epoch};
use filter_data::{ad_hash, filter_data};
use libc::c_void;
use nserror::{nsresult, NS_ERROR_FAILURE, NS_OK};
use nsstring::{nsACString, nsAString, nsCString, nsString};
use pdslib::{
    budget::{
        hashmap_filter_storage::HashMapFilterStorage,
        pure_dp_filter::{PureDPBudget, PureDPBudgetFilter},
        traits::FilterStorage as _,
    },
    events::{hashmap_event_storage::HashMapEventStorage, ppa_event::PpaEvent, traits::EventUris},
    pds::epoch_pds::{EpochPrivateDataService, FilterId, StaticCapacities},
    queries::{
        ppa_histogram::{
            AttributionLogic, PpaHistogramConfig, PpaHistogramRequest, PpaRelevantEventSelector,
        },
        traits::ReportRequestUris,
    },
};
use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    ops::Deref as _,
    ptr,
    sync::{atomic::AtomicBool, mpsc, Arc, Mutex},
};
use thin_vec::ThinVec;
use xpcom::{nsIID, xpcom, xpcom_method, RefPtr};

type Pdslib = EpochPrivateDataService<
    HashMapFilterStorage<
        FilterId<usize, String>,
        PureDPBudgetFilter,
        PureDPBudget,
        StaticCapacities<FilterId<usize, String>, PureDPBudget>,
    >,
    HashMapEventStorage<PpaEvent, PpaRelevantEventSelector>,
    PpaHistogramRequest,
    anyhow::Error,
>;

#[xpcom(implement(nsIPrivateAttributionService), atomic)]
struct MyPrivateAttributionService {
    pdslib: Mutex<Pdslib>,
}

#[allow(non_snake_case)]
impl MyPrivateAttributionService {
    fn new() -> Result<RefPtr<Self>, ()> {
        let capacities: StaticCapacities<FilterId<usize, String>, PureDPBudget> =
            StaticCapacities::mock();
        let filters: HashMapFilterStorage<_, PureDPBudgetFilter, _, _> =
            HashMapFilterStorage::new(capacities).unwrap();
        let events: HashMapEventStorage<_, PpaRelevantEventSelector> = HashMapEventStorage::new();

        let pdslib = EpochPrivateDataService {
            filter_storage: filters,
            event_storage: events,
            _phantom_request: PhantomData,
            _phantom_error: PhantomData,
        };
        let pdslib = Mutex::new(pdslib);

        let this = Self::allocate(InitMyPrivateAttributionService { pdslib });
        Ok(this)
    }

    xpcom_method!(
        on_attribution_event => OnAttributionEvent(
            sourceHost: *const nsACString,
            ty: *const nsACString,
            index: u32,
            ad: *const nsAString,
            targetHost: *const nsACString
        )
    );

    fn on_attribution_event(
        &self,
        sourceHost: &nsACString,
        ty: &nsACString,
        index: u32,
        ad: &nsAString,
        targetHost: &nsACString,
    ) -> Result<(), nsresult> {
        log::trace!("onAttributionEvent(sourceHost={sourceHost}, ty={ty}, index={index}, ad={ad}, targetHost={targetHost})");

        /// todo: converting to String does a copy, we should use nsACString instead
        let source_uri = sourceHost.to_string();
        let target_uri = targetHost.to_string();

        let now = timestamp_now();
        let ad = ad.to_string();
        let uris = EventUris {
            source_uri,
            trigger_uris: vec![target_uri.clone()],
            intermediary_uris: vec![],
            querier_uris: vec![target_uri],
        };

        let event = PpaEvent {
            id: 1, // unused
            timestamp: now,
            epoch_number: timestamp_to_epoch(now),
            histogram_index: index as usize,
            uris,
            filter_data: filter_data(ad, index),
        };

        log::trace!("Registering event: {:?}", event);

        let mut pdslib = self.pdslib.lock().unwrap();
        pdslib.register_event(event).unwrap();

        Ok(())
    }

    xpcom_method!(
        on_attribution_conversion => OnAttributionConversion(
            targetHost: *const nsACString,
            task: *const nsAString,
            histogramSize: u32,
            lookbackDays: u32,
            impressionType: *const nsACString,
            ads: *const ThinVec<nsString>,
            sourceHosts: *const ThinVec<nsCString>
        )
    );

    fn on_attribution_conversion(
        &self,
        targetHost: &nsACString,
        task: &nsAString,
        histogramSize: u32,
        lookbackDays: u32,
        impressionType: &nsACString,
        ads: &ThinVec<nsString>,
        sourceHosts: &ThinVec<nsCString>,
    ) -> Result<(), nsresult> {
        log::trace!(
            "onAttributionConversion(targetHost={targetHost}, task={task}, histogramSize={histogramSize}, lookbackDays={lookbackDays}, impressionType={impressionType}, ads={ads:?}, sourceHosts={sourceHosts:?})",
        );

        let target_host = targetHost.to_string();
        let source_hosts = sourceHosts
            .iter()
            .map(|host| host.to_string())
            .collect::<Vec<_>>();

        let start_epoch = days_ago_to_epoch(lookbackDays as usize);
        let end_epoch = epoch_now();

        let uris = ReportRequestUris {
            trigger_uri: target_host.clone(),
            source_uris: source_hosts,
            intermediary_uris: vec![],
            querier_uris: vec![target_host],
        };

        let ad_hashes: HashSet<u32> = ads
            .iter()
            .map(|ad| ad_hash(ad.to_string()) as u32)
            .collect();
        let is_matching_event = move |filter_data: u64| {
            let ad_hash = (filter_data >> 32) as u32;
            ad_hashes.contains(&ad_hash)
        };

        let request_config = PpaHistogramConfig {
            start_epoch,
            end_epoch,
            // using values from ara_demo.rs
            report_global_sensitivity: 32768.0,
            query_global_sensitivity: 65536.0,
            requested_epsilon: 1.0,
            histogram_size: histogramSize as usize,
        };
        let mut request = PpaHistogramRequest::new(
            request_config,
            PpaRelevantEventSelector {
                report_request_uris: uris,
                is_matching_event: Box::new(is_matching_event),
                bucket_intermediary_mapping: HashMap::new(),
            },
        )
        .unwrap();

        let mut pdslib = self.pdslib.lock().unwrap();
        let report = pdslib.compute_report(&mut request).unwrap();
        drop(pdslib);

        for (uri, report) in report {
            log::debug!("Report for Uri {uri}:");
            log::debug!("{:?}", report);
        }

        Ok(())
    }

    xpcom_method!(
        get_budget => GetBudget(
            filterType: *const nsACString,
            epochId: u64,
            uri: *const nsACString
        ) -> f64
    );

    fn get_budget(
        &self,
        filter_type: &nsACString,
        epoch_id: u64,
        uri: &nsACString,
    ) -> Result<f64, nsresult> {
        log::trace!("getBudget(filterType={filter_type}, epochId={epoch_id}, uri={uri})");

        let filter_type = filter_type.to_string();
        let epoch_id = epoch_id as usize;
        let uri = uri.to_string();

        let filter_id: FilterId<usize, String> = match filter_type.as_str() {
            "Nc" => FilterId::Nc(epoch_id, uri),
            "C" => FilterId::C(epoch_id),
            "QTrigger" => FilterId::QTrigger(epoch_id, uri),
            "QSource" => FilterId::QSource(epoch_id, uri),
            _ => {
                log::warn!("Unknown filter type: {}", filter_type);
                return Err(NS_ERROR_FAILURE);
            }
        };

        let mut pdslib = self.pdslib.lock().unwrap();
        let budget = pdslib
            .filter_storage
            .remaining_budget(&filter_id)
            .map(|budget| match budget {
                PureDPBudget::Infinite => f64::NAN,
                PureDPBudget::Epsilon(epsilon) => epsilon,
            })
            .unwrap_or(0.0);

        return Ok(budget);
    }
}

#[no_mangle]
pub unsafe extern "C" fn nsPrivateAttributionConstructor(
    iid: &nsIID,
    result: *mut *mut c_void,
) -> nsresult {
    log::trace!("nsPrivateAttributionConstructor");

    let service = match MyPrivateAttributionService::new() {
        Ok(service) => service,
        Err(_) => return NS_ERROR_FAILURE,
    };

    service.QueryInterface(iid, result)
}
