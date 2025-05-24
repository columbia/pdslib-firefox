pub mod filter_storage;
pub mod uri;

use std::{collections::HashMap, ops::DerefMut, sync::Mutex};

use filter_storage::SqliteFilterStorage;
use libc::c_void;
use log::info;
use nserror::{nsresult, NS_ERROR_FAILURE, NS_OK};
use nsstring::{nsACString, nsCString};
use pdslib::{
    budget::{pure_dp_filter::PureDPBudget, traits::FilterStorage},
    events::{ppa_event::PpaEvent, relevant_events::RelevantEvents, traits::EventUris},
    pds::{
        aliases::PpaPdsCore,
        quotas::{FilterId, StaticCapacities},
    },
    queries::{
        ppa_histogram::{PpaHistogramConfig, PpaHistogramRequest, PpaRelevantEventSelector},
        traits::ReportRequestUris,
    },
};
use thin_vec::{thin_vec, ThinVec};
use uri::MozUri;
use xpcom::{
    interfaces::{PpaEvent as JsPpaEvent, PpaHistogramRequest as JsPpaHistogramRequest},
    nsIID, xpcom, xpcom_method, RefPtr,
};

#[xpcom(implement(nsIPrivateAttributionPdslibService), atomic)]
struct PdslibService {
    pdslib: Mutex<PpaPdsCore<SqliteFilterStorage, MozUri>>,
}

#[allow(non_snake_case)]
impl PdslibService {
    fn new() -> Result<RefPtr<Self>, ()> {
        info!("PdslibService::new");

        let capacities = Self::capacities();
        let filters = SqliteFilterStorage::new(capacities).unwrap();
        let pdslib = PpaPdsCore::new(filters);

        let this = Self::allocate(InitPdslibService {
            pdslib: Mutex::new(pdslib),
        });
        Ok(this)
    }

    fn capacities() -> StaticCapacities<FilterId<u64, MozUri>, PureDPBudget> {
        StaticCapacities::new(
            PureDPBudget::from(1.0),
            PureDPBudget::from(8.0),
            PureDPBudget::from(2.0),
            PureDPBudget::from(4.0),
        )
    }

    xpcom_method!(
        compute_report => ComputeReport(
            request: *const JsPpaHistogramRequest,
            events: *const ThinVec<Option<RefPtr<JsPpaEvent>>>
        ) -> ThinVec<f64>
    );

