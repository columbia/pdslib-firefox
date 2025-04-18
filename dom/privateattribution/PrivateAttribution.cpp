/* -*- Mode: C++; tab-width: 2; indent-tabs-mode: nil; c-basic-offset: 2 -*- */
/* vim:set ts=2 sw=2 sts=2 et cindent: */
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "PrivateAttribution.h"
#include "mozilla/dom/BindingUtils.h"
#include "mozilla/dom/ContentChild.h"
#include "mozilla/dom/PrivateAttributionBinding.h"
#include "mozilla/Components.h"
#include "mozilla/StaticPrefs_datareporting.h"
#include "nsIGlobalObject.h"
#include "nsIPrivateAttributionService.h"
#include "nsXULAppAPI.h"
#include "nsURLHelper.h"

namespace mozilla::dom {

NS_IMPL_CYCLE_COLLECTION_WRAPPERCACHE(PrivateAttribution, mOwner)

PrivateAttribution::PrivateAttribution(nsIGlobalObject* aGlobal)
    : mOwner(aGlobal) {
  MOZ_ASSERT(aGlobal);
}

JSObject* PrivateAttribution::WrapObject(JSContext* aCx,
                                         JS::Handle<JSObject*> aGivenProto) {
  return PrivateAttribution_Binding::Wrap(aCx, this, aGivenProto);
}

PrivateAttribution::~PrivateAttribution() = default;

bool PrivateAttribution::ShouldRecord() {
#ifdef MOZ_TELEMETRY_REPORTING
  return (StaticPrefs::dom_private_attribution_submission_enabled() &&
          StaticPrefs::datareporting_healthreport_uploadEnabled());
#else
  return false;
#endif
}

bool PrivateAttribution::GetSourceHostIfNonPrivate(nsACString& aSourceHost,
                                                   ErrorResult& aRv) {
  MOZ_ASSERT(mOwner);
  nsIPrincipal* prin = mOwner->PrincipalOrNull();
  if (!prin || NS_FAILED(prin->GetHost(aSourceHost))) {
    aRv.ThrowInvalidStateError("Couldn't get source host");
    return false;
  }
  return !prin->GetIsInPrivateBrowsing();
}

[[nodiscard]] static bool ValidateHost(const nsACString& aHost,
                                       ErrorResult& aRv) {
  if (!net_IsValidDNSHost(aHost)) {
    aRv.ThrowSyntaxError(aHost + " is not a valid host name"_ns);
    return false;
  }
  return true;
}

void PrivateAttribution::SaveImpression(
    const PrivateAttributionImpressionOptions& aOptions, ErrorResult& aRv) {
  nsAutoCString source;
  if (!GetSourceHostIfNonPrivate(source, aRv)) {
    return;
  }

  if (!ValidateHost(aOptions.mTarget, aRv)) {
    return;
  }

  if (!ShouldRecord()) {
    return;
  }

  if (XRE_IsParentProcess()) {
    nsCOMPtr<nsIPrivateAttributionService> pa =
        components::PrivateAttribution::Service();
    if (NS_WARN_IF(!pa)) {
      return;
    }
    pa->OnAttributionEvent(source, GetEnumString(aOptions.mType),
                           aOptions.mIndex, aOptions.mAd, aOptions.mTarget);
    return;
  }

  auto* content = ContentChild::GetSingleton();
  if (NS_WARN_IF(!content)) {
    return;
  }
  content->SendAttributionEvent(source, aOptions.mType, aOptions.mIndex,
                                aOptions.mAd, aOptions.mTarget);
}

void PrivateAttribution::MeasureConversion(
    const PrivateAttributionConversionOptions& aOptions, ErrorResult& aRv) {
  nsAutoCString source;
  if (!GetSourceHostIfNonPrivate(source, aRv)) {
    return;
  }
  for (const nsACString& host : aOptions.mSources) {
    if (!ValidateHost(host, aRv)) {
      return;
    }
  }

  if (!ShouldRecord()) {
    return;
  }

  if (XRE_IsParentProcess()) {
    nsCOMPtr<nsIPrivateAttributionService> pa =
        components::PrivateAttribution::Service();
    if (NS_WARN_IF(!pa)) {
      return;
    }
    pa->OnAttributionConversion(
        source, aOptions.mTask, aOptions.mHistogramSize,
        aOptions.mLookbackDays.WasPassed() ? aOptions.mLookbackDays.Value() : 0,
        aOptions.mImpression.WasPassed()
            ? GetEnumString(aOptions.mImpression.Value())
            : EmptyCString(),
        aOptions.mAds, aOptions.mSources);
    return;
  }

  auto* content = ContentChild::GetSingleton();
  if (NS_WARN_IF(!content)) {
    return;
  }
  content->SendAttributionConversion(
      source, aOptions.mTask, aOptions.mHistogramSize,
      aOptions.mLookbackDays.WasPassed() ? Some(aOptions.mLookbackDays.Value())
                                         : Nothing(),
      aOptions.mImpression.WasPassed() ? Some(aOptions.mImpression.Value())
                                       : Nothing(),
      aOptions.mAds, aOptions.mSources);
}

double PrivateAttribution::GetBudget(const nsACString& filterType,
                                     int64_t epochId, const nsACString& uri) {
  if (XRE_IsParentProcess()) {
    nsCOMPtr<nsIPrivateAttributionService> pa =
        components::PrivateAttribution::Service();
    if (NS_WARN_IF(!pa)) {
      return -1.0;
    }

    double out = -1.0;
    pa->GetBudget(filterType, epochId, uri, &out);
    return out;
  }

  auto* content = ContentChild::GetSingleton();
  if (NS_WARN_IF(!content)) {
    return -1.0;
  }
  
  double budget = -1.0;
  if (!content->SendGetBudget(filterType, epochId, uri, &budget)) {
    return -1.0;
  }
  return budget;
}

void PrivateAttribution::ClearEvents() {
  if (XRE_IsParentProcess()) {
    nsCOMPtr<nsIPrivateAttributionService> pa =
        components::PrivateAttribution::Service();
    if (NS_WARN_IF(!pa)) {
      return;
    }

    pa->ClearEvents();
  }

  auto* content = ContentChild::GetSingleton();
  if (NS_WARN_IF(!content)) {
    return;
  }
  content->SendClearEvents();
}

void PrivateAttribution::AddMockEvent(uint64_t aEpoch,
                                      const nsACString& aSourceUri,
                                      const nsTArray<nsCString>& aTriggerUris,
                                      const nsTArray<nsCString>& aQuerierUris) {
  if (XRE_IsParentProcess()) {
    nsCOMPtr<nsIPrivateAttributionService> pa =
        components::PrivateAttribution::Service();
    if (NS_WARN_IF(!pa)) {
      return;
    }

    pa->AddMockEvent(aEpoch, aSourceUri, aTriggerUris, aQuerierUris);
  }

  auto* content = ContentChild::GetSingleton();
  if (NS_WARN_IF(!content)) {
    return;
  }
  content->SendAddMockEvent(aEpoch, aSourceUri, aTriggerUris, aQuerierUris);
}

void PrivateAttribution::ComputeReportFor(
    const nsACString& triggerUri, const nsTArray<nsCString>& sourceUris,
    const nsTArray<nsCString>& querierUris) {
  if (XRE_IsParentProcess()) {
    nsCOMPtr<nsIPrivateAttributionService> pa =
        components::PrivateAttribution::Service();
    if (NS_WARN_IF(!pa)) {
      return;
    }

    pa->ComputeReportFor(triggerUri, sourceUris, querierUris);
  }

  auto* content = ContentChild::GetSingleton();
  if (NS_WARN_IF(!content)) {
    return;
  }
  content->SendComputeReportFor(triggerUri, sourceUris, querierUris);
}

}  // namespace mozilla::dom
