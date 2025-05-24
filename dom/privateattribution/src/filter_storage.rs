use std::ops::{Deref, DerefMut};

use log::{debug, trace};
use nsstring::{nsCString, nsString};
use pdslib::{
    budget::{
        pure_dp_filter::{PureDPBudget, PureDPBudgetFilter},
        traits::FilterStorage,
    },
    pds::quotas::{FilterId, StaticCapacities},
};
use storage::Conn;
use xpcom::{
    getter_addrefs,
    interfaces::{mozIStorageService, nsIFile, nsIProperties},
    RefPtr, XpCom,
};

use crate::uri::MozUri;

pub struct SqliteFilterStorage {
    capacities: StaticCapacities<FilterId<u64, MozUri>, PureDPBudget>,
    conn: Conn,
}

impl SqliteFilterStorage {
    fn db_file() -> Result<RefPtr<nsIFile>, anyhow::Error> {
        let dir_service =
            xpcom::get_service::<nsIProperties>(c"@mozilla.org/file/directory_service;1").unwrap();

        let db_file: RefPtr<nsIFile> = getter_addrefs(|p| unsafe {
            dir_service.Get(
                c"ProfD".as_ptr(),
                &nsIFile::IID,
                p as *mut *mut std::ffi::c_void,
            )
        })?;

        let cookies_file = nsCString::from("pdslib.sqlite");
        unsafe { db_file.AppendNative(cookies_file.deref()) };

        let mut db_file_str = nsString::new();
        unsafe { db_file.GetPath(db_file_str.deref_mut()) }.to_result()?;
        debug!("db_file path: {db_file_str}");

        Ok(db_file)
    }

    pub fn clear_db(&self) -> Result<(), anyhow::Error> {
        trace!("Clearing filters database");

        let query = "DELETE FROM filter";
        trace!("Executing query: {query}");

        let mut stmt = self.conn.prepare(query)?;
        stmt.execute()?;
        trace!("Database cleared successfully");

        Ok(())
    }
}

impl FilterStorage for SqliteFilterStorage {
    type FilterId = FilterId<u64, MozUri>;
    type Budget = PureDPBudget;
    type Filter = PureDPBudgetFilter;
    type Capacities = StaticCapacities<Self::FilterId, Self::Budget>;
    type Error = anyhow::Error;

    fn new(capacities: Self::Capacities) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        trace!("SqliteFilterStorage::new");

        let storage =
            xpcom::get_service::<mozIStorageService>(c"@mozilla.org/storage/service;1").unwrap();

        let db_file = Self::db_file()?;

        let conn = getter_addrefs(|p| unsafe {
            storage.OpenUnsharedDatabase(db_file.deref(), mozIStorageService::CONNECTION_DEFAULT, p)
        })
        .unwrap();
        let conn = Conn::wrap(conn);
        trace!("Opened unshared database and got connection");

        let this = Self { capacities, conn };

        // create filter table
        let query = "CREATE TABLE IF NOT EXISTS filter (
                id TEXT PRIMARY KEY,
                budget REAL NOT NULL,
                capacity REAL
            )";
        trace!("Creating filter table with query: {query}");

        this.conn.exec(query)?;
        trace!("Filter table created successfully");

        Ok(this)
    }

    fn capacities(&self) -> &Self::Capacities {
        &self.capacities
    }

    fn get_filter(
        &mut self,
        filter_id: &Self::FilterId,
    ) -> Result<Option<Self::Filter>, Self::Error> {
        let filter_id_str = nsCString::from(format!("{filter_id:?}"));
        trace!("SqliteFilterStorage::get_filter(filter_id={filter_id_str})");

        let query = "SELECT budget, capacity FROM filter WHERE id = :id";
        trace!("Getting filter with query: {query}\nfilter_id: {filter_id_str}");

        let mut stmt = self.conn.prepare(query)?;
        stmt.bind_by_name("id", filter_id_str.clone())?;

        let Some(row) = stmt.step()? else {
            trace!("Filter ID not present in the database: {filter_id_str}");
            return Ok(None);
        };

        let budget_value: f64 = row.get_by_name("budget")?;
        let budget = Self::Budget::from(budget_value);
        let capacity_value: Option<f64> = row.get_by_name("capacity")?;
        let capacity = capacity_value.map(Self::Budget::from);

        trace!("Filter retrieved successfully: {filter_id_str}, budget: {budget_value}, capacity: {capacity_value:?}");

        let filter = PureDPBudgetFilter {
            consumed: budget,
            capacity,
        };
        Ok(Some(filter))
    }

    fn set_filter(
        &mut self,
        filter_id: &Self::FilterId,
        filter: Self::Filter,
    ) -> Result<(), Self::Error> {
        let filter_id_str = nsCString::from(format!("{filter_id:?}"));
        trace!("SqliteFilterStorage::set_filter(filter_id={filter_id_str})");

        let budget_value: f64 = filter.consumed;
        let capacity_value: Option<f64> = filter.capacity.map(|c| c.into());

        let query =
            "INSERT OR REPLACE INTO filter (id, budget, capacity) VALUES (:id, :budget, :capacity)";
        trace!("Setting filter with query: {query}\nfilter_id: {filter_id_str}\nbudget: {budget_value}\ncapacity: {capacity_value:?}");

        {
            let mut stmt = self.conn.prepare(query)?;
            stmt.bind_by_name("id", filter_id_str.clone())?;
            stmt.bind_by_name("budget", budget_value)?;
            stmt.bind_by_name("capacity", capacity_value)?;
            stmt.execute()?;
        }

        trace!("Filter set operation completed for filter_id: {filter_id_str}");
        Ok(())
    }
}
