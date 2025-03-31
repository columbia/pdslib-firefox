#![allow(warnings)]

use libc::c_void;
use nserror::{nsresult, NS_ERROR_FAILURE, NS_OK};
use nsstring::{nsACString, nsAString, nsCString, nsString};
use std::{
    collections::HashSet,
    marker::PhantomData,
    ptr,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use thin_vec::ThinVec;
use xpcom::{nsIID, xpcom, xpcom_method, RefPtr};

#[xpcom(implement(nsIPrivateAttributionService), atomic)]
struct MyPrivateAttributionService {}

#[allow(non_snake_case)]
impl MyPrivateAttributionService {
    fn new() -> Result<RefPtr<Self>, ()> {
        log::warn!("MyPrivateAttributionService::new()");

        let this = Self::allocate(InitMyPrivateAttributionService {});
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
        log::warn!("MyPrivateAttributionService::on_attribution_event({sourceHost:?}, {ty:?}, {index}, {ad:?}, {targetHost:?})");

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
        log::warn!("MyPrivateAttributionService::on_attribution_conversion({targetHost:?}, {task:?}, {histogramSize}, {lookbackDays}, {impressionType:?}, {ads:?}, {sourceHosts:?})");

        Ok(())
    }
}

#[no_mangle]
pub unsafe extern "C" fn nsPrivateAttributionConstructor(
    iid: &nsIID,
    result: *mut *mut c_void,
) -> nsresult {
    log::warn!("nsPrivateAttributionConstructor(<iid>, {result:?})");

    let Ok(service) = MyPrivateAttributionService::new() else {
        return NS_ERROR_FAILURE;
    };

    service.QueryInterface(iid, result)
}