    fn compute_report(
        &self,
        request: &JsPpaHistogramRequest,
        events: &ThinVec<Option<RefPtr<JsPpaEvent>>>,
    ) -> Result<ThinVec<f64>, nsresult> {
        info!("PdslibService::compute_report");

        let histogram_size = get_attr(request, JsPpaHistogramRequest::GetHistogramSize)?;
        let config = PpaHistogramConfig {
            start_epoch: get_attr(request, JsPpaHistogramRequest::GetStartEpoch)?,
            end_epoch: get_attr(request, JsPpaHistogramRequest::GetEndEpoch)?,
            attributable_value: get_attr(request, JsPpaHistogramRequest::GetAttributableValue)?,
            max_attributable_value: get_attr(
                request,
                JsPpaHistogramRequest::GetMaxAttributableValue,
            )?,
            requested_epsilon: get_attr(request, JsPpaHistogramRequest::GetRequestedEpsilon)?,
            histogram_size,
        };

        let trigger_uri = get_attr_str(request, JsPpaHistogramRequest::GetTriggerHost)?;
        let uris = ReportRequestUris {
            trigger_uri: trigger_uri.clone(),
            source_uris: get_attr_vec(request, JsPpaHistogramRequest::GetSourceHosts)?,
            intermediary_uris: get_attr_vec(request, JsPpaHistogramRequest::GetIntermediaryHosts)?,
            querier_uris: get_attr_vec(request, JsPpaHistogramRequest::GetQuerierHosts)?,
        };

        // create dummy RelevantEventSelector (not used in pdslib core.rs)
        let selector = PpaRelevantEventSelector {
            report_request_uris: uris,
            is_matching_event: Box::new(|_| true),
            bucket_intermediary_mapping: HashMap::new(),
        };

        let request = PpaHistogramRequest::new(config, selector).map_err(|_| NS_ERROR_FAILURE)?;

        let mut relevant_events_map = HashMap::new();
        for event in events {
            let event = event.as_ref().ok_or(NS_ERROR_FAILURE)?;
            let event = xpidl_to_pdslib_event(event)?;
            relevant_events_map
                .entry(event.epoch_number)
                .or_insert_with(Vec::new)
                .push(event);
        }
        let relevant_events = RelevantEvents::from_mapping(relevant_events_map);

        let mut pdslib = self.pdslib.lock().map_err(|_| NS_ERROR_FAILURE)?;
        let report = pdslib.compute_report(&request, relevant_events).unwrap();
        let report = &report[&trigger_uri];

        // create histogram from report
        let mut histogram = thin_vec![0.0; histogram_size as usize];
        for (bin, value) in &report.filtered_report.bin_values {
            if *bin < histogram_size {
                histogram[*bin as usize] = *value;
            }
        }

        Ok(histogram)
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
        log::info!("getBudget(filterType={filter_type}, epochId={epoch_id}, uri={uri})");

        let uri = MozUri(uri.into());

        let filter_id: FilterId<u64, MozUri> = match filter_type.to_utf8().as_ref() {
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

        let budget = pdslib.filter_storage.remaining_budget(&filter_id).unwrap();

        log::info!("Budget for filter {filter_id:?}: {budget}");
        return Ok(budget);
    }

    xpcom_method!(
        clear_budgets => ClearBudgets()
    );

    fn clear_budgets(&self) -> Result<(), nsresult> {
        log::info!("clearBudgets()");

        let pdslib = self.pdslib.lock().unwrap();
        pdslib.filter_storage.clear_db().unwrap();

        log::info!("Successfully cleared budgets");
        Ok(())
    }
}

fn get_attr<R, O>(request: &R, getter: unsafe fn(&R, *mut O) -> nsresult) -> Result<O, nsresult> {
    let mut var: O = unsafe { std::mem::zeroed() };
    let rv = unsafe { getter(request, &mut var) };
    if rv != NS_OK {
        return Err(rv);
    }
    Ok(var)
}

fn get_attr_str<R>(
    request: &R,
    getter: unsafe fn(&R, *mut nsACString) -> nsresult,
) -> Result<MozUri, nsresult> {
    let mut str = nsCString::new();
    let rv = unsafe { getter(request, str.deref_mut()) };
    if rv != NS_OK {
        return Err(rv);
    }
    Ok(MozUri(str))
}

fn get_attr_vec<R, V: FromIterator<MozUri>>(
    request: &R,
    getter: unsafe fn(&R, *mut ThinVec<nsCString>) -> nsresult,
) -> Result<V, nsresult> {
    let mut vec: ThinVec<nsCString> = ThinVec::new();
    let rv = unsafe { getter(request, &mut vec as *mut _) };
    if rv != NS_OK {
        return Err(rv);
    }
    let vec: V = vec.into_iter().map(MozUri).collect();
    Ok(vec)
}

fn xpidl_to_pdslib_event(event: &JsPpaEvent) -> Result<PpaEvent<MozUri>, nsresult> {
    let uris = EventUris {
        source_uri: get_attr_str(event, JsPpaEvent::GetSourceHost)?,
        trigger_uris: get_attr_vec(event, JsPpaEvent::GetTriggerHosts)?,
        intermediary_uris: get_attr_vec(event, JsPpaEvent::GetIntermediaryHosts)?,
        querier_uris: get_attr_vec(event, JsPpaEvent::GetQuerierHosts)?,
    };
    let event = PpaEvent {
        timestamp: get_attr(event, JsPpaEvent::GetTimestamp)?,
        epoch_number: get_attr(event, JsPpaEvent::GetEpochNumber)?,
        histogram_index: get_attr(event, JsPpaEvent::GetHistogramIndex)?,
        uris,
        // unused fields:
        id: 0,
        filter_data: 0,
    };
    Ok(event)
}

#[no_mangle]
pub unsafe extern "C" fn nsPrivateAttributionPdslibConstructor(
    iid: &nsIID,
    result: *mut *mut c_void,
) -> nsresult {
    info!("nsPrivateAttributionPdslibConstructor");

    let service = match PdslibService::new() {
        Ok(service) => service,
        Err(_) => return NS_ERROR_FAILURE,
    };

    service.QueryInterface(iid, result)
}
