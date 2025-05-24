/* -*- Mode: C++; tab-width: 2; indent-tabs-mode: nil; c-basic-offset: 2 -*- */
/* vim:set ts=2 sw=2 sts=2 et cindent: */
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef mozilla_dom_PrivateAttribution_h
#define mozilla_dom_PrivateAttribution_h

#include "nsTArray.h"
#include "nsWrapperCache.h"
#include "nsCOMPtr.h"

class nsIGlobalObject;
namespace mozilla {
class ErrorResult;
}

namespace mozilla::dom {

struct PrivateAttributionImpressionOptions;
struct PrivateAttributionConversionOptions;

class PrivateAttribution final : public nsWrapperCache {
 public:
  NS_INLINE_DECL_CYCLE_COLLECTING_NATIVE_REFCOUNTING(PrivateAttribution)
  NS_DECL_CYCLE_COLLECTION_NATIVE_WRAPPERCACHE_CLASS(PrivateAttribution)

  explicit PrivateAttribution(nsIGlobalObject* aGlobal);
  static already_AddRefed<PrivateAttribution> Create(nsIGlobalObject& aGlobal);

  nsIGlobalObject* GetParentObject() const { return mOwner; }
  JSObject* WrapObject(JSContext*, JS::Handle<JSObject*> aGivenProto) override;

  void SaveImpression(const PrivateAttributionImpressionOptions&, ErrorResult&);
  void MeasureConversion(const PrivateAttributionConversionOptions&,
                         ErrorResult&);
  void AddMockEvent(uint64_t aIndex, uint64_t aTimestamp,
                    const nsACString& aSourceHost,
                    const nsACString& aTargetHost, const nsAString& aAd,
                    ErrorResult& aRv);
  void ComputeReportFor(const nsACString& aTargetHost,
                        const nsTArray<nsCString>& aSourceHosts,
                        uint64_t aHistogramSize, uint64_t aLookbackDays,
                        const nsAString& aAd, ErrorResult& aRv);
  [[nodiscard]] double GetBudget(const nsACString& aFilterType,
                                 uint64_t aEpochId, const nsACString& aUri,
                                 ErrorResult& aRv);
  void ClearBudgets(ErrorResult& aRv);

 private:
  static bool ShouldRecord();

  [[nodiscard]] bool GetSourceHostIfNonPrivate(nsACString&, ErrorResult&);

  ~PrivateAttribution();

  nsCOMPtr<nsIGlobalObject> mOwner;
};

}  // namespace mozilla::dom

// Implemented in Rust.
extern "C" {
nsresult nsPrivateAttributionPdslibConstructor(REFNSIID aIID, void** aResult);
}  // extern "C"

#endif  // mozilla_dom_PrivateAttribution_h
