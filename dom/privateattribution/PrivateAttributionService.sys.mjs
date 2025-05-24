/* vim: set ts=2 sw=2 sts=2 et tw=80: */
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

const lazy = {};

import { XPCOMUtils } from "resource://gre/modules/XPCOMUtils.sys.mjs";
import { AppConstants } from "resource://gre/modules/AppConstants.sys.mjs";

ChromeUtils.defineESModuleGetters(lazy, {
  IndexedDB: "resource://gre/modules/IndexedDB.sys.mjs",
  DAPTelemetrySender: "resource://gre/modules/DAPTelemetrySender.sys.mjs",
  HPKEConfigManager: "resource://gre/modules/HPKEConfigManager.sys.mjs",
  setTimeout: "resource://gre/modules/Timer.sys.mjs",
});

XPCOMUtils.defineLazyPreferenceGetter(
  lazy,
  "gIsTelemetrySendingEnabled",
  "datareporting.healthreport.uploadEnabled",
  true
);

XPCOMUtils.defineLazyPreferenceGetter(
  lazy,
  "gIsPPAEnabled",
  "dom.private-attribution.submission.enabled",
  true
);

XPCOMUtils.defineLazyPreferenceGetter(
  lazy,
  "gOhttpRelayUrl",
  "toolkit.shopping.ohttpRelayURL"
);
XPCOMUtils.defineLazyPreferenceGetter(
  lazy,
  "gOhttpGatewayKeyUrl",
  "toolkit.shopping.ohttpConfigURL"
);

const DAY_IN_MILLI = 1000 * 60 * 60 * 24;
const EPOCH_DURATION = 7 * DAY_IN_MILLI;
const DAP_TIMEOUT_MILLI = 30000;

/**
 *
 */
export class PrivateAttributionService {
  constructor({
    dapTelemetrySender,
    dateProvider,
    testForceEnabled,
    testDapOptions,
  } = {}) {
    this._dapTelemetrySender = dapTelemetrySender;
    this._dateProvider = dateProvider ?? Date;
    this._testForceEnabled = testForceEnabled;
    this._testDapOptions = testDapOptions;

    this.dbName = "PrivateAttribution";
    this.impressionStoreName = "impressions";
    this.storeNames = [this.impressionStoreName];
    this.dbVersion = 1;
  }

  get dapTelemetrySender() {
    return this._dapTelemetrySender || lazy.DAPTelemetrySender;
  }

  now() {
    return this._dateProvider.now();
  }

  async onAttributionEvent(sourceHost, type, index, ad, targetHost) {
    if (!this.isEnabled()) {
      return;
    }

    const now = this.now();

    try {
      const impressionStore = await this.getImpressionStore();

      const epoch = this.timestampToEpoch(now);
      const impression = {
        index,
        sourceHost,
        targetHost,
        timestamp: now,
        epoch,
        ad,
      };

      await this.addNewImpression(impressionStore, impression);
    } catch (e) {
      console.error(e);
    }
  }

  async onAttributionConversion(
    targetHost,
    task,
    histogramSize,
    lookbackDays,
    impressionType,
    ads,
    sourceHosts
  ) {
    if (!this.isEnabled()) {
      return;
    }

    const now = this.now();

    try {
      const impressionStore = await this.getImpressionStore();

      const nowEpoch = this.timestampToEpoch(now);
      const lookbackDaysEpoch = this.daysAgoToEpoch(now, lookbackDays);
      for (let epoch = lookbackDaysEpoch; epoch <= nowEpoch; epoch++) {
        const impressions = await this.getImpressions(impressionStore, epoch);

        const relevantImpressions = await this.filterRelevantImpressions(
          impressions,
          ads,
          sourceHosts,
          targetHost
        );

        // format impressions as pdslib events
        const events = relevantImpressions.map(impression => ({
          timestamp: impression.timestamp,
          epochNumber: impression.epoch,
          histogramIndex: impression.index,
          sourceHost: impression.sourceHost,
          triggerHosts: [impression.targetHost],
          intermediaryHosts: [impression.targetHost],
          querierHosts: [impression.targetHost],
        }));

        const request = {
          startEpoch: lookbackDaysEpoch,
          endEpoch: nowEpoch,
          attributableValue: 100.0,
          maxAttributableValue: 200.0,
          requestedEpsilon: 1.0,
          histogramSize,
          triggerHost: targetHost,
          sourceHosts,
          intermediaryHosts: [],
          querierHosts: [targetHost],
        };

        const report = this.pdslib.computeReport(request, events);
        console.log("Pdslib report:", report);
      }
    } catch (e) {
      console.error(e);
    }
  }

