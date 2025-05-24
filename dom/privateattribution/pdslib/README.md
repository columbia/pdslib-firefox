# Private Data Service Library

`pdslib` is a Rust library that implements on-device DP budgeting for privacy-preserving attribution measurement APIs such as [PPA](https://w3c.github.io/ppa/). It aims to remain more generic than Web advertising use cases, exposing a relatively abstract interface for: (1) storing events (such as impressions for PPA but also other personal-data-events like locations the user has visited previously) and (2) requesting reports computed based on previously stored events (such as conversion attribution reports for PPA but also whether the user visited a particular location previously).

## State and versions

The library is currently under active development and is highly experimental. We are making available a first version of it -- tagged [v0.2 - Cookie Monster](https://github.com/columbia/pdslib/tree/19eee219404e90b8529138137e3f8430f06a78ee) -- which implements the [Cookie Monster algorithm](https://arxiv.org/abs/2405.16719) for individual privacy loss accounting (Section 3.3 of the linked paper). The purpose of this public release is to help with understanding of the algorithm in the paper, however please note that because the repository is still very much under development and experimental, we urge that you do not rely on it for implementations of PPA.

File [src/pds/epoch_pds.rs](https://github.com/columbia/pdslib/blob/19eee219404e90b8529138137e3f8430f06a78ee/src/pds/epoch_pds.rs) implements the Cookie Monster algorithm. A test case that shows how to use the library in the context of PPA Level 1 is in [tests/ppa_workflow.rs](https://github.com/columbia/pdslib/blob/19eee219404e90b8529138137e3f8430f06a78ee/tests/ppa_workflow.rs). This version of pdslib implements privacy loss accounting against a single privacy filter (a.k.a., `budget`), for example for a single advertiser. To support multiple privacy filters (such as one per advertiser, as dictated by the Cookie Monster paper and in PPA Level 1), one would instantiate multiple pdslibs. No support is provided in this version for management of these different pdslib instances. 

The next version of the library -- currently under development -- will implement our extensions to Cookie Monster to support and manage multiple privacy filters. These extensions, required in our opinion for a future PPA Level 2 API, will involve management of these filters to preserve both user privacy and isolation among queriers competing for privacy budget on user devices.

## Repository structure
- `src` contains the following main components: `budget`, `events`, `mechanisms` (no dependencies), `queries` (depends on `budget`, `events`, `mechanisms`) and `pds` (depends on the rest).
- `src/*/traits.rs` define interfaces. Other files in `src/*` implement these interfaces, with very simple in-memory datastructures for now. Other crates using pdslib in particular environments (e.g., Chromium or Android) can have implementations for the same traits using browser storage or SQLite databases.
- `src/pds` is structured to work with  `budget`, `events`, `queries` only through interfaces. So we should be able to swap the implementation for event storage or replace the type of query, and still obtain a working implementation of the `PrivateDataService` interface.
- `tests` contains integration tests. In particular, `tests/*_demo.rs` show how an external application can use pdslib to register events and request different types of reports on a device. 

## Logging
The pdslib library uses the log4rs library for logging.

### Configure Logging
The configuration file can be found at `./logging_config.yaml`. You can change the lowest log level, by modifying the level attribute to one of: trace, debug, info, warn, error. The configuration file contains options to log to the console and/or to write to a file `log/pdslib.log`. By default, it is set to log to the console, but you can modify this by adding or replacing appenders.

### Initialize Logging
To enable logging, call `pdslib::util::log_util::init()`. To log, use the functions: `trace!()`, `debug!()`, `info!()`, `warn!()`, `error!()`.