  async addMockEvent(index, timestamp, sourceHost, targetHost, ad) {
    let impression = {
      index,
      sourceHost,
      targetHost,
      timestamp,
      epoch: this.timestampToEpoch(timestamp),
      ad,
    };

    const impressionStore = await this.getImpressionStore();
    await this.addNewImpression(impressionStore, impression);
  }

  async computeReportFor(
    targetHost,
    sourceHosts,
    histogramSize,
    lookbackDays,
    ad
  ) {
    await this.onAttributionConversion(
      targetHost,
      "", // task id
      histogramSize,
      lookbackDays,
      "", // impression type
      [ad],
      sourceHosts
    );
  }

  getBudget(...args) {
    return this.pdslib.getBudget(...args);
  }

  async clearBudgets(...args) {
    this.pdslib.clearBudgets(...args);

    // also clear impressions
    const impressionStore = await this.getImpressionStore();
    await impressionStore.clear();
  }

  timestampToEpoch(timestamp) {
    return Math.floor(timestamp / EPOCH_DURATION);
  }

  daysAgoToEpoch(now, daysAgo) {
    const daysAgoInMillis = daysAgo * DAY_IN_MILLI;
    const targetTime = now - daysAgoInMillis;
    return this.timestampToEpoch(targetTime);
  }

  async addNewImpression(impressionStore, impression) {
    const impressions = (await impressionStore.get(impression.epoch)) ?? [];
    impressions.push(impression);
    await impressionStore.put(impressions, impression.epoch);
  }

  async getImpressions(impressionStore, epoch) {
    return (await impressionStore.get(epoch)) ?? [];
  }

  async filterRelevantImpressions(impressions, ads, sourceHosts, targetHost) {
    return impressions.filter(
      impression =>
        ads.includes(impression.ad) &&
        targetHost === impression.targetHost &&
        (!sourceHosts || sourceHosts.includes(impression.sourceHost))
    );
  }

  async getImpressionStore() {
    return await this.getStore(this.impressionStoreName);
  }

  async getStore(storeName) {
    return (await this.db).objectStore(storeName, "readwrite");
  }

  get db() {
    return this._db || (this._db = this.createOrOpenDb());
  }

  async createOrOpenDb() {
    try {
      return await this.openDatabase();
    } catch {
      await lazy.IndexedDB.deleteDatabase(this.dbName);
      return this.openDatabase();
    }
  }

  async openDatabase() {
    return await lazy.IndexedDB.open(this.dbName, this.dbVersion, db => {
      this.storeNames.forEach(store => {
        if (!db.objectStoreNames.contains(store)) {
          db.createObjectStore(store);
        }
      });
    });
  }

  get pdslib() {
    return this._pdslib || (this._pdslib = this.getPdslibService());
  }

  getPdslibService() {
    return Cc["@mozilla.org/private-attribution-pdslib;1"].getService(
      Ci.nsIPrivateAttributionPdslibService
    );
  }

  async sendDapReport(id, index, size, value) {
    const task = {
      id,
      time_precision: 60,
      measurement_type: "vecu8",
    };

    const measurement = new Array(size).fill(0);
    measurement[index] = value;

    let options = {
      timeout: DAP_TIMEOUT_MILLI,
      ohttp_relay: lazy.gOhttpRelayUrl,
      ...this._testDapOptions,
    };

    if (options.ohttp_relay) {
      // Fetch the OHTTP-Gateway-HPKE key if not provided yet.
      if (!options.ohttp_hpke) {
        const controller = new AbortController();
        lazy.setTimeout(() => controller.abort(), DAP_TIMEOUT_MILLI);

        options.ohttp_hpke = await lazy.HPKEConfigManager.get(
          lazy.gOhttpGatewayKeyUrl,
          {
            maxAge: DAY_IN_MILLI,
            abortSignal: controller.signal,
          }
        );
      }
    } else if (!this._testForceEnabled) {
      // Except for testing, do no allow PPA to bypass OHTTP.
      throw new Error("PPA requires an OHTTP relay for submission");
    }

    await this.dapTelemetrySender.sendDAPMeasurement(
      task,
      measurement,
      options
    );
  }

  getModelProp(type) {
    return this.models[type ? type : "default"];
  }

  isEnabled() {
    return (
      this._testForceEnabled ||
      (lazy.gIsTelemetrySendingEnabled &&
        AppConstants.MOZ_TELEMETRY_REPORTING &&
        lazy.gIsPPAEnabled)
    );
  }

  QueryInterface = ChromeUtils.generateQI([Ci.nsIPrivateAttributionService]);
}
